//! How much an agent run may do without a person answering (DPES-1105).
//!
//! The level names what the daemon asks of the agent, and each agent's argument
//! builder maps it to that agent's flags. Design: `docs/design/agent-permission-levels.md`.

use serde::{Deserialize, Serialize};

/// A conversation turn failed because its level has no mapping for the agent
/// (Extended on every agent in PR A).
pub const PERMISSION_LEVEL_UNSUPPORTED: &str = "permission_level_unsupported";

/// Defined now so the wire and the docs name them. Nothing emits them until the
/// live check shows what the CLIs report for a denied or unreviewed action
/// (`docs/design/agent-permission-levels.md`, "Follow-up: live check").
pub const PERMISSION_REVIEW_DENIED: &str = "permission_review_denied";
pub const PERMISSION_REVIEW_UNAVAILABLE: &str = "permission_review_unavailable";

/// A request with no level means [`PermissionLevel::Restricted`], on the wire and
/// in stored records, except a task request, which means [`TASK_DEFAULT_PERMISSION`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionLevel {
    #[default]
    Restricted,
    Extended,
    Auto,
    Full,
}

/// The level a task runs at when its request names none. Implementation work needs
/// a shell and git, which Restricted removes (DPES-1126).
pub const TASK_DEFAULT_PERMISSION: PermissionLevel = PermissionLevel::Auto;

impl PermissionLevel {
    /// Whether this is the level a request without one means. A stored contract
    /// omits a Restricted level, so it is written exactly as before the field.
    pub fn is_restricted(&self) -> bool {
        *self == Self::Restricted
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Restricted => "restricted",
            Self::Extended => "extended",
            Self::Auto => "auto",
            Self::Full => "full",
        }
    }

    /// The level a stored name spells, or `None` for any other text.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "restricted" => Some(Self::Restricted),
            "extended" => Some(Self::Extended),
            "auto" => Some(Self::Auto),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_level_is_spelled_the_same_on_the_wire_and_in_the_store() {
        for (level, name) in [
            (PermissionLevel::Restricted, "restricted"),
            (PermissionLevel::Extended, "extended"),
            (PermissionLevel::Auto, "auto"),
            (PermissionLevel::Full, "full"),
        ] {
            assert_eq!(level.as_str(), name);
            assert_eq!(serde_json::to_value(level).unwrap(), json!(name));
            assert_eq!(
                serde_json::from_value::<PermissionLevel>(json!(name)).unwrap(),
                level
            );
        }
        assert!(serde_json::from_value::<PermissionLevel>(json!("sandboxed")).is_err());
        assert!(serde_json::from_value::<PermissionLevel>(json!("Auto")).is_err());
    }

    #[test]
    fn parse_accepts_exactly_the_stored_names() {
        assert_eq!(PermissionLevel::parse("auto"), Some(PermissionLevel::Auto));
        assert_eq!(PermissionLevel::parse("Auto"), None);
        assert_eq!(PermissionLevel::parse(""), None);
    }

    #[test]
    fn an_absent_level_is_restricted() {
        assert_eq!(PermissionLevel::default(), PermissionLevel::Restricted);
    }
}
