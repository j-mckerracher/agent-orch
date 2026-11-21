//! Native work modes a chat turn can run in (DPES-1105). The catalog offers each
//! agent's modes under the CLI's own names. A send selects one option per group,
//! and [`resolve`] turns that selection into the authority tier the daemon's
//! checks read and the argv the turn launches with.
//!
//! The tier and the argv come from this table, never from a client. An option
//! whose argv changes gets a new `version`, because the profile recorded on each
//! turn names that version.

use std::collections::BTreeMap;

use orchestrator_core::permission::PermissionLevel;
use serde::Serialize;

const PERMISSION_GROUP: &str = "permission";
const AGENTS: [&str; 3] = ["claude-code", "github-copilot", "codex"];

/// A group from which a send selects exactly one option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModeGroup {
    pub id: &'static str,
    pub label: &'static str,
    pub options: Vec<ModeOption>,
}

/// One choice within a group. Only the permission group's options carry a tier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModeOption {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub available: bool,
    pub unavailable_reason: Option<&'static str>,
    pub needs_confirmation: bool,
    pub default: bool,
    /// Whether a turn in this option may carry proposals. A work option never
    /// removes actions, so it is neutral (`true`); the permission option decides.
    pub allows_actions: bool,
    #[serde(skip)]
    tier: Option<PermissionLevel>,
    /// Flags this option adds after the tier's base flags.
    #[serde(skip)]
    flags: &'static [&'static str],
    /// Bumped whenever the argv this option resolves to changes.
    #[serde(skip)]
    version: u32,
}

/// What a send asked for: the one permission level a request named before modes
/// existed, or the chosen option of each group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeRequest {
    Legacy(PermissionLevel),
    /// The chosen option per group id. A group left out takes its default.
    Chosen(BTreeMap<String, String>),
}

/// A selection resolved against an agent's table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMode {
    /// The authority tier. The daemon's checks read this, never the wire.
    pub tier: PermissionLevel,
    /// The argument profile recorded on the turn, such as `codex:auto:v2`.
    pub profile: String,
    /// The permission option's id. For Claude it is the `--permission-mode` value.
    pub permission: &'static str,
    /// Flags the chosen options add after the tier's base flags.
    pub flags: Vec<&'static str>,
}

/// The groups an agent offers, in display order. Empty for an agent with no modes.
pub fn groups(agent: &str) -> Vec<ModeGroup> {
    match agent {
        "claude-code" => claude_groups(),
        "github-copilot" => copilot_groups(),
        "codex" => codex_groups(),
        _ => Vec::new(),
    }
}

/// Resolves a request for `agent`. `None` when the agent has no such group or
/// option, the option is unavailable, or the level has no mapping (Extended).
pub fn resolve(agent: &str, request: &ModeRequest) -> Option<ResolvedMode> {
    let groups = groups(agent);
    if let ModeRequest::Chosen(chosen) = request {
        if chosen
            .keys()
            .any(|id| !groups.iter().any(|group| group.id == id))
        {
            return None;
        }
    }
    let mut picked: Vec<(&str, &ModeOption)> = Vec::new();
    for group in &groups {
        let wanted = match request {
            ModeRequest::Legacy(level) if group.id == PERMISSION_GROUP => {
                Some(legacy_option(agent, *level)?)
            }
            ModeRequest::Legacy(_) => None,
            ModeRequest::Chosen(chosen) => chosen.get(group.id).map(String::as_str),
        };
        let option = match wanted {
            Some(id) => group.options.iter().find(|option| option.id == id)?,
            None => group.options.iter().find(|option| option.default)?,
        };
        if !option.available {
            return None;
        }
        picked.push((group.id, option));
    }
    let (_, permission) = *picked
        .iter()
        .find(|(group, _)| *group == PERMISSION_GROUP)?;
    let tier = permission.tier?;
    let suffix: String = picked
        .iter()
        .filter(|(group, option)| *group != PERMISSION_GROUP && !option.default)
        .map(|(_, option)| format!("+{}", option.id))
        .collect();
    Some(ResolvedMode {
        tier,
        profile: format!("{agent}:{}{suffix}:v{}", permission.id, permission.version),
        permission: permission.id,
        flags: picked
            .iter()
            .flat_map(|(_, option)| option.flags.iter().copied())
            .collect(),
    })
}

/// The selection a stored profile was recorded for, with every group filled in.
/// `None` for a profile this table does not produce: a turn from before modes,
/// or a retired version.
pub fn choices_of_profile(profile: &str) -> Option<BTreeMap<String, String>> {
    AGENTS.iter().find_map(|agent| {
        selections(agent).into_iter().find(|choice| {
            resolve(agent, &ModeRequest::Chosen(choice.clone()))
                .is_some_and(|mode| mode.profile == profile)
        })
    })
}

/// The option a request for one permission level meant before modes existed.
/// Codex full access is the one exception to its argv: legacy Full had no sandbox
/// flag, so the user's config decided; this option pins `danger-full-access`.
fn legacy_option(agent: &str, level: PermissionLevel) -> Option<&'static str> {
    use PermissionLevel::{Auto, Full, Restricted};
    Some(match (agent, level) {
        ("claude-code", Restricted) => "dontAsk",
        ("claude-code", Auto) => "auto",
        ("claude-code", Full) => "bypassPermissions",
        ("github-copilot", Restricted) => "default",
        ("github-copilot", Auto) => "assisted",
        ("github-copilot", Full) => "allow-all",
        ("codex", Restricted) => "read-only",
        ("codex", Auto) => "auto",
        ("codex", Full) => "full-access",
        _ => return None,
    })
}

/// Every selection an agent offers: one available option from each group.
fn selections(agent: &str) -> Vec<BTreeMap<String, String>> {
    groups(agent)
        .iter()
        .fold(vec![BTreeMap::new()], |partial, group| {
            partial
                .iter()
                .flat_map(|choice| {
                    group
                        .options
                        .iter()
                        .filter(|option| option.available)
                        .map(move |option| {
                            let mut next = choice.clone();
                            next.insert(group.id.to_owned(), option.id.to_owned());
                            next
                        })
                })
                .collect()
        })
}

fn plain(id: &'static str, label: &'static str, description: &'static str) -> ModeOption {
    ModeOption {
        id,
        label,
        description,
        available: true,
        unavailable_reason: None,
        needs_confirmation: false,
        default: false,
        allows_actions: true,
        tier: None,
        flags: &[],
        version: 1,
    }
}

fn permission(
    tier: PermissionLevel,
    id: &'static str,
    label: &'static str,
    description: &'static str,
) -> ModeOption {
    ModeOption {
        tier: Some(tier),
        allows_actions: matches!(tier, PermissionLevel::Restricted | PermissionLevel::Auto),
        ..plain(id, label, description)
    }
}

fn group(id: &'static str, label: &'static str, options: Vec<ModeOption>) -> ModeGroup {
    ModeGroup { id, label, options }
}

fn claude_groups() -> Vec<ModeGroup> {
    use PermissionLevel::{Auto, Full, Restricted};
    vec![
        group(
            PERMISSION_GROUP,
            "Permission",
            vec![
                permission(Auto, "manual", "Manual", "Asks before it acts."),
                permission(
                    Auto,
                    "acceptEdits",
                    "Accept edits",
                    "Edits files without asking.",
                ),
                permission(
                    Auto,
                    "plan",
                    "Plan",
                    "Reads and plans, without editing source.",
                ),
                ModeOption {
                    default: true,
                    // Auto pre-approves Read, Edit and Write, so they run without asking.
                    flags: &["--allowedTools", "Read,Edit,Write"],
                    ..permission(
                        Auto,
                        "auto",
                        "Auto",
                        "Acts on its own, with a safety check.",
                    )
                },
                permission(
                    Restricted,
                    "dontAsk",
                    "Don't ask",
                    "Runs only pre-approved tools and denies the rest.",
                ),
                ModeOption {
                    needs_confirmation: true,
                    ..permission(
                        Full,
                        "bypassPermissions",
                        "Bypass permissions",
                        "Runs everything without asking.",
                    )
                },
            ],
        ),
        group(
            "work",
            "Work",
            vec![
                ModeOption {
                    default: true,
                    ..plain("standard", "Standard", "Answers and works in the chat.")
                },
                ModeOption {
                    available: false,
                    unavailable_reason: Some("Goal mode isn't supported for chat yet."),
                    ..plain("goal", "Goal", "Works until a stated condition holds.")
                },
            ],
        ),
    ]
}

fn copilot_groups() -> Vec<ModeGroup> {
    use PermissionLevel::{Auto, Full, Restricted};
    vec![
        group(
            "agent",
            "Agent",
            vec![
                ModeOption {
                    default: true,
                    ..plain("standard", "Standard", "Answers and works in the chat.")
                },
                ModeOption {
                    flags: &["--plan"],
                    ..plain("plan", "Plan", "Plans before it changes anything.")
                },
                ModeOption {
                    flags: &["--autopilot", "--max-autopilot-continues", "2"],
                    ..plain(
                        "autopilot",
                        "Autopilot",
                        "Keeps going without asking, for at most two continues.",
                    )
                },
                ModeOption {
                    available: false,
                    unavailable_reason: Some(
                        "Plan then autopilot did not finish in a headless turn.",
                    ),
                    flags: &[
                        "--plan",
                        "--mode",
                        "autopilot",
                        "--max-autopilot-continues",
                        "2",
                    ],
                    ..plain(
                        "plan-autopilot",
                        "Plan then autopilot",
                        "Plans, then keeps going without asking.",
                    )
                },
            ],
        ),
        group(
            PERMISSION_GROUP,
            "Permission",
            vec![
                permission(
                    Restricted,
                    "default",
                    "Default",
                    "Reads only, with no writes, shell, web or memory.",
                ),
                ModeOption {
                    default: true,
                    ..permission(
                        Auto,
                        "assisted",
                        "Assisted",
                        "A judge approves safe requests; writes stay in the scratch folder.",
                    )
                },
                ModeOption {
                    needs_confirmation: true,
                    ..permission(
                        Full,
                        "allow-all",
                        "Allow all",
                        "Runs everything without asking.",
                    )
                },
            ],
        ),
    ]
}

fn codex_groups() -> Vec<ModeGroup> {
    use PermissionLevel::{Auto, Full, Restricted};
    let unavailable = "Not available through codex exec.";
    vec![
        group(
            PERMISSION_GROUP,
            "Permission",
            vec![
                permission(
                    Restricted,
                    "read-only",
                    "Read only",
                    "Reads the workspace, writes nothing.",
                ),
                ModeOption {
                    default: true,
                    // Version 2: `--sandbox workspace-write` with `--approve-for-me` is
                    // refused by codex-cli 0.171.0, so version 1 never ran.
                    version: 2,
                    ..permission(
                        Auto,
                        "auto",
                        "Auto",
                        "Writes in the workspace; approvals go to automatic review.",
                    )
                },
                ModeOption {
                    needs_confirmation: true,
                    ..permission(
                        Full,
                        "full-access",
                        "Full access",
                        "Runs with no sandbox, as your user.",
                    )
                },
            ],
        ),
        group(
            "work",
            "Work",
            vec![
                ModeOption {
                    default: true,
                    ..plain("standard", "Standard", "Answers and works in the chat.")
                },
                ModeOption {
                    available: false,
                    unavailable_reason: Some(unavailable),
                    ..plain("plan", "Plan", "Plans before it changes anything.")
                },
                ModeOption {
                    available: false,
                    unavailable_reason: Some(unavailable),
                    ..plain("goal", "Goal", "Works until a stated condition holds.")
                },
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `--permission-mode` choices of `claude` 2.1.311, from `claude --help`.
    const CLAUDE_PERMISSION_MODE_CHOICES: [&str; 6] = [
        "acceptEdits",
        "auto",
        "bypassPermissions",
        "manual",
        "dontAsk",
        "plan",
    ];

    fn all_options(agent: &str) -> Vec<(&'static str, ModeOption)> {
        groups(agent)
            .into_iter()
            .flat_map(|group| {
                group
                    .options
                    .into_iter()
                    .map(move |option| (group.id, option))
            })
            .collect()
    }

    #[test]
    fn each_group_has_exactly_one_default() {
        for agent in AGENTS {
            for group in groups(agent) {
                let defaults = group.options.iter().filter(|option| option.default).count();
                assert_eq!(defaults, 1, "{agent} {}", group.id);
            }
        }
    }

    #[test]
    fn each_agent_has_exactly_one_restricted_option_and_only_permissions_carry_a_tier() {
        for agent in AGENTS {
            let restricted = all_options(agent)
                .into_iter()
                .filter(|(_, option)| option.tier == Some(PermissionLevel::Restricted))
                .count();
            assert_eq!(restricted, 1, "{agent}");
            for (group, option) in all_options(agent) {
                assert_eq!(
                    option.tier.is_some(),
                    group == PERMISSION_GROUP,
                    "{agent} {}",
                    option.id
                );
            }
        }
    }

    #[test]
    fn an_unavailable_option_says_why_and_an_available_one_does_not() {
        for agent in AGENTS {
            for (_, option) in all_options(agent) {
                assert_eq!(
                    option.available,
                    option.unavailable_reason.is_none(),
                    "{}",
                    option.id
                );
            }
        }
    }

    #[test]
    fn a_default_routed_send_gets_the_defaults_of_the_routed_agent() {
        let routed = ModeRequest::Chosen(BTreeMap::new());
        let claude = resolve("claude-code", &routed).unwrap();
        assert_eq!(claude.tier, PermissionLevel::Auto);
        assert_eq!(claude.profile, "claude-code:auto:v1");
        let copilot = resolve("github-copilot", &routed).unwrap();
        assert_eq!(copilot.tier, PermissionLevel::Auto);
        assert_eq!(copilot.profile, "github-copilot:assisted:v1");
        let codex = resolve("codex", &routed).unwrap();
        assert_eq!(codex.tier, PermissionLevel::Auto);
        assert_eq!(codex.profile, "codex:auto:v2");
    }

    #[test]
    fn legacy_levels_resolve_to_the_option_they_named_and_extended_has_none() {
        let tier = |agent, level| resolve(agent, &ModeRequest::Legacy(level)).map(|mode| mode.tier);
        for agent in AGENTS {
            assert_eq!(
                tier(agent, PermissionLevel::Restricted),
                Some(PermissionLevel::Restricted)
            );
            assert_eq!(
                tier(agent, PermissionLevel::Auto),
                Some(PermissionLevel::Auto)
            );
            assert_eq!(
                tier(agent, PermissionLevel::Full),
                Some(PermissionLevel::Full)
            );
            assert_eq!(tier(agent, PermissionLevel::Extended), None);
        }
        assert_eq!(
            resolve("codex", &ModeRequest::Legacy(PermissionLevel::Full))
                .unwrap()
                .profile,
            "codex:full-access:v1"
        );
        assert_eq!(
            resolve(
                "github-copilot",
                &ModeRequest::Legacy(PermissionLevel::Restricted)
            )
            .unwrap()
            .profile,
            "github-copilot:default:v1"
        );
    }

    #[test]
    fn unknown_unavailable_and_mismatched_ids_do_not_resolve() {
        let choose = |pairs: &[(&str, &str)]| {
            ModeRequest::Chosen(
                pairs
                    .iter()
                    .map(|(g, o)| (g.to_string(), o.to_string()))
                    .collect(),
            )
        };
        assert!(resolve("codex", &choose(&[("permission", "sandboxed")])).is_none());
        assert!(resolve("codex", &choose(&[("work", "goal")])).is_none());
        assert!(resolve("codex", &choose(&[("nope", "standard")])).is_none());
        assert!(resolve("claude-code", &choose(&[("work", "goal")])).is_none());
        assert!(resolve("github-copilot", &choose(&[("agent", "plan-autopilot")])).is_none());
        assert!(resolve("pi", &ModeRequest::Chosen(BTreeMap::new())).is_none());
    }

    #[test]
    fn profiles_are_unique_and_each_one_decodes_to_its_selection() {
        for agent in AGENTS {
            let all = selections(agent);
            let profiles: Vec<String> = all
                .iter()
                .map(|choice| {
                    resolve(agent, &ModeRequest::Chosen(choice.clone()))
                        .unwrap()
                        .profile
                })
                .collect();
            let mut unique = profiles.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(unique.len(), profiles.len(), "{agent}");
            for (choice, profile) in all.iter().zip(&profiles) {
                assert_eq!(
                    choices_of_profile(profile).as_ref(),
                    Some(choice),
                    "{profile}"
                );
            }
        }
    }

    #[test]
    fn every_claude_permission_mode_is_a_choice_of_the_pinned_cli() {
        for (group, option) in all_options("claude-code") {
            if group == PERMISSION_GROUP {
                assert!(
                    CLAUDE_PERMISSION_MODE_CHOICES.contains(&option.id),
                    "{}",
                    option.id
                );
            }
        }
    }

    #[test]
    fn a_retired_or_unknown_profile_decodes_to_nothing() {
        assert!(choices_of_profile("codex:auto:v1").is_none());
        assert!(choices_of_profile("codex:restricted:v1").is_none());
        assert!(choices_of_profile("github-copilot:assisted+plan:v1").is_some());
    }
}
