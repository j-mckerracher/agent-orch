//! Times leave the store as RFC 3339 UTC, `YYYY-MM-DDTHH:MM:SSZ`.
//!
//! SQLite's `CURRENT_TIMESTAMP` writes UTC as `YYYY-MM-DD HH:MM:SS`, with no zone
//! designator. A reader that takes that text for local time shows the UTC clock under
//! the local zone name. Every stored time is read through this module, so no wire value
//! is zone-less. Text already in RFC 3339 form (Rust-written columns) passes unchanged.

use sqlx::sqlite::SqliteRow;
use sqlx::Row;

/// `stored` as RFC 3339 UTC. A legacy `YYYY-MM-DD HH:MM:SS` value is UTC by construction.
pub fn utc_instant(stored: String) -> String {
    if stored.as_bytes().get(10) == Some(&b' ') {
        format!("{}T{}Z", &stored[..10], &stored[11..])
    } else {
        stored
    }
}

pub(crate) fn instant(row: &SqliteRow, column: &str) -> Result<String, sqlx::Error> {
    row.try_get(column).map(utc_instant)
}

pub(crate) fn opt_instant(row: &SqliteRow, column: &str) -> Result<Option<String>, sqlx::Error> {
    Ok(row.try_get::<Option<String>, _>(column)?.map(utc_instant))
}

#[cfg(test)]
mod tests {
    use super::utc_instant;

    #[test]
    fn a_legacy_sqlite_time_is_read_as_utc() {
        assert_eq!(
            utc_instant("2026-11-20 06:41:00".to_owned()),
            "2026-11-20T06:41:00Z"
        );
    }

    #[test]
    fn rfc_3339_text_is_unchanged() {
        for already in ["2026-11-20T06:41:00Z", "2026-11-20T06:41:00+00:00", ""] {
            assert_eq!(utc_instant(already.to_owned()), already);
        }
    }
}
