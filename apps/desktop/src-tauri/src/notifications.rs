use orchestrator_ipc::{Capability, Relay, RelayError, RelayEvent};
use orchestrator_private_fs::{create_private_dir, create_private_file, is_owner_only};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::{NotificationExt, PermissionState};

const SETTINGS_FILE: &str = "decision-notifications.json";
const MAX_STATE_BYTES: u64 = 1_048_576;
const MAX_MARKERS: usize = 4096;
const MAX_APPROVAL_ID_BYTES: usize = 256;
/// Owner decision 72334 (DPES-856): an unanswered decision is reminded once it is 4 hours old,
/// then every 4 hours until it is answered or deferred.
const DEFAULT_INTERVAL_MINUTES: u16 = 240;
const MIN_INTERVAL_MINUTES: u16 = 5;
const MAX_INTERVAL_MINUTES: u16 = 1440;
const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(15);
const AGGREGATE_WINDOW: Duration = Duration::from_secs(1);
const RETRY_DELAY: Duration = Duration::from_secs(2);
const EVENT_CAPACITY: usize = 2048;
/// The notifier only reads. Every request it sends must be on this list, so no code path here
/// can grant, decide, approve or reject an approval (`approvals.resolve`, `review.*`).
const READ_ONLY_METHODS: [&str; 3] = ["events.subscribe", "approvals.list", "queue.snapshot"];
static TEMP_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Settings {
    pub enabled: bool,
    pub reminder_interval_minutes: Option<u16>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            reminder_interval_minutes: Some(DEFAULT_INTERVAL_MINUTES),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Marker {
    /// When the decision was requested, from a real journal event timestamp. `None` when no real
    /// timestamp is known (`approvals.list` carries none): the age is never invented, so such a
    /// decision is announced once and never reminded by age.
    #[serde(default)]
    requested_at: Option<u64>,
    last_notified_at: Option<u64>,
    last_reminded_at: Option<u64>,
    new_notification_attempted: bool,
    /// The decision was answered (approved, rejected, superseded or changes requested).
    acknowledged: bool,
    /// Local only: the user silenced reminders for this decision. It stays pending.
    #[serde(default)]
    silenced: bool,
    /// Local only: no reminder before this time (unix seconds). The decision stays pending.
    #[serde(default)]
    deferred_until: Option<u64>,
}

impl Default for Permission {
    fn default() -> Self {
        Self::Unavailable
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DurableState {
    version: u8,
    settings: Settings,
    markers: BTreeMap<String, Marker>,
}

impl Default for DurableState {
    fn default() -> Self {
        Self {
            version: 1,
            settings: Settings::default(),
            markers: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    Granted,
    Denied,
    Prompt,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Status {
    pub enabled: bool,
    pub reminder_interval_minutes: Option<u16>,
    pub permission: Permission,
    pub pending_count: Option<usize>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetRequest {
    enabled: bool,
    reminder_interval_minutes: Option<u16>,
    request_permission: bool,
}
fn parse_set_request(input: Value) -> Result<SetRequest, &'static str> {
    if input.get("reminder_interval_minutes").is_none() {
        return Err("Invalid notification settings.");
    }
    let request = serde_json::from_value::<SetRequest>(input)
        .map_err(|_| "Invalid notification settings.")?;
    if !valid_interval(request.reminder_interval_minutes) {
        return Err("Reminder interval must be 5–1440 minutes or off.");
    }
    Ok(request)
}

fn apply_settings(
    path: &Path,
    state: &mut DurableState,
    request: &SetRequest,
) -> Result<bool, &'static str> {
    let updated = Settings {
        enabled: request.enabled,
        reminder_interval_minutes: request.reminder_interval_minutes,
    };
    if state.settings == updated {
        return Ok(false);
    }
    let mut next = state.clone();
    next.settings = updated;
    save_state(path, &next).map_err(|_| "Unable to save notification settings.")?;
    *state = next;
    Ok(true)
}
fn update_settings(
    path: &Path,
    state: &mut DurableState,
    input: Value,
) -> Result<SetRequest, &'static str> {
    let request = parse_set_request(input)?;
    apply_settings(path, state, &request)?;
    Ok(request)
}

/// A local action on the current notice. It changes only this app's reminder state; the decisions
/// stay pending and no daemon request is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoticeAction {
    /// Silence reminders for the decisions pending now.
    Acknowledge,
    /// Postpone reminders for the decisions pending now by this many minutes.
    Defer(u16),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoticeRequest {
    action: String,
    minutes: Option<u16>,
}

fn parse_notice_request(input: Value) -> Result<NoticeAction, &'static str> {
    let request = serde_json::from_value::<NoticeRequest>(input)
        .map_err(|_| "Invalid notification action.")?;
    match (request.action.as_str(), request.minutes) {
        ("acknowledge", None) => Ok(NoticeAction::Acknowledge),
        ("defer", Some(minutes)) if valid_interval(Some(minutes)) => {
            Ok(NoticeAction::Defer(minutes))
        }
        ("defer", _) => Err("Defer must be 5–1440 minutes."),
        _ => Err("Invalid notification action."),
    }
}

/// Applies a local notice action to the decisions that are pending and unanswered. Returns whether
/// anything changed. It never touches `acknowledged` (answered) or the pending set.
fn apply_notice_action(
    markers: &mut BTreeMap<String, Marker>,
    pending: &BTreeSet<String>,
    action: NoticeAction,
    now: u64,
) -> bool {
    let mut changed = false;
    for id in pending {
        let Some(marker) = markers.get_mut(id).filter(|marker| !marker.acknowledged) else {
            continue;
        };
        match action {
            NoticeAction::Acknowledge if !marker.silenced => {
                marker.silenced = true;
                changed = true;
            }
            NoticeAction::Defer(minutes) => {
                let until = now.saturating_add(u64::from(minutes) * 60);
                if marker.deferred_until != Some(until) {
                    marker.deferred_until = Some(until);
                    changed = true;
                }
            }
            NoticeAction::Acknowledge => {}
        }
    }
    changed
}

#[derive(Default)]
struct RuntimeState {
    permission: Permission,
    pending_count: Option<usize>,
    error: Option<String>,
    pending_ids: BTreeSet<String>,
    /// Pending decisions whose task is waiting on a dependency (excluded from reminders).
    /// `None` when that is unknown, which also pauses reminders.
    held_ids: Option<BTreeSet<String>>,
}

struct Inner {
    app: AppHandle,
    path: PathBuf,
    data_dir: Option<PathBuf>,
    durable: Mutex<DurableState>,
    runtime: Mutex<RuntimeState>,
    stop: AtomicBool,
    worker: Mutex<Option<JoinHandle<()>>>,
    last_published: Mutex<Option<Status>>,
}

#[derive(Clone)]
pub struct Service(Arc<Inner>);

impl Service {
    pub fn new(app: AppHandle, data_dir: Option<PathBuf>) -> Result<Self, String> {
        let config_dir = app
            .path()
            .app_config_dir()
            .map_err(|_| "Unable to access private notification settings.".to_owned())?;
        create_private_dir(&config_dir)
            .map_err(|_| "Unable to protect notification settings.".to_owned())?;
        let path = config_dir.join(SETTINGS_FILE);
        let (durable, load_error) = load_state(&path);
        let runtime = RuntimeState {
            permission: Permission::Unavailable,
            pending_count: None,
            error: load_error,
            pending_ids: BTreeSet::new(),
            held_ids: None,
        };
        Ok(Self(Arc::new(Inner {
            app,
            path,
            data_dir,
            durable: Mutex::new(durable),
            runtime: Mutex::new(runtime),
            stop: AtomicBool::new(false),
            worker: Mutex::new(None),
            last_published: Mutex::new(None),
        })))
    }

    pub fn start(&self) -> Result<(), String> {
        let inner = Arc::clone(&self.0);
        let worker = thread::Builder::new()
            .name("decision-notifications".into())
            .spawn(move || run(inner))
            .map_err(|_| "Unable to start decision notifications.".to_owned())?;
        *self.0.worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(worker);
        Ok(())
    }

    pub fn stop(&self) {
        self.0.stop.store(true, Ordering::Release);
        if let Some(worker) = self
            .0
            .worker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = worker.join();
        }
    }

    pub fn get(&self) -> Status {
        refresh_permission(&self.0);
        let snapshot = status(&self.0);
        publish(&self.0, &snapshot)
    }

    pub fn set(&self, input: Value) -> Status {
        let mut durable = self.0.durable.lock().unwrap_or_else(|e| e.into_inner());
        let request = match update_settings(&self.0.path, &mut durable, input) {
            Ok(request) => request,
            Err(message) => {
                drop(durable);
                return fail_status(&self.0, message);
            }
        };
        drop(durable);
        let mut runtime = self.0.runtime.lock().unwrap_or_else(|e| e.into_inner());
        if runtime.error.as_deref() == Some("Unable to save notification settings.") {
            runtime.error = None;
        }
        drop(runtime);
        if request.request_permission {
            let permission = match self.0.app.notification().request_permission() {
                Ok(state) => permission(state),
                Err(_) => {
                    set_error(
                        &self.0,
                        Some("Unable to request OS notification permission."),
                    );
                    Permission::Unavailable
                }
            };
            self.0
                .runtime
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .permission = permission;
        }
        refresh_permission(&self.0);
        let snapshot = status(&self.0);
        publish(&self.0, &snapshot)
    }
}

impl Service {
    /// Local acknowledge/defer of the current notice. Touches only the private reminder state.
    pub fn notice(&self, input: Value) -> Status {
        let action = match parse_notice_request(input) {
            Ok(action) => action,
            Err(message) => return fail_status(&self.0, message),
        };
        let mut durable = self.0.durable.lock().unwrap_or_else(|e| e.into_inner());
        let pending = self
            .0
            .runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending_ids
            .clone();
        let mut next = durable.clone();
        if apply_notice_action(&mut next.markers, &pending, action, unix_now()) {
            if save_state(&self.0.path, &next).is_err() {
                drop(durable);
                return fail_status(&self.0, "Unable to save notification history.");
            }
            *durable = next;
        }
        drop(durable);
        let snapshot = status(&self.0);
        publish(&self.0, &snapshot)
    }
}

fn unavailable_status(error: &str) -> Status {
    Status {
        enabled: false,
        reminder_interval_minutes: Some(DEFAULT_INTERVAL_MINUTES),
        permission: Permission::Unavailable,
        pending_count: None,
        error: Some(error.to_owned()),
    }
}

#[tauri::command]
pub fn decision_notifications_get(service: tauri::State<'_, Result<Service, String>>) -> Status {
    match service.inner() {
        Ok(service) => service.get(),
        Err(error) => unavailable_status(error),
    }
}

#[tauri::command]
pub fn decision_notifications_set(
    service: tauri::State<'_, Result<Service, String>>,
    settings: Value,
) -> Status {
    match service.inner() {
        Ok(service) => service.set(settings),
        Err(error) => unavailable_status(error),
    }
}

#[tauri::command]
pub fn decision_notifications_notice(
    service: tauri::State<'_, Result<Service, String>>,
    notice: Value,
) -> Status {
    match service.inner() {
        Ok(service) => service.notice(notice),
        Err(error) => unavailable_status(error),
    }
}

fn load_state(path: &Path) -> (DurableState, Option<String>) {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() && metadata.len() <= MAX_STATE_BYTES => {
            metadata
        }
        Ok(_) => {
            return (
                DurableState::default(),
                Some("Notification settings are unreadable.".into()),
            )
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (DurableState::default(), None)
        }
        Err(_) => {
            return (
                DurableState::default(),
                Some("Notification settings are unavailable.".into()),
            )
        }
    };
    if !is_owner_only(path).unwrap_or(false) {
        return (
            DurableState::default(),
            Some("Notification settings are not private.".into()),
        );
    }
    let file = match OpenOptions::new().read(true).open(path) {
        Ok(file) => file,
        Err(_) => {
            return (
                DurableState::default(),
                Some("Notification settings are unavailable.".into()),
            )
        }
    };
    let opened = match file.metadata() {
        Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_STATE_BYTES => metadata,
        _ => {
            return (
                DurableState::default(),
                Some("Notification settings are unreadable.".into()),
            )
        }
    };
    if opened.len() != metadata.len() {
        return (
            DurableState::default(),
            Some("Notification settings changed while loading.".into()),
        );
    }
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    if file
        .take(MAX_STATE_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > MAX_STATE_BYTES
    {
        return (
            DurableState::default(),
            Some("Notification settings are unreadable.".into()),
        );
    }
    let parsed = serde_json::from_slice::<DurableState>(&bytes).ok();
    match parsed {
        Some(state)
            if state.version == 1
                && state.markers.len() <= MAX_MARKERS
                && state.markers.keys().all(|id| valid_id(id))
                && valid_interval(state.settings.reminder_interval_minutes) =>
        {
            (state, None)
        }
        _ => (
            DurableState::default(),
            Some("Notification settings are invalid; defaults are active.".into()),
        ),
    }
}

fn save_state(path: &Path, state: &DurableState) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(state).map_err(std::io::Error::other)?;
    if bytes.len() as u64 > MAX_STATE_BYTES {
        return Err(std::io::Error::other("notification state exceeds limit"));
    }
    let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = path.with_file_name(format!(
        "{SETTINGS_FILE}.{}.{}.{}.tmp",
        std::process::id(),
        stamp,
        id
    ));
    create_private_file(&temporary)?;
    let write_result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_APPROVAL_ID_BYTES && !id.chars().any(char::is_control)
}

fn valid_interval(interval: Option<u16>) -> bool {
    interval.is_none_or(|minutes| (MIN_INTERVAL_MINUTES..=MAX_INTERVAL_MINUTES).contains(&minutes))
}

fn status(inner: &Inner) -> Status {
    let durable = inner.durable.lock().unwrap_or_else(|e| e.into_inner());
    let runtime = inner.runtime.lock().unwrap_or_else(|e| e.into_inner());
    Status {
        enabled: durable.settings.enabled,
        reminder_interval_minutes: durable.settings.reminder_interval_minutes,
        permission: runtime.permission,
        pending_count: runtime.pending_count,
        error: runtime.error.clone(),
    }
}

fn fail_status(inner: &Inner, message: &str) -> Status {
    set_error(inner, Some(message));
    let snapshot = status(inner);
    publish(inner, &snapshot);
    snapshot
}

fn set_error(inner: &Inner, message: Option<&str>) {
    inner
        .runtime
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .error = message.map(str::to_owned);
}

fn permission(state: PermissionState) -> Permission {
    match state {
        PermissionState::Granted => Permission::Granted,
        PermissionState::Denied => Permission::Denied,
        PermissionState::Prompt | PermissionState::PromptWithRationale => Permission::Prompt,
    }
}

fn refresh_permission(inner: &Inner) {
    let (state, error) = match inner.app.notification().permission_state() {
        Ok(state) => (permission(state), None),
        Err(_) => (
            Permission::Unavailable,
            Some("OS notifications are unavailable."),
        ),
    };
    let mut runtime = inner.runtime.lock().unwrap_or_else(|e| e.into_inner());
    runtime.permission = state;
    if let Some(error) = error {
        runtime.error = Some(error.into());
    }
}

fn publish(inner: &Inner, snapshot: &Status) -> Status {
    {
        let mut last = inner
            .last_published
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if last.as_ref() == Some(snapshot) {
            return snapshot.clone();
        }
        *last = Some(snapshot.clone());
    }
    if let Err(message) = update_badge(inner, snapshot.pending_count) {
        set_error(inner, Some(message));
        let updated = status(inner);
        *inner
            .last_published
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(updated.clone());
        if inner
            .app
            .emit("decision-notifications-status", &updated)
            .is_err()
        {
            set_error(inner, Some("Unable to publish notification status."));
            *inner
                .last_published
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
            return status(inner);
        }
        return updated;
    }
    if inner
        .app
        .emit("decision-notifications-status", snapshot)
        .is_err()
    {
        set_error(inner, Some("Unable to publish notification status."));
        *inner
            .last_published
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        return status(inner);
    }
    snapshot.clone()
}

#[cfg(target_os = "windows")]
pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let icon = app
        .default_window_icon()
        .ok_or_else(|| std::io::Error::other("missing default window icon"))?
        .clone();
    tauri::tray::TrayIconBuilder::with_id("decision-notifications")
        .icon(icon)
        .tooltip("Pending decision count unavailable")
        .build(app)?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn create_tray(_app: &AppHandle) -> tauri::Result<()> {
    Ok(())
}

#[cfg(any(target_os = "windows", test))]
fn tray_indicator_icon(
    base: &tauri::image::Image<'_>,
    count: Option<usize>,
) -> Result<tauri::image::Image<'static>, &'static str> {
    let width = base.width();
    let height = base.height();
    let mut rgba = base.rgba().to_vec();
    if rgba.len() != width as usize * height as usize * 4 {
        return Err("Windows tray icon data is invalid.");
    }
    if count != Some(0) {
        let color = if count.is_some() {
            [220, 40, 40, 255]
        } else {
            [245, 170, 20, 255]
        };
        let center_x = width as i32 - 4;
        let center_y = height as i32 - 4;
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let dx = x - center_x;
                let dy = y - center_y;
                if dx * dx + dy * dy <= 9 {
                    let offset = (y as usize * width as usize + x as usize) * 4;
                    rgba[offset..offset + 4].copy_from_slice(&color);
                }
            }
        }
    }
    Ok(tauri::image::Image::new_owned(rgba, width, height))
}

fn update_badge(inner: &Inner, count: Option<usize>) -> Result<(), &'static str> {
    #[cfg(target_os = "macos")]
    {
        let app = inner.app.clone();
        let label = match count {
            None => Some("?".to_owned()),
            Some(0) => None,
            Some(count) => Some(count.to_string()),
        };
        app.run_on_main_thread(move || {
            use objc2::MainThreadMarker;
            use objc2_app_kit::NSApplication;
            use objc2_foundation::NSString;
            let marker = MainThreadMarker::new().expect("AppKit updates run on the main thread");
            let tile = NSApplication::sharedApplication(marker).dockTile();
            let label = label.map(|value| NSString::from_str(&value));
            tile.setBadgeLabel(label.as_deref());
        })
        .map_err(|_| "Unable to update the macOS Dock badge.")?;
    }
    #[cfg(target_os = "windows")]
    {
        let tray = inner
            .app
            .tray_by_id("decision-notifications")
            .ok_or("Windows tray icon is unavailable.")?;
        let base = inner
            .app
            .default_window_icon()
            .ok_or("Windows tray icon is unavailable.")?;
        let icon = tray_indicator_icon(base, count)?;
        tray.set_icon(Some(icon))
            .map_err(|_| "Unable to update the Windows tray indicator.")?;
        let tooltip = match count {
            Some(0) => "No pending decisions".to_owned(),
            Some(1) => "1 pending decision".to_owned(),
            Some(count) => format!("{count} pending decisions"),
            None => "Pending decision count unavailable".to_owned(),
        };
        tray.set_tooltip(Some(&tooltip))
            .map_err(|_| "Unable to update the Windows tray count.")?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (inner, count);
        return Err("Decision badges are unavailable on this platform.");
    }
    Ok(())
}

fn invalidate_pending_state(runtime: &mut RuntimeState) {
    runtime.pending_count = None;
    runtime.pending_ids.clear();
    runtime.held_ids = None;
}

fn enqueue_relay_event(
    sender: &mpsc::SyncSender<RelayEvent>,
    overflowed: &AtomicBool,
    event: RelayEvent,
) {
    if sender.try_send(event).is_err() {
        overflowed.store(true, Ordering::Release);
    }
}

fn mark_unknown(inner: &Inner, message: &str) {
    {
        let mut runtime = inner.runtime.lock().unwrap_or_else(|e| e.into_inner());
        invalidate_pending_state(&mut runtime);
        runtime.error = Some(message.to_owned());
    }
    publish(inner, &status(inner));
}

fn run(inner: Arc<Inner>) {
    refresh_permission(&inner);
    // The first snapshot of this process is the baseline: decisions already pending at app start
    // are recorded silently. Every later snapshot, including the first one after a reconnect, is
    // not: a decision that appeared while disconnected is announced.
    let mut baseline_done = false;
    loop {
        if inner.stop.load(Ordering::Acquire) {
            return;
        }
        let Some(data_dir) = inner.data_dir.as_ref() else {
            mark_unknown(&inner, "Local daemon data directory is unavailable.");
            sleep_or_stop(&inner, RETRY_DELAY);
            continue;
        };
        let (sender, receiver) = mpsc::sync_channel(EVENT_CAPACITY);
        let overflowed = Arc::new(AtomicBool::new(false));
        let open = |capabilities: &[Capability]| {
            let sender = sender.clone();
            let overflowed = Arc::clone(&overflowed);
            Relay::open(data_dir, capabilities, move |event| {
                enqueue_relay_event(&sender, &overflowed, event)
            })
        };
        // `queue.snapshot` tells which pending decisions wait on a dependency. A daemon without
        // it still works; reminders then pause because the hold state is unknown.
        let (relay, queue_available) =
            match open(&[Capability::Events, Capability::Approvals, Capability::Queue]) {
                Ok((relay, _)) => (relay, true),
                Err(RelayError::UnsupportedCapability) => {
                    match open(&[Capability::Events, Capability::Approvals]) {
                        Ok((relay, _)) => (relay, false),
                        Err(_) => {
                            mark_unknown(&inner, "Unable to connect to the local daemon.");
                            sleep_or_stop(&inner, RETRY_DELAY);
                            continue;
                        }
                    }
                }
                Err(_) => {
                    mark_unknown(&inner, "Unable to connect to the local daemon.");
                    sleep_or_stop(&inner, RETRY_DELAY);
                    continue;
                }
            };
        let mut next_id = 1_u64;
        if send_rpc(&relay, next_id, "events.subscribe", json!({})).is_err() {
            relay.close();
            mark_unknown(&inner, "Unable to subscribe to decision events.");
            continue;
        }
        next_id += 1;
        let mut outstanding = match request_snapshot(&relay, &mut next_id, queue_available) {
            Ok(outstanding) => Some(outstanding),
            Err(_) => {
                relay.close();
                mark_unknown(&inner, "Unable to request pending decisions.");
                continue;
            }
        };
        let mut subscription: Option<String> = None;
        let subscription_deadline = Instant::now() + Duration::from_secs(10);
        let mut initial = false;
        // Real journal timestamps of unresolved `approval.requested` events (replayed or live).
        let mut requested_ts = BTreeMap::<String, u64>::new();
        let mut pending_ack = BTreeSet::<String>::new();
        let mut snapshot_deadline = Instant::now() + SNAPSHOT_INTERVAL;
        let mut new_deadline: Option<Instant> = None;
        let mut reminder_deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if inner.stop.load(Ordering::Acquire) {
                relay.close();
                return;
            }
            if overflowed.load(Ordering::Acquire) {
                mark_unknown(&inner, "Decision event backlog exceeded its safe limit.");
                break;
            }
            let now = Instant::now();
            if subscription.is_none() && now >= subscription_deadline {
                mark_unknown(&inner, "Decision event subscription timed out.");
                break;
            }
            if outstanding
                .as_ref()
                .is_some_and(|pending| now >= pending.deadline)
            {
                mark_unknown(&inner, "Pending decision snapshot timed out.");
                break;
            }
            let snapshot_due =
                now >= snapshot_deadline || new_deadline.is_some_and(|deadline| now >= deadline);
            if snapshot_due && outstanding.is_none() {
                match request_snapshot(&relay, &mut next_id, queue_available) {
                    Ok(pending) => outstanding = Some(pending),
                    Err(_) => {
                        mark_unknown(&inner, "Unable to request pending decisions.");
                        break;
                    }
                }
                snapshot_deadline = now + SNAPSHOT_INTERVAL;
                new_deadline = None;
            }
            if now >= reminder_deadline && initial {
                refresh_permission(&inner);
                send_due_reminder(&inner);
                reminder_deadline = now + Duration::from_secs(30);
            }
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(RelayEvent::Closed(_)) => break,
                Ok(RelayEvent::Message(message)) => {
                    let response_id = message.get("id").and_then(Value::as_u64);
                    if response_id == Some(1) {
                        subscription = message
                            .get("result")
                            .and_then(|result| result.get("subscription_id"))
                            .and_then(Value::as_str)
                            .map(str::to_owned);
                        if subscription.is_none() {
                            mark_unknown(&inner, "Decision event subscription was rejected.");
                            break;
                        }
                    } else if response_id.is_some()
                        && outstanding
                            .as_ref()
                            .is_some_and(|pending| pending.owns(response_id))
                    {
                        let Some(pending) = outstanding.as_mut() else {
                            continue;
                        };
                        if response_id == Some(pending.approvals_id) {
                            let Some(rows) = message.get("result").and_then(Value::as_array) else {
                                mark_unknown(&inner, "Unable to read pending decisions.");
                                break;
                            };
                            pending.rows = Some(pending_rows(rows));
                        } else {
                            // A failed or malformed `queue.snapshot` leaves the hold state unknown.
                            pending.waiting_tasks =
                                Some(message.get("result").and_then(dependency_waiting_tasks));
                        }
                        if outstanding.as_ref().is_some_and(Outstanding::complete) {
                            let Some((rows, waiting_tasks)) =
                                outstanding.take().and_then(Outstanding::finish)
                            else {
                                continue;
                            };
                            let ids: BTreeSet<String> =
                                rows.iter().map(|(id, _)| id.clone()).collect();
                            let held = held_approval_ids(&rows, waiting_tasks.as_ref());
                            reconcile(
                                &inner,
                                &ids,
                                held,
                                !baseline_done,
                                &requested_ts,
                                &pending_ack,
                            );
                            refresh_known_count(&inner);
                            initial = true;
                            baseline_done = true;
                            notify_new(&inner);
                            pending_ack.clear();
                            snapshot_deadline = Instant::now() + SNAPSHOT_INTERVAL;
                            publish(&inner, &status(&inner));
                        }
                    } else if message.get("method").and_then(Value::as_str) == Some("events.event")
                    {
                        let params = &message["params"];
                        if params.get("subscription_id").and_then(Value::as_str)
                            != subscription.as_deref()
                        {
                            continue;
                        }
                        let event = &params["event"];
                        let kind = event.get("kind").and_then(Value::as_str).unwrap_or("");
                        let Some(id) = event.get("payload").and_then(approval_id) else {
                            continue;
                        };
                        if kind == "approval.requested" {
                            // Only a real journal timestamp counts; none means the age stays unknown.
                            if let Some(at) = event
                                .get("created_at")
                                .and_then(Value::as_str)
                                .and_then(parse_timestamp)
                            {
                                remember_requested(&mut requested_ts, &id, at);
                                adopt_requested_at(&inner, &id, at);
                            }
                            if initial {
                                new_deadline = Some(Instant::now() + AGGREGATE_WINDOW);
                            }
                        } else if is_acknowledgement_event(kind) {
                            requested_ts.remove(&id);
                            if !insert_acknowledgement_id(&mut pending_ack, &id) {
                                mark_unknown(
                                    &inner,
                                    "Acknowledgement backlog exceeded its safe limit.",
                                );
                                break;
                            }
                            apply_answered_event(&inner, &id);
                            new_deadline = Some(Instant::now() + AGGREGATE_WINDOW);
                        }
                    } else if message.get("method").and_then(Value::as_str) == Some("events.error")
                    {
                        mark_unknown(&inner, "Decision event stream failed.");
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        relay.close();
        if !inner.stop.load(Ordering::Acquire) {
            let current = status(&inner);
            if current.pending_count.is_some() || current.error.is_none() {
                mark_unknown(
                    &inner,
                    "Daemon event connection lost; pending state unknown.",
                );
            }
            sleep_or_stop(&inner, RETRY_DELAY);
        }
    }
}

fn approval_id(value: &Value) -> Option<String> {
    let id = value
        .get("id")
        .or_else(|| value.get("approval_id"))?
        .as_str()?;
    valid_id(id).then(|| id.to_owned())
}

fn is_acknowledgement_event(kind: &str) -> bool {
    matches!(
        kind,
        "review.changes_requested"
            | "approval.approved"
            | "approval.rejected"
            | "approval.superseded"
    )
}

fn acknowledge_marker(marker: &mut Marker) -> bool {
    if marker.acknowledged {
        false
    } else {
        marker.acknowledged = true;
        true
    }
}

fn apply_acknowledgements(markers: &mut BTreeMap<String, Marker>, ids: &BTreeSet<String>) -> bool {
    let mut changed = false;
    for id in ids {
        if let Some(marker) = markers.get_mut(id) {
            changed |= acknowledge_marker(marker);
        }
    }
    changed
}

fn insert_acknowledgement_id(ids: &mut BTreeSet<String>, id: &str) -> bool {
    if ids.contains(id) {
        return true;
    }
    if ids.len() >= MAX_MARKERS {
        return false;
    }
    ids.insert(id.to_owned())
}
fn apply_answered_event(inner: &Inner, id: &str) {
    let mut durable = inner.durable.lock().unwrap_or_else(|e| e.into_inner());
    let changed = durable.markers.get_mut(id).is_some_and(acknowledge_marker);
    if changed && save_state(&inner.path, &durable).is_err() {
        set_error(inner, Some("Unable to save notification history."));
    }
}

/// Remembers a real journal timestamp. When the map is full a new id is skipped, which only leaves
/// that decision's age unknown (it is then never reminded by age).
fn remember_requested(requested: &mut BTreeMap<String, u64>, id: &str, at: u64) {
    if requested.contains_key(id) || requested.len() < MAX_MARKERS {
        requested.insert(id.to_owned(), at);
    }
}

/// Fills in the real request time of a known decision that has none yet. Returns whether it did.
fn adopt_requested_time(markers: &mut BTreeMap<String, Marker>, id: &str, at: u64) -> bool {
    match markers.get_mut(id) {
        Some(marker) if marker.requested_at.is_none() => {
            marker.requested_at = Some(at);
            true
        }
        _ => false,
    }
}

fn adopt_requested_at(inner: &Inner, id: &str, at: u64) {
    let mut durable = inner.durable.lock().unwrap_or_else(|e| e.into_inner());
    if adopt_requested_time(&mut durable.markers, id, at)
        && save_state(&inner.path, &durable).is_err()
    {
        set_error(inner, Some("Unable to save notification history."));
    }
}

/// `(approval id, task id)` of one pending decision.
type PendingRow = (String, Option<String>);

fn pending_rows(rows: &[Value]) -> Vec<PendingRow> {
    rows.iter()
        .filter_map(|row| {
            let task = row
                .pointer("/action/task_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
            approval_id(row).map(|id| (id, task))
        })
        .collect()
}

/// Task ids that `queue.snapshot` reports as waiting on a dependency. `None` when the result is
/// not a queue snapshot.
fn dependency_waiting_tasks(result: &Value) -> Option<BTreeSet<String>> {
    let queued = result.get("queued")?.as_array()?;
    Some(
        queued
            .iter()
            .filter(|entry| {
                entry.get("waiting_reason").and_then(Value::as_str) == Some("dependency")
            })
            .filter_map(|entry| {
                entry
                    .get("task_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect(),
    )
}

/// Pending decisions whose task waits on a dependency. `None` (unknown) when the queue state is
/// unknown.
fn held_approval_ids(
    rows: &[PendingRow],
    waiting_tasks: Option<&BTreeSet<String>>,
) -> Option<BTreeSet<String>> {
    let waiting = waiting_tasks?;
    Some(
        rows.iter()
            .filter(|(_, task)| task.as_ref().is_some_and(|task| waiting.contains(task)))
            .map(|(id, _)| id.clone())
            .collect(),
    )
}

/// The two read-only requests of one snapshot, answered independently.
struct Outstanding {
    approvals_id: u64,
    queue_id: Option<u64>,
    deadline: Instant,
    rows: Option<Vec<PendingRow>>,
    /// Outer `None`: queue reply not received yet. Inner `None`: hold state unknown.
    waiting_tasks: Option<Option<BTreeSet<String>>>,
}

impl Outstanding {
    fn owns(&self, id: Option<u64>) -> bool {
        id == Some(self.approvals_id) || (id.is_some() && id == self.queue_id)
    }

    fn complete(&self) -> bool {
        self.rows.is_some() && self.waiting_tasks.is_some()
    }

    fn finish(self) -> Option<(Vec<PendingRow>, Option<BTreeSet<String>>)> {
        Some((self.rows?, self.waiting_tasks?))
    }
}

/// Builds a request only for a read-only method; anything else is refused.
fn rpc_request(id: u64, method: &str, params: Value) -> Option<Value> {
    READ_ONLY_METHODS
        .contains(&method)
        .then(|| json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
}

fn send_rpc(relay: &Relay, id: u64, method: &str, params: Value) -> Result<(), ()> {
    let request = rpc_request(id, method, params).ok_or(())?;
    relay.send(request).map_err(|_| ())
}

fn request_snapshot(
    relay: &Relay,
    next_id: &mut u64,
    queue_available: bool,
) -> Result<Outstanding, ()> {
    let approvals_id = *next_id;
    *next_id += 1;
    send_rpc(relay, approvals_id, "approvals.list", Value::Null)?;
    let queue_id = if queue_available {
        let id = *next_id;
        *next_id += 1;
        send_rpc(relay, id, "queue.snapshot", Value::Null)?;
        Some(id)
    } else {
        None
    };
    Ok(Outstanding {
        approvals_id,
        queue_id,
        deadline: Instant::now() + Duration::from_secs(10),
        rows: None,
        waiting_tasks: if queue_available { None } else { Some(None) },
    })
}

fn refresh_known_count(inner: &Inner) {
    let durable = inner.durable.lock().unwrap_or_else(|e| e.into_inner());
    let mut runtime = inner.runtime.lock().unwrap_or_else(|e| e.into_inner());
    if runtime.pending_count.is_some() {
        runtime.pending_count = pending_count(Some(&runtime.pending_ids), &durable.markers);
    }
}

fn prune_resolved(markers: &mut BTreeMap<String, Marker>, pending: &BTreeSet<String>) -> bool {
    let before = markers.len();
    markers.retain(|id, _| pending.contains(id));
    before != markers.len()
}

fn pending_count(
    pending: Option<&BTreeSet<String>>,
    markers: &BTreeMap<String, Marker>,
) -> Option<usize> {
    pending.map(|ids| {
        ids.iter()
            .filter(|id| !markers.get(*id).is_some_and(|marker| marker.acknowledged))
            .count()
    })
}

fn should_notify_new(marker: &Marker) -> bool {
    !marker.acknowledged && !marker.new_notification_attempted
}

fn recoverable_daemon_error(error: Option<&str>) -> bool {
    matches!(
        error,
        Some(
            "Local daemon data directory is unavailable."
                | "Unable to connect to the local daemon."
                | "Unable to subscribe to decision events."
                | "Unable to request pending decisions."
                | "Decision event subscription timed out."
                | "Decision event subscription was rejected."
                | "Pending decision snapshot timed out."
                | "Unable to read pending decisions."
                | "Decision event stream failed."
                | "Decision event backlog exceeded its safe limit."
                | "Acknowledgement backlog exceeded its safe limit."
                | "New decision backlog exceeded its safe limit."
                | "Daemon event connection lost; pending state unknown."
                | "Notification history is full; new alerts are paused."
                | "Unable to save notification history."
        )
    )
}

fn insert_marker_if_room(
    markers: &mut BTreeMap<String, Marker>,
    id: String,
    marker: Marker,
) -> bool {
    if markers.contains_key(&id) || markers.len() >= MAX_MARKERS {
        return false;
    }
    markers.insert(id, marker);
    true
}
/// Records the pending decisions. `baseline` is true only for the first snapshot of this app
/// process: decisions already pending then are recorded as already announced, so launching the app
/// never produces a notification burst. A later snapshot (including the first one after a
/// reconnect) leaves a decision it has not seen before unannounced, so it is announced once.
/// `requested` holds real journal timestamps only; a decision without one keeps an unknown age.
fn reconcile_markers(
    markers: &mut BTreeMap<String, Marker>,
    pending: &BTreeSet<String>,
    baseline: bool,
    requested: &BTreeMap<String, u64>,
    acknowledgement_ids: &BTreeSet<String>,
    now: u64,
) -> (bool, bool) {
    let mut changed = prune_resolved(markers, pending);
    let mut at_capacity = false;
    for id in pending {
        let requested_at = requested.get(id).copied().filter(|at| *at <= now);
        if markers.contains_key(id) {
            if let Some(at) = requested_at {
                changed |= adopt_requested_time(markers, id, at);
            }
            continue;
        }
        let marker = Marker {
            requested_at,
            last_notified_at: None,
            last_reminded_at: None,
            new_notification_attempted: baseline,
            acknowledged: false,
            silenced: false,
            deferred_until: None,
        };
        if insert_marker_if_room(markers, id.clone(), marker) {
            changed = true;
        } else {
            at_capacity = true;
        }
    }
    changed |= apply_acknowledgements(markers, acknowledgement_ids);
    (changed, at_capacity)
}

fn reconcile(
    inner: &Inner,
    pending: &BTreeSet<String>,
    held: Option<BTreeSet<String>>,
    baseline: bool,
    requested: &BTreeMap<String, u64>,
    acknowledgement_ids: &BTreeSet<String>,
) {
    let now = unix_now();
    let mut durable = inner.durable.lock().unwrap_or_else(|e| e.into_inner());
    let (changed, at_capacity) = reconcile_markers(
        &mut durable.markers,
        pending,
        baseline,
        requested,
        acknowledgement_ids,
        now,
    );
    let retry_save = inner
        .runtime
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .error
        .as_deref()
        == Some("Unable to save notification history.");
    let saved = !changed && !retry_save || save_state(&inner.path, &durable).is_ok();
    let count = pending_count(Some(pending), &durable.markers);
    drop(durable);
    let mut runtime = inner.runtime.lock().unwrap_or_else(|e| e.into_inner());
    runtime.pending_ids = pending.clone();
    runtime.held_ids = held;
    runtime.pending_count = count;
    if !saved {
        runtime.error = Some("Unable to save notification history.".into());
    } else if at_capacity {
        runtime.error = Some("Notification history is full; new alerts are paused.".into());
    } else if recoverable_daemon_error(runtime.error.as_deref()) {
        runtime.error = None;
    }
}

/// Parses a journal `created_at`: SQLite `CURRENT_TIMESTAMP` (`YYYY-MM-DD HH:MM:SS`, UTC) as the
/// daemon stores it, or RFC 3339. Anything else, or a future time, is not a real timestamp.
fn parse_timestamp_at(timestamp: &str, now: u64) -> Option<u64> {
    let parsed =
        time::OffsetDateTime::parse(timestamp, &time::format_description::well_known::Rfc3339)
            .ok()
            .or_else(|| {
                let format = time::format_description::parse_borrowed::<2>(
                    "[year]-[month]-[day] [hour]:[minute]:[second]",
                )
                .ok()?;
                Some(
                    time::PrimitiveDateTime::parse(timestamp, &format)
                        .ok()?
                        .assume_utc(),
                )
            })?
            .unix_timestamp();
    u64::try_from(parsed).ok().filter(|value| *value <= now)
}

fn parse_timestamp(timestamp: &str) -> Option<u64> {
    parse_timestamp_at(timestamp, unix_now())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// A reminder is due once the decision is `interval` old by its real request time, then every
/// `interval` after the last reminder. No real request time means no age, so never due. Answered,
/// silenced and deferred (until the deferral passes) decisions are never due.
fn reminder_due(marker: &Marker, now: u64, interval: Option<u16>) -> bool {
    let (Some(minutes), Some(requested_at)) = (interval, marker.requested_at) else {
        return false;
    };
    let since = marker.last_reminded_at.unwrap_or(requested_at);
    !marker.acknowledged
        && !marker.silenced
        && marker.deferred_until.is_none_or(|until| now >= until)
        && requested_at <= now
        && now.saturating_sub(since) >= u64::from(minutes) * 60
}

/// Pending decisions to remind about now. Decisions waiting on a dependency are excluded; when
/// that is unknown (`held` is `None`) nothing is reminded.
fn reminders_due(
    markers: &BTreeMap<String, Marker>,
    pending: &BTreeSet<String>,
    held: Option<&BTreeSet<String>>,
    now: u64,
    interval: Option<u16>,
) -> Vec<String> {
    let Some(held) = held else {
        return Vec::new();
    };
    markers
        .iter()
        .filter(|(id, marker)| {
            pending.contains(*id) && !held.contains(*id) && reminder_due(marker, now, interval)
        })
        .map(|(id, _)| id.clone())
        .collect()
}

fn notification_allowed(enabled: bool, permission: Permission) -> bool {
    enabled && permission == Permission::Granted
}
fn delivery_allowed(known: bool, enabled: bool, permission: Permission) -> bool {
    known && notification_allowed(enabled, permission)
}

fn send_due_reminder(inner: &Inner) {
    let now = unix_now();
    let (enabled, interval, permission, known, due) = {
        let durable = inner.durable.lock().unwrap_or_else(|e| e.into_inner());
        let runtime = inner.runtime.lock().unwrap_or_else(|e| e.into_inner());
        let interval = durable.settings.reminder_interval_minutes;
        let due = reminders_due(
            &durable.markers,
            &runtime.pending_ids,
            runtime.held_ids.as_ref(),
            now,
            interval,
        );
        (
            durable.settings.enabled,
            interval,
            runtime.permission,
            runtime.pending_count.is_some(),
            due,
        )
    };
    if interval.is_none() || due.is_empty() || !delivery_allowed(known, enabled, permission) {
        return;
    }
    let count = due.len();
    let mut durable = inner.durable.lock().unwrap_or_else(|e| e.into_inner());
    for id in due {
        if let Some(marker) = durable.markers.get_mut(&id) {
            marker.last_reminded_at = Some(now);
        }
    }
    let saved = save_state(&inner.path, &durable).is_ok();
    drop(durable);
    if !saved {
        set_error(inner, Some("Unable to save notification history."));
        publish(inner, &status(inner));
        return;
    }
    let delivered = inner
        .app
        .notification()
        .builder()
        .title("Decision reminder")
        .body(notification_body(count))
        .show()
        .is_ok();
    set_error(
        inner,
        (!delivered).then_some("Unable to deliver OS notification."),
    );
    publish(inner, &status(inner));
}

fn notification_body(count: usize) -> String {
    if count == 1 {
        "1 decision is waiting for your review.".into()
    } else {
        format!("{count} decisions are waiting for your review.")
    }
}

fn notify_new(inner: &Inner) {
    let now = unix_now();
    let (enabled, permission, known, eligible) = {
        let durable = inner.durable.lock().unwrap_or_else(|e| e.into_inner());
        let runtime = inner.runtime.lock().unwrap_or_else(|e| e.into_inner());
        let eligible = runtime
            .pending_ids
            .iter()
            .filter(|id| durable.markers.get(*id).is_some_and(should_notify_new))
            .cloned()
            .collect::<Vec<_>>();
        (
            durable.settings.enabled,
            runtime.permission,
            runtime.pending_count.is_some(),
            eligible,
        )
    };
    if !known || eligible.is_empty() {
        return;
    }
    let should_send = delivery_allowed(known, enabled, permission);
    let mut durable = inner.durable.lock().unwrap_or_else(|e| e.into_inner());
    for id in &eligible {
        if let Some(marker) = durable.markers.get_mut(id) {
            marker.new_notification_attempted = true;
            if should_send {
                marker.last_notified_at = Some(now);
            }
        }
    }
    let saved = save_state(&inner.path, &durable).is_ok();
    drop(durable);
    if !saved {
        set_error(inner, Some("Unable to save notification history."));
        publish(inner, &status(inner));
        return;
    }
    let delivered = !should_send
        || inner
            .app
            .notification()
            .builder()
            .title("Decision needed")
            .body(notification_body(eligible.len()))
            .show()
            .is_ok();
    set_error(
        inner,
        (!delivered).then_some("Unable to deliver OS notification."),
    );
    publish(inner, &status(inner));
}

fn sleep_or_stop(inner: &Inner, duration: Duration) {
    let deadline = Instant::now() + duration;
    while !inner.stop.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker() -> Marker {
        Marker {
            requested_at: Some(1_000),
            last_notified_at: None,
            last_reminded_at: None,
            new_notification_attempted: false,
            acknowledged: false,
            silenced: false,
            deferred_until: None,
        }
    }

    const FOUR_HOURS: u64 = 4 * 60 * 60;

    #[test]
    fn defaults_and_interval_bounds_are_supported() {
        assert!(!Settings::default().enabled);
        // Owner decision 72334: the default reminder interval is 4 hours.
        assert_eq!(DEFAULT_INTERVAL_MINUTES, 240);
        assert_eq!(Settings::default().reminder_interval_minutes, Some(240));
        assert_eq!(unavailable_status("x").reminder_interval_minutes, Some(240));
        assert!(valid_interval(Some(DEFAULT_INTERVAL_MINUTES)));
        assert!(valid_interval(None));
        assert!(valid_interval(Some(5)));
        assert!(valid_interval(Some(1440)));
        assert!(!valid_interval(Some(4)));
        assert!(!valid_interval(Some(1441)));
    }

    #[test]
    fn invalid_settings_leave_memory_and_private_disk_state_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(SETTINGS_FILE);
        let mut state = DurableState::default();
        state.settings.enabled = true;
        state.markers.insert("approval-1".into(), marker());
        save_state(&path, &state).unwrap();
        let memory_before = serde_json::to_vec(&state).unwrap();
        let disk_before = fs::read(&path).unwrap();

        let result = update_settings(
            &path,
            &mut state,
            serde_json::json!({
                "enabled": false,
                "reminder_interval_minutes": 4,
                "request_permission": false
            }),
        );

        assert!(result.is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), memory_before);
        assert_eq!(fs::read(&path).unwrap(), disk_before);
    }

    #[test]
    fn delivery_requires_known_state_enabled_setting_and_granted_permission() {
        assert!(delivery_allowed(true, true, Permission::Granted));
        assert!(!delivery_allowed(false, true, Permission::Granted));
        assert!(!delivery_allowed(true, false, Permission::Granted));
        for permission in [
            Permission::Denied,
            Permission::Prompt,
            Permission::Unavailable,
        ] {
            assert!(!delivery_allowed(true, true, permission));
        }
    }

    #[test]
    fn reminder_fires_at_four_hours_then_every_four_hours_from_the_real_request_time() {
        let requested = Marker {
            requested_at: Some(10_000),
            ..marker()
        };
        let interval = Some(DEFAULT_INTERVAL_MINUTES);
        assert!(!reminder_due(&requested, 10_000 + FOUR_HOURS - 1, interval));
        assert!(reminder_due(&requested, 10_000 + FOUR_HOURS, interval));
        // Announcing the new decision does not move the first reminder.
        let announced = Marker {
            last_notified_at: Some(10_100),
            ..requested.clone()
        };
        assert!(reminder_due(&announced, 10_000 + FOUR_HOURS, interval));
        // After a reminder the next one is four hours later, again and again.
        let reminded_at = 10_000 + FOUR_HOURS + 30;
        let reminded = Marker {
            last_reminded_at: Some(reminded_at),
            ..requested.clone()
        };
        assert!(!reminder_due(
            &reminded,
            reminded_at + FOUR_HOURS - 1,
            interval
        ));
        assert!(reminder_due(&reminded, reminded_at + FOUR_HOURS, interval));
        // Reminders off, a future request time and an answered decision are never due.
        assert!(!reminder_due(&requested, 10_000 + 10 * FOUR_HOURS, None));
        let future = Marker {
            requested_at: Some(20_000),
            ..marker()
        };
        assert!(!reminder_due(&future, 10_000 + FOUR_HOURS, interval));
        let answered = Marker {
            acknowledged: true,
            ..requested
        };
        assert!(!reminder_due(&answered, 10_000 + 10 * FOUR_HOURS, interval));
    }

    #[test]
    fn a_decision_without_a_real_timestamp_is_announced_once_and_never_reminded_by_age() {
        let pending = BTreeSet::from(["approval-1".to_owned()]);
        let mut markers = BTreeMap::new();
        // `approvals.list` carries no timestamp and no journal event was seen: nothing is invented.
        reconcile_markers(
            &mut markers,
            &pending,
            false,
            &BTreeMap::new(),
            &BTreeSet::new(),
            1_000,
        );
        let unknown_age = markers.get("approval-1").unwrap();
        assert_eq!(unknown_age.requested_at, None);
        assert!(should_notify_new(unknown_age), "announced once");
        for now in [1_000, 1_000 + FOUR_HOURS, 1_000 + 100 * FOUR_HOURS] {
            assert!(!reminder_due(
                unknown_age,
                now,
                Some(DEFAULT_INTERVAL_MINUTES)
            ));
            assert!(
                reminders_due(&markers, &pending, Some(&BTreeSet::new()), now, Some(5)).is_empty()
            );
        }
        // A later real event timestamp is adopted, and only then can the age-based reminder fire.
        assert!(adopt_requested_time(&mut markers, "approval-1", 500));
        assert!(
            !adopt_requested_time(&mut markers, "approval-1", 900),
            "a known time is kept"
        );
        let due = reminders_due(
            &markers,
            &pending,
            Some(&BTreeSet::new()),
            500 + FOUR_HOURS,
            Some(240),
        );
        assert_eq!(due, ["approval-1"]);
    }

    #[test]
    fn real_event_timestamps_set_the_age_and_a_future_timestamp_is_not_real() {
        let pending = BTreeSet::from(["approval-1".to_owned(), "approval-2".to_owned()]);
        let requested = BTreeMap::from([
            ("approval-1".to_owned(), 400),
            ("approval-2".to_owned(), 5_000),
        ]);
        let mut markers = BTreeMap::new();
        reconcile_markers(
            &mut markers,
            &pending,
            true,
            &requested,
            &BTreeSet::new(),
            1_000,
        );
        assert_eq!(markers["approval-1"].requested_at, Some(400));
        assert_eq!(markers["approval-2"].requested_at, None, "in the future");
    }

    #[test]
    fn journal_timestamps_parse_in_the_sqlite_utc_format_and_rfc3339() {
        let now = 2_000_000_000;
        // SQLite CURRENT_TIMESTAMP, as the daemon's durable_events.created_at stores it.
        assert_eq!(
            parse_timestamp_at("2026-01-02 03:04:05", now),
            Some(1_767_323_045)
        );
        assert_eq!(
            parse_timestamp_at("2026-01-02T03:04:05Z", now),
            Some(1_767_323_045)
        );
        assert_eq!(
            parse_timestamp_at("2026-01-02T04:04:05+01:00", now),
            Some(1_767_323_045)
        );
        for bad in [
            "",
            "yesterday",
            "2026-01-02",
            "2026-13-02 03:04:05",
            "1767323045",
        ] {
            assert_eq!(parse_timestamp_at(bad, now), None, "{bad}");
        }
        assert_eq!(
            parse_timestamp_at("2026-01-02 03:04:05", 1_767_323_044),
            None,
            "future"
        );
    }

    #[test]
    fn decisions_waiting_on_a_dependency_are_excluded_from_reminders() {
        let pending = BTreeSet::from(["held".to_owned(), "free".to_owned()]);
        let markers =
            BTreeMap::from([("held".to_owned(), marker()), ("free".to_owned(), marker())]);
        let now = 1_000 + FOUR_HOURS;
        let rows = vec![
            ("held".to_owned(), Some("task-waiting".to_owned())),
            ("free".to_owned(), Some("task-running".to_owned())),
        ];
        let queue = serde_json::json!({"queued": [
            {"task_id": "task-waiting", "waiting_reason": "dependency", "waiting_on": "task-a"},
            {"task_id": "task-running", "waiting_reason": "global_cap"}
        ]});
        let waiting = dependency_waiting_tasks(&queue);
        assert_eq!(waiting, Some(BTreeSet::from(["task-waiting".to_owned()])));
        let held = held_approval_ids(&rows, waiting.as_ref());
        assert_eq!(held, Some(BTreeSet::from(["held".to_owned()])));

        assert_eq!(
            reminders_due(&markers, &pending, held.as_ref(), now, Some(240)),
            ["free"]
        );
        // Once the dependency is met the decision is reminded.
        let released = held_approval_ids(&rows, Some(&BTreeSet::new()));
        assert_eq!(
            reminders_due(&markers, &pending, released.as_ref(), now, Some(240)),
            ["free", "held"]
        );
        // An unknown hold state (no queue snapshot) pauses every reminder.
        assert_eq!(held_approval_ids(&rows, None), None);
        assert!(reminders_due(&markers, &pending, None, now, Some(240)).is_empty());
        assert_eq!(
            dependency_waiting_tasks(&serde_json::json!({"nope": []})),
            None
        );
    }

    #[test]
    fn acknowledge_and_defer_are_local_and_leave_the_decision_pending() {
        let pending = BTreeSet::from(["approval-1".to_owned(), "approval-2".to_owned()]);
        let answered = Marker {
            acknowledged: true,
            ..marker()
        };
        let mut markers = BTreeMap::from([
            ("approval-1".to_owned(), marker()),
            ("approval-2".to_owned(), answered),
        ]);
        let now = 1_000 + FOUR_HOURS;
        let interval = Some(DEFAULT_INTERVAL_MINUTES);
        let none_held = BTreeSet::new();
        assert_eq!(
            reminders_due(&markers, &pending, Some(&none_held), now, interval),
            ["approval-1"]
        );

        // Defer postpones: not due until the deferral passes, then due again.
        assert!(apply_notice_action(
            &mut markers,
            &pending,
            NoticeAction::Defer(60),
            now
        ));
        assert_eq!(markers["approval-1"].deferred_until, Some(now + 3_600));
        assert!(
            reminders_due(&markers, &pending, Some(&none_held), now + 3_599, interval).is_empty()
        );
        assert_eq!(
            reminders_due(&markers, &pending, Some(&none_held), now + 3_600, interval),
            ["approval-1"]
        );

        // Acknowledge silences until the decision is answered.
        assert!(apply_notice_action(
            &mut markers,
            &pending,
            NoticeAction::Acknowledge,
            now
        ));
        assert!(!apply_notice_action(
            &mut markers,
            &pending,
            NoticeAction::Acknowledge,
            now
        ));
        assert!(reminders_due(
            &markers,
            &pending,
            Some(&none_held),
            now + 100 * FOUR_HOURS,
            interval
        )
        .is_empty());

        // Neither action answers the decision: it is still pending, still counted, and no answer
        // (`acknowledged`) was recorded. An already answered decision is left untouched.
        assert_eq!(pending_count(Some(&pending), &markers), Some(1));
        assert!(!markers["approval-1"].acknowledged);
        assert!(!markers["approval-2"].silenced && markers["approval-2"].deferred_until.is_none());
    }

    #[test]
    fn notice_requests_are_validated_and_the_notifier_can_only_send_read_only_requests() {
        assert_eq!(
            parse_notice_request(serde_json::json!({"action": "acknowledge", "minutes": null})),
            Ok(NoticeAction::Acknowledge)
        );
        assert_eq!(
            parse_notice_request(serde_json::json!({"action": "defer", "minutes": 60})),
            Ok(NoticeAction::Defer(60))
        );
        for invalid in [
            serde_json::json!({"action": "defer", "minutes": 4}),
            serde_json::json!({"action": "defer", "minutes": 1441}),
            serde_json::json!({"action": "defer", "minutes": null}),
            serde_json::json!({"action": "acknowledge", "minutes": 60}),
            serde_json::json!({"action": "approve", "minutes": null}),
            serde_json::json!({"action": "acknowledge", "minutes": null, "approval_id": "a"}),
            serde_json::json!({"minutes": 60}),
        ] {
            assert!(parse_notice_request(invalid.clone()).is_err(), "{invalid}");
        }

        // No approval RPC can be built, so none can be sent: only reads are allowed.
        for read in ["events.subscribe", "approvals.list", "queue.snapshot"] {
            assert!(rpc_request(7, read, Value::Null).is_some(), "{read}");
        }
        for write in [
            "approvals.resolve",
            "review.request_approval",
            "review.request_changes",
            "delivery.request",
            "tasks.cancel",
        ] {
            assert!(
                rpc_request(7, write, serde_json::json!({"decision": "approve"})).is_none(),
                "{write}"
            );
        }
    }

    #[test]
    fn first_launch_baseline_is_silent_and_a_reconnect_announces_what_appeared_while_disconnected()
    {
        let no_acks = BTreeSet::new();
        let none = BTreeMap::new();
        let mut markers = BTreeMap::new();

        // First launch: decisions already pending are recorded as announced (no burst).
        let at_start = BTreeSet::from(["old-1".to_owned(), "old-2".to_owned()]);
        reconcile_markers(&mut markers, &at_start, true, &none, &no_acks, 1_000);
        assert!(markers.values().all(|marker| !should_notify_new(marker)));

        // Disconnected; "new-1" is requested meanwhile. The first snapshot after the reconnect is
        // not a baseline: only the decision not seen before is announced.
        let after_reconnect =
            BTreeSet::from(["old-1".to_owned(), "old-2".to_owned(), "new-1".to_owned()]);
        reconcile_markers(
            &mut markers,
            &after_reconnect,
            false,
            &none,
            &no_acks,
            2_000,
        );
        let announce: Vec<_> = after_reconnect
            .iter()
            .filter(|id| should_notify_new(&markers[*id]))
            .collect();
        assert_eq!(announce, ["new-1"]);

        // A decision answered while disconnected is dropped, not announced.
        let later = BTreeSet::from(["old-2".to_owned(), "new-1".to_owned()]);
        reconcile_markers(&mut markers, &later, false, &none, &no_acks, 3_000);
        assert!(!markers.contains_key("old-1"));
    }

    #[test]
    fn markers_from_before_these_fields_still_load_with_no_age_and_no_local_action() {
        let old = r#"{"first_observed_at":5,"last_notified_at":null,"last_reminded_at":null,"new_notification_attempted":true,"acknowledged":false}"#;
        let restored: Marker = serde_json::from_str(old).unwrap();
        assert_eq!(restored.requested_at, None);
        assert!(!restored.silenced && restored.deferred_until.is_none());
    }

    #[test]
    fn acknowledged_pending_decision_is_not_counted_notified_or_reminded() {
        let acknowledged = Marker {
            acknowledged: true,
            ..marker()
        };
        let pending = BTreeSet::from(["approval-1".to_owned()]);
        let markers = BTreeMap::from([("approval-1".to_owned(), acknowledged.clone())]);

        assert_eq!(pending_count(Some(&pending), &markers), Some(0));
        assert!(!should_notify_new(&acknowledged));
        assert!(!reminder_due(&acknowledged, 10_000, Some(15)));
    }

    #[test]
    fn buffered_ack_before_snapshot_marks_new_pending_id_ineligible() {
        let mut acknowledgement_ids = BTreeSet::new();
        assert!(insert_acknowledgement_id(
            &mut acknowledgement_ids,
            "approval-1"
        ));
        let pending = BTreeSet::from(["approval-1".to_owned()]);
        let new_ids = BTreeMap::from([("approval-1".to_owned(), 1_000)]);
        let mut markers = BTreeMap::new();

        let (changed, at_capacity) = reconcile_markers(
            &mut markers,
            &pending,
            false,
            &new_ids,
            &acknowledgement_ids,
            1_000,
        );

        let marker = markers.get("approval-1").unwrap();
        assert!(changed);
        assert!(!at_capacity);
        assert!(marker.acknowledged);
        assert!(!should_notify_new(marker));
        assert_eq!(pending_count(Some(&pending), &markers), Some(0));
    }

    #[test]
    fn private_state_save_load_preserves_restart_deduplication() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(SETTINGS_FILE);
        let persisted = Marker {
            last_notified_at: Some(1_000),
            new_notification_attempted: true,
            ..marker()
        };
        let mut state = DurableState::default();
        state.settings.enabled = true;
        state.markers.insert("approval-1".into(), persisted.clone());
        save_state(&path, &state).unwrap();

        let (restored, error) = load_state(&path);

        assert_eq!(error, None);
        assert_eq!(restored.settings, state.settings);
        assert_eq!(restored.markers.get("approval-1"), Some(&persisted));
        assert!(!should_notify_new(
            restored.markers.get("approval-1").unwrap()
        ));
    }

    #[test]
    fn marker_insertion_enforces_capacity_and_event_parser_rejects_invalid_ids() {
        let mut markers = BTreeMap::new();
        for index in 0..MAX_MARKERS {
            assert!(insert_marker_if_room(
                &mut markers,
                format!("approval-{index}"),
                marker()
            ));
        }
        assert_eq!(markers.len(), MAX_MARKERS);
        assert!(!insert_marker_if_room(
            &mut markers,
            "approval-over-capacity".into(),
            marker()
        ));
        assert_eq!(markers.len(), MAX_MARKERS);

        assert_eq!(
            approval_id(&serde_json::json!({"id": "approval-1"})),
            Some("approval-1".into())
        );
        assert!(approval_id(&serde_json::json!({"id": ""})).is_none());
        assert!(approval_id(&serde_json::json!({"id": "bad\u{0001}id"})).is_none());
        assert!(approval_id(&serde_json::json!({
            "id": "x".repeat(MAX_APPROVAL_ID_BYTES + 1)
        }))
        .is_none());
    }

    #[test]
    fn bounded_relay_overflow_invalidates_known_pending_state() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let overflowed = AtomicBool::new(false);
        enqueue_relay_event(
            &sender,
            &overflowed,
            RelayEvent::Message(serde_json::json!({})),
        );
        assert!(!overflowed.load(Ordering::Acquire));
        enqueue_relay_event(
            &sender,
            &overflowed,
            RelayEvent::Message(serde_json::json!({})),
        );
        assert!(overflowed.load(Ordering::Acquire));
        assert!(matches!(receiver.try_recv(), Ok(RelayEvent::Message(_))));

        let mut runtime = RuntimeState {
            pending_count: Some(1),
            pending_ids: BTreeSet::from(["approval-1".to_owned()]),
            ..RuntimeState::default()
        };
        invalidate_pending_state(&mut runtime);
        assert_eq!(runtime.pending_count, None);
        assert!(runtime.pending_ids.is_empty());
        assert!(!delivery_allowed(
            runtime.pending_count.is_some(),
            true,
            Permission::Granted
        ));
    }

    #[test]
    fn unavailable_status_preserves_native_command_failure_contract() {
        let status = unavailable_status("service initialization failed");
        let serialized = serde_json::to_value(&status).unwrap();

        assert_eq!(serialized["enabled"], false);
        assert_eq!(serialized["permission"], "unavailable");
        assert!(serialized["pending_count"].is_null());
        assert!(serialized["error"]
            .as_str()
            .is_some_and(|error| !error.is_empty()));
    }

    #[test]
    fn only_canonical_acknowledgement_kinds_are_accepted() {
        for kind in [
            "review.changes_requested",
            "approval.approved",
            "approval.rejected",
            "approval.superseded",
        ] {
            assert!(is_acknowledgement_event(kind), "{kind}");
        }
        for lookalike in [
            "approved",
            "task.approved",
            "approval.approved_extra",
            "approval.rejected.v2",
            "review.not_changes_requested",
        ] {
            assert!(!is_acknowledgement_event(lookalike), "{lookalike}");
        }
    }
    #[test]
    fn tray_indicator_pixels_distinguish_zero_pending_and_unknown() {
        let base = tauri::image::Image::new_owned(vec![0; 8 * 8 * 4], 8, 8);
        let clear = tray_indicator_icon(&base, Some(0)).unwrap();
        let pending = tray_indicator_icon(&base, Some(2)).unwrap();
        let unknown = tray_indicator_icon(&base, None).unwrap();
        assert_eq!(clear.rgba(), base.rgba());
        assert_ne!(pending.rgba(), clear.rgba());
        assert_ne!(unknown.rgba(), clear.rgba());
        assert_ne!(pending.rgba(), unknown.rgba());
    }
}
