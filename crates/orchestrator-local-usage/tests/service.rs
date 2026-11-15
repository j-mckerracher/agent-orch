//! End-to-end checks of the collector against a FAKE reader: this test binary
//! re-executed as `node`, with its behavior chosen by a mode file. Covers the
//! opt-in, timeouts, crashes, bad and oversized output, a wrong day, a missing
//! binary, the cache and refresh, and opt-out stopping every read.
//!
//! `harness = false`, so this file has its own `main`. Run with
//! `cargo test -p orchestrator-local-usage --test service`.

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use orchestrator_local_usage::{
    LocalUsageService, ReaderFault, ReaderOrigin, ReaderSource, ResolvedReader, Snapshot, Timing,
    Window,
};

/// Recorded reader output for Claude Code (synthetic: one priced call, one unpriced),
/// dated 2026-11-15.
const RECORDED: &[u8] = include_bytes!("fixtures/report-claude-synthetic.json");
const DAY_START: i64 = 1_794_700_800_000; // 2026-11-15T00:00:00Z
const DAY_END: i64 = DAY_START + 86_400_000;

fn window() -> Window {
    Window {
        start_unix_ms: DAY_START,
        end_unix_ms: DAY_END,
        timezone: "UTC".into(),
        utc_offset_seconds: 0,
    }
}

fn zero_report(provider: &str) -> String {
    format!(
        r#"{{"currency":"USD","period":"Today (2026-11-15)","periodKey":"today","overview":{{"cost":0,"calls":0,"sessions":0,"tokens":{{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}}}},"models":[],"unpricedModels":[],"provider":"{provider}"}}"#
    )
}

/// The fake reader: `--fake-reader <mode file> <marker file> ...`. Every extra
/// argument (the real `report` command line) is ignored.
fn fake_reader(args: &[String]) -> ! {
    let mode = fs::read_to_string(&args[0]).unwrap_or_default();
    let mode = mode.trim();
    let marker = PathBuf::from(&args[1]);
    let provider = args
        .windows(2)
        .find(|pair| pair[0] == "--provider")
        .map(|pair| pair[1].clone())
        .unwrap_or_default();
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&marker)
        .unwrap();
    writeln!(file, "run {provider}").unwrap();
    drop(file);
    let mut out = std::io::stdout();
    match mode {
        "hang" => std::thread::sleep(Duration::from_secs(3600)),
        "slow" => {
            std::thread::sleep(Duration::from_millis(400));
            if provider == "claude" {
                out.write_all(RECORDED).unwrap();
            } else {
                out.write_all(zero_report(&provider).as_bytes()).unwrap();
            }
        }
        "ok" => {
            if provider == "claude" {
                out.write_all(RECORDED).unwrap();
            } else {
                out.write_all(zero_report(&provider).as_bytes()).unwrap();
            }
        }
        "crash" => std::process::exit(3),
        "garbage" => out.write_all(b"not json at all").unwrap(),
        "wrong-day" => {
            let text = String::from_utf8(RECORDED.to_vec())
                .unwrap()
                .replace("2026-11-15", "2026-11-14");
            out.write_all(text.as_bytes()).unwrap();
        }
        "oversized" => {
            let chunk = vec![b'a'; 1024 * 1024];
            for _ in 0..9 {
                out.write_all(&chunk).unwrap();
            }
        }
        _ => std::process::exit(9),
    }
    out.flush().unwrap();
    std::process::exit(0)
}

/// Points the collector at this test binary with the mode read from `mode_file`.
struct FakeSource {
    mode_file: PathBuf,
    marker: PathBuf,
    missing_binary: bool,
}

impl ReaderSource for FakeSource {
    fn resolve(&self) -> Result<ResolvedReader, ReaderFault> {
        let program = if self.missing_binary {
            PathBuf::from("/definitely/not/a/real/reader")
        } else {
            std::env::current_exe().unwrap()
        };
        Ok(ResolvedReader {
            program,
            prefix: vec![
                OsString::from("--fake-reader"),
                self.mode_file.as_os_str().to_owned(),
                self.marker.as_os_str().to_owned(),
            ],
            version: "0.9.28".into(),
            node_version: "22.16.0".into(),
            origin: ReaderOrigin::Bundled,
        })
    }
}

struct Scenario {
    dir: tempfile::TempDir,
    mode_file: PathBuf,
    marker: PathBuf,
}

impl Scenario {
    fn new(mode: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mode_file = dir.path().join("mode");
        let marker = dir.path().join("runs");
        fs::write(&mode_file, mode).unwrap();
        Self {
            dir,
            mode_file,
            marker,
        }
    }

    fn set_mode(&self, mode: &str) {
        fs::write(&self.mode_file, mode).unwrap();
    }

    fn runs(&self) -> usize {
        fs::read_to_string(&self.marker)
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }

    fn service(&self, timing: Timing, missing_binary: bool) -> LocalUsageService {
        LocalUsageService::new(
            self.dir.path(),
            Box::new(FakeSource {
                mode_file: self.mode_file.clone(),
                marker: self.marker.clone(),
                missing_binary,
            }),
            timing,
        )
    }
}

fn timing() -> Timing {
    Timing {
        interval: Duration::from_secs(3600),
        retry_gap: Duration::from_millis(100),
        refresh_gap: Duration::from_millis(0),
        timeout: Duration::from_secs(5),
    }
}

fn wait_for(limit: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    condition()
}

fn state_of(snapshot: &Snapshot) -> String {
    serde_json::to_value(snapshot.reader.state)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

fn claude(snapshot: &Snapshot) -> &orchestrator_local_usage::ProviderUsage {
    snapshot
        .providers
        .iter()
        .find(|p| p.id == "claude-code")
        .unwrap()
}

fn has_figures(snapshot: &Snapshot) -> bool {
    snapshot.observed_at_unix_ms.is_some()
}

type Check = fn() -> Result<(), String>;

fn check(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

fn off_by_default_and_no_read_happens() -> Result<(), String> {
    let scenario = Scenario::new("ok");
    let service = scenario.service(timing(), false);
    let snapshot = service.read(window());
    check(!snapshot.enabled, "opt-in must default to off")?;
    check(state_of(&snapshot) == "off", "reader state must be off")?;
    check(
        snapshot
            .providers
            .iter()
            .all(|p| p.estimated_costs.is_empty()),
        "no figures while off",
    )?;
    std::thread::sleep(Duration::from_millis(400));
    check(
        scenario.runs() == 0,
        "nothing may be read while the opt-in is off",
    )
}

fn opt_in_reads_and_maps_estimates_honestly() -> Result<(), String> {
    let scenario = Scenario::new("ok");
    let service = scenario.service(timing(), false);
    service
        .set_enabled(true, window())
        .map_err(|e| e.to_string())?;
    let ready = wait_for(Duration::from_secs(20), || {
        has_figures(&service.read(window()))
    });
    check(ready, "figures should arrive after opt-in")?;
    let snapshot = service.read(window());
    check(
        state_of(&snapshot) == "ok",
        "reader state ok after a complete read",
    )?;
    let claude = claude(&snapshot);
    check(claude.calls == Some(2), "claude calls from the report")?;
    check(
        claude.tokens.input == Some(1900),
        "input tokens are the reported counters",
    )?;
    check(
        claude.estimated_costs.len() == 1
            && claude.estimated_costs[0].amount == "0.0126"
            && claude.estimated_costs[0].currency == "USD",
        "the priced cost is an estimate in USD, not a sum with unpriced calls",
    )?;
    check(
        claude.unpriced_models == 1 && claude.unpriced_tokens == 770,
        "unpriced model is counted, not zero",
    )?;
    check(!claude.cost_complete, "a lower bound is not complete")?;
    let codex = snapshot.providers.iter().find(|p| p.id == "codex").unwrap();
    check(
        codex.calls == Some(0) && codex.estimated_costs.is_empty(),
        "codex with no calls is no_calls, not a $0 figure",
    )?;
    check(
        snapshot.data_window.is_some(),
        "the data window is reported",
    )?;
    check(!snapshot.stale, "fresh figures are not stale")?;
    // Persisted: a new service on the same data directory starts enabled.
    let again = scenario.service(timing(), false);
    check(
        again.read(window()).enabled,
        "the opt-in survives a restart",
    )
}

fn opt_out_stops_every_read_and_clears_the_cache() -> Result<(), String> {
    let scenario = Scenario::new("ok");
    let service = scenario.service(timing(), false);
    service
        .set_enabled(true, window())
        .map_err(|e| e.to_string())?;
    let ready = wait_for(Duration::from_secs(20), || {
        has_figures(&service.read(window()))
    });
    check(ready, "figures before opt-out")?;
    let off = service
        .set_enabled(false, window())
        .map_err(|e| e.to_string())?;
    check(
        !off.enabled && !has_figures(&off),
        "opt-out drops the figures at once",
    )?;
    let runs_at_off = scenario.runs();
    std::thread::sleep(Duration::from_millis(600));
    check(
        scenario.runs() == runs_at_off,
        "no read may start after opt-out",
    )?;
    let after = service.read(window());
    check(
        state_of(&after) == "off" && !has_figures(&after),
        "reads after opt-out stay off",
    )
}

fn opt_out_kills_a_running_reader() -> Result<(), String> {
    let scenario = Scenario::new("hang");
    let service = scenario.service(timing(), false);
    service
        .set_enabled(true, window())
        .map_err(|e| e.to_string())?;
    let started = wait_for(Duration::from_secs(10), || scenario.runs() > 0);
    check(started, "the hanging reader should have started")?;
    let began = Instant::now();
    service
        .set_enabled(false, window())
        .map_err(|e| e.to_string())?;
    let stopped = wait_for(Duration::from_secs(5), || {
        !service.read(window()).refreshing
    });
    check(stopped, "a running reader must stop after opt-out")?;
    check(
        began.elapsed() < Duration::from_secs(5),
        "opt-out must not wait for the reader",
    )
}

fn a_hung_reader_times_out_and_the_read_never_waits() -> Result<(), String> {
    let scenario = Scenario::new("hang");
    let quick = Timing {
        timeout: Duration::from_millis(500),
        ..timing()
    };
    let service = scenario.service(quick, false);
    let began = Instant::now();
    service
        .set_enabled(true, window())
        .map_err(|e| e.to_string())?;
    let snapshot = service.read(window());
    check(
        began.elapsed() < Duration::from_millis(200),
        "a read must not wait for the reader",
    )?;
    check(
        state_of(&snapshot) != "ok",
        "no figure while the reader hangs",
    )?;
    let timed_out = wait_for(Duration::from_secs(15), || {
        state_of(&service.read(window())) == "timeout"
    });
    check(timed_out, "a hung reader is reported as a timeout")
}

fn a_slow_read_shows_the_cache_while_refreshing() -> Result<(), String> {
    let scenario = Scenario::new("slow");
    let service = scenario.service(timing(), false);
    service
        .set_enabled(true, window())
        .map_err(|e| e.to_string())?;
    let during = service.read(window());
    check(
        during.refreshing || !has_figures(&during),
        "a read during a refresh reports it",
    )?;
    let done = wait_for(Duration::from_secs(20), || {
        has_figures(&service.read(window()))
    });
    check(done, "the slow reader eventually completes")
}

fn a_crash_bad_output_and_oversized_output_are_reported() -> Result<(), String> {
    for (mode, expected) in [
        ("crash", "failed"),
        ("garbage", "bad_output"),
        ("oversized", "output_too_large"),
        ("wrong-day", "wrong_day"),
    ] {
        let scenario = Scenario::new(mode);
        let service = scenario.service(timing(), false);
        service
            .set_enabled(true, window())
            .map_err(|e| e.to_string())?;
        let reported = wait_for(Duration::from_secs(20), || {
            state_of(&service.read(window())) == expected
        });
        check(reported, &format!("mode {mode} should report {expected}"))?;
        let snapshot = service.read(window());
        check(
            !has_figures(&snapshot),
            &format!("mode {mode} must not serve figures"),
        )?;
        if mode == "crash" {
            check(
                snapshot.reader.exit_status == Some(3),
                "a crash keeps the exit status",
            )?;
        }
    }
    Ok(())
}

fn a_missing_binary_is_failed_not_zero() -> Result<(), String> {
    let scenario = Scenario::new("ok");
    let service = scenario.service(timing(), true);
    service
        .set_enabled(true, window())
        .map_err(|e| e.to_string())?;
    let reported = wait_for(Duration::from_secs(10), || {
        state_of(&service.read(window())) == "failed"
    });
    check(reported, "a reader that cannot start is failed")?;
    check(
        !has_figures(&service.read(window())),
        "no figures from a reader that never ran",
    )
}

fn figures_stay_as_stale_when_a_later_read_fails() -> Result<(), String> {
    let scenario = Scenario::new("ok");
    let service = scenario.service(timing(), false);
    service
        .set_enabled(true, window())
        .map_err(|e| e.to_string())?;
    let ready = wait_for(Duration::from_secs(20), || {
        has_figures(&service.read(window()))
    });
    check(ready, "first complete read")?;
    scenario.set_mode("crash");
    let _ = service.refresh(window());
    let failed = wait_for(Duration::from_secs(20), || {
        state_of(&service.read(window())) == "failed"
    });
    check(failed, "the later read fails")?;
    let snapshot = service.read(window());
    check(has_figures(&snapshot), "the last complete figures are kept")?;
    check(
        claude(&snapshot).calls == Some(2),
        "kept figures are the last complete ones",
    )
}

fn refresh_runs_the_reader_once_within_the_gap() -> Result<(), String> {
    let scenario = Scenario::new("ok");
    let gapped = Timing {
        refresh_gap: Duration::from_secs(600),
        ..timing()
    };
    let service = scenario.service(gapped, false);
    service
        .set_enabled(true, window())
        .map_err(|e| e.to_string())?;
    let ready = wait_for(Duration::from_secs(20), || {
        has_figures(&service.read(window()))
    });
    check(ready, "first read")?;
    let before = scenario.runs();
    let _ = service.refresh(window());
    let _ = service.refresh(window());
    std::thread::sleep(Duration::from_millis(400));
    check(
        scenario.runs() == before,
        "an explicit refresh inside the gap starts no read",
    )
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--fake-reader") {
        fake_reader(&args[2..]);
    }
    let scenarios: &[(&str, Check)] = &[
        (
            "off by default, no read",
            off_by_default_and_no_read_happens,
        ),
        (
            "opt-in reads and maps estimates honestly",
            opt_in_reads_and_maps_estimates_honestly,
        ),
        (
            "opt-out stops every read and clears the cache",
            opt_out_stops_every_read_and_clears_the_cache,
        ),
        (
            "opt-out kills a running reader",
            opt_out_kills_a_running_reader,
        ),
        (
            "a hung reader times out; reads never wait",
            a_hung_reader_times_out_and_the_read_never_waits,
        ),
        (
            "a slow read shows its state while refreshing",
            a_slow_read_shows_the_cache_while_refreshing,
        ),
        (
            "crash, bad output, oversized and wrong-day are reported",
            a_crash_bad_output_and_oversized_output_are_reported,
        ),
        (
            "a missing binary is failed, not zero",
            a_missing_binary_is_failed_not_zero,
        ),
        (
            "a later failed read keeps the last figures",
            figures_stay_as_stale_when_a_later_read_fails,
        ),
        (
            "explicit refresh is rate limited",
            refresh_runs_the_reader_once_within_the_gap,
        ),
    ];
    let mut failures = 0;
    for (name, run) in scenarios {
        match run() {
            Ok(()) => println!("ok - {name}"),
            Err(message) => {
                failures += 1;
                println!("not ok - {name}: {message}");
            }
        }
    }
    if failures > 0 {
        eprintln!("{failures} scenario(s) failed");
        std::process::exit(1);
    }
    println!("1..{} all scenarios passed", scenarios.len());
}
