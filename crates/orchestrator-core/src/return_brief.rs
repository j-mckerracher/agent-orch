//! The return brief: what happened while the user was away (DPES-1105 phase 5b).
//!
//! The brief is built from counts, ids, short titles, state words and project codes only. No
//! text an agent wrote (a report body, task output, a failure reason) is ever a field, so the
//! brief cannot carry an injection. It is shown to the user and is never put into a snapshot or
//! a prompt. This module holds the types and the cap; it does no I/O and reads no clock.

use serde::Serialize;

/// Most entries in any one brief list.
pub const BRIEF_LIST_CAP: usize = 10;
/// Longest title in a brief, in bytes.
pub const BRIEF_TITLE_BYTES: usize = 80;

/// Totals for the window. Counts are exact; the lists below are capped samples.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct BriefCounts {
    pub tasks_succeeded: u32,
    pub tasks_failed: u32,
    pub tasks_stalled: u32,
    /// Tasks running now, not only in the window.
    pub tasks_running: u32,
    pub proposals_approved_by_user: u32,
    pub proposals_approved_by_grant: u32,
    pub executed_by_grant: u32,
    pub failed_under_grant: u32,
    /// Proposals pending now.
    pub proposals_waiting: u32,
    /// Delivery approvals requested now.
    pub deliveries_waiting: u32,
    /// Decisions the user holds back now.
    pub decisions_held: u32,
    pub grant_uses: u32,
    pub grants_expired: u32,
    pub grants_revoked: u32,
    pub quiet_updates: u32,
    pub replies_paused_by_limit: u32,
}

/// A grant that was used, expired or revoked in the window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BriefGrant {
    pub id: String,
    /// `project` or `workstream`.
    pub scope_kind: String,
    /// A project code (`p-xxxxxxxx`) or a cut workstream title; never a path.
    pub scope_label: String,
    pub uses_in_window: u32,
    pub uses_used: u32,
    pub max_uses: u32,
    pub status: String,
}

/// One decision or problem the user may want to look at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BriefItem {
    /// `proposal_waiting`, `delivery_waiting`, `task_failed`, `task_stalled` or
    /// `sign_in_needed`.
    pub kind: String,
    /// A proposal id or a task id.
    pub id: String,
    pub title: String,
    pub state: String,
    /// A project code.
    pub project: Option<String>,
    pub at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Brief {
    pub from_ms: u64,
    pub to_ms: u64,
    /// True while the user is still away: the window ends now.
    pub open: bool,
    pub counts: BriefCounts,
    pub grants: Vec<BriefGrant>,
    /// Decisions waiting for the user.
    pub waiting: Vec<BriefItem>,
    /// Failed or stalled work.
    pub attention: Vec<BriefItem>,
}

impl Brief {
    /// Cuts every list to [`BRIEF_LIST_CAP`] entries and every title to
    /// [`BRIEF_TITLE_BYTES`] bytes. Counts already carry the totals, so no overflow count is
    /// kept.
    pub fn capped(mut self) -> Self {
        self.grants.truncate(BRIEF_LIST_CAP);
        for items in [&mut self.waiting, &mut self.attention] {
            items.truncate(BRIEF_LIST_CAP);
            for item in items.iter_mut() {
                item.title = cut_title(&item.title);
            }
        }
        for grant in &mut self.grants {
            grant.scope_label = cut_title(&grant.scope_label);
        }
        self
    }
}

/// `text` cut to [`BRIEF_TITLE_BYTES`] bytes on a character boundary.
pub fn cut_title(text: &str) -> String {
    let mut end = text.len().min(BRIEF_TITLE_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(n: usize, title: &str) -> BriefItem {
        BriefItem {
            kind: "task_failed".to_owned(),
            id: format!("t-{n}"),
            title: title.to_owned(),
            state: "Failed".to_owned(),
            project: Some("p-00000000".to_owned()),
            at_ms: 1,
        }
    }

    fn brief(items: usize) -> Brief {
        Brief {
            from_ms: 1,
            to_ms: 2,
            open: false,
            counts: BriefCounts::default(),
            grants: (0..items)
                .map(|n| BriefGrant {
                    id: format!("g-{n}"),
                    scope_kind: "project".to_owned(),
                    scope_label: "p-00000000".to_owned(),
                    uses_in_window: 1,
                    uses_used: 1,
                    max_uses: 20,
                    status: "active".to_owned(),
                })
                .collect(),
            waiting: (0..items).map(|n| item(n, "short")).collect(),
            attention: (0..items).map(|n| item(n, "short")).collect(),
        }
    }

    #[test]
    fn a_brief_list_is_cut_to_ten_entries_and_counts_are_kept() {
        let mut big = brief(25);
        big.counts.tasks_failed = 25;
        let capped = big.capped();
        assert_eq!(capped.grants.len(), BRIEF_LIST_CAP);
        assert_eq!(capped.waiting.len(), BRIEF_LIST_CAP);
        assert_eq!(capped.attention.len(), BRIEF_LIST_CAP);
        assert_eq!(capped.counts.tasks_failed, 25);
        assert_eq!(capped.attention[0].id, "t-0", "the first entries stay");
    }

    #[test]
    fn a_short_brief_is_left_whole() {
        assert_eq!(brief(3).capped(), brief(3));
    }

    #[test]
    fn a_brief_title_is_cut_to_eighty_bytes_on_a_character_boundary() {
        let mut long = brief(1);
        long.waiting[0].title = "x".repeat(200);
        long.attention[0].title = "é".repeat(100);
        let capped = long.capped();
        assert_eq!(capped.waiting[0].title.len(), BRIEF_TITLE_BYTES);
        assert_eq!(capped.attention[0].title.len(), BRIEF_TITLE_BYTES);
        assert!(capped.attention[0].title.chars().all(|c| c == 'é'));
    }

    #[test]
    fn a_brief_serializes_with_the_documented_field_names() {
        let value = serde_json::to_value(brief(1)).unwrap();
        for key in [
            "from_ms",
            "to_ms",
            "open",
            "counts",
            "grants",
            "waiting",
            "attention",
        ] {
            assert!(value.get(key).is_some(), "{key}");
        }
        assert_eq!(value["counts"]["decisions_held"], 0);
        assert_eq!(value["waiting"][0]["project"], "p-00000000");
    }
}
