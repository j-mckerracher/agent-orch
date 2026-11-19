//! Held decisions (DPES-1105 phase 4d): `holds.set`, `holds.release` and `holds.list`, and
//! the write an approved `decision.hold` runs.
//!
//! A hold hides a decision until a date. It approves, rejects and expires nothing: a hold
//! "ends" by the clock alone, so reading it needs no job and no write. The date is `YYYY-MM-DD`,
//! today to today plus 60 days by the daemon's clock, and holds until the end of that day in UTC.

use orchestrator_store::{DecisionHold, HoldError, HoldSubject};

use super::*;

const MS_PER_DAY: i64 = 86_400_000;
/// The farthest day a hold may name, counted from today.
const MAX_HOLD_DAYS: i64 = 60;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SetParams {
    subject_kind: String,
    subject_id: String,
    until: String,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReleaseParams {
    hold_id: String,
    idempotency_key: String,
}

impl DaemonApi {
    /// `holds.set`: holds one subject until a date. User-only (routed in the ipc crate).
    pub(super) fn holds_set(&self, params: Value) -> Result<Value, RpcError> {
        let request: SetParams = parse_params(params)?;
        let subject = HoldSubject::parse(&request.subject_kind).ok_or(RpcError::InvalidParams)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "holds.set",
            &key,
            &request,
            || {
                let hold = self.set_hold_inner(
                    subject,
                    &request.subject_id,
                    &request.until,
                    &idempotency::stable_id("hold", &key),
                )?;
                Ok(json!({"hold": hold_wire(&hold)}))
            },
        )
    }

    /// `holds.release`: ends a hold before its date. User-only.
    pub(super) fn holds_release(&self, params: Value) -> Result<Value, RpcError> {
        let request: ReleaseParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "holds.release",
            &key,
            &request,
            || {
                let hold = self
                    .store
                    .release_decision_hold(&request.hold_id)
                    .map_err(map_hold_error)?;
                Ok(json!({"hold": hold_wire(&hold)}))
            },
        )
    }

    /// `holds.list`: the holds in force now. A hold whose day has passed is not listed.
    pub(super) fn holds_list(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        let holds = self
            .store
            .decision_holds_in_force(self.holds_now_ms())
            .map_err(map_hold_error)?;
        Ok(json!({"holds": holds.iter().map(hold_wire).collect::<Vec<_>>()}))
    }

    /// The hold write, shared with action execution. The caller holds the idempotency gate.
    /// `hold_id` derives from the caller's key, so the same request holds once. The date is
    /// checked here against the daemon clock: outside today to today plus 60 days it is
    /// `InvalidParams`. A subject with nothing open to hold is `NotFound`.
    pub(super) fn set_hold_inner(
        &self,
        subject: HoldSubject,
        subject_id: &str,
        until: &str,
        hold_id: &str,
    ) -> Result<DecisionHold, RpcError> {
        let now = self.holds_now_ms();
        let until_ms = hold_end_ms(until, now).ok_or(RpcError::InvalidParams)?;
        if let Some(hold) = self.store.decision_hold(hold_id).map_err(map_hold_error)? {
            return Ok(hold);
        }
        self.store
            .set_decision_hold(hold_id, subject, subject_id, until, until_ms, now)
            .map_err(map_hold_error)
    }

    /// The daemon clock in Unix milliseconds: the wake scheduler's, so a test moves it by hand.
    pub(super) fn holds_now_ms(&self) -> i64 {
        let now = self
            .wake
            .as_ref()
            .map_or_else(crate::executor::unix_now_ms, |wake| wake.now_ms());
        i64::try_from(now).unwrap_or(i64::MAX)
    }
}

fn hold_wire(hold: &DecisionHold) -> Value {
    json!({
        "id": hold.id,
        "subject_kind": hold.subject_kind.as_str(),
        "subject_id": hold.subject_id,
        "until_date": hold.until_date,
        "state": hold.state.as_str(),
    })
}

fn map_hold_error(error: StoreError) -> RpcError {
    match HoldError::of(&error) {
        Some(HoldError::NothingOpen) => RpcError::NotFound,
        Some(HoldError::IdInUse) => RpcError::Conflict,
        Some(_) => RpcError::InvalidParams,
        None => map_store_error(error),
    }
}

/// The last millisecond of `until` (`YYYY-MM-DD`, a real calendar day) in UTC, when that day is
/// today or at most 60 days after it. `None` for any other text or day.
pub(super) fn hold_end_ms(until: &str, now_ms: i64) -> Option<i64> {
    let shaped = until.len() == 10
        && until.bytes().enumerate().all(|(index, byte)| match index {
            4 | 7 => byte == b'-',
            _ => byte.is_ascii_digit(),
        });
    if !shaped {
        return None;
    }
    let year: i64 = until.get(0..4)?.parse().ok()?;
    let month: i64 = until.get(5..7)?.parse().ok()?;
    let day: i64 = until.get(8..10)?.parse().ok()?;
    let days = days_from_civil(year, month, day);
    // A day that is not on the calendar, such as 31 February, does not read back as itself.
    if civil_from_days(days) != (year, month, day) {
        return None;
    }
    let today = now_ms.div_euclid(MS_PER_DAY);
    if !(today..=today + MAX_HOLD_DAYS).contains(&days) {
        return None;
    }
    Some((days + 1) * MS_PER_DAY - 1)
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The `(year, month, day)` of a day count from 1970-01-01. The inverse of `days_from_civil`.
pub(super) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400;
    (if month <= 2 { year + 1 } else { year }, month, day)
}
