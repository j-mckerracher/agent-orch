//! The pull request links of a completed turn (DPES-1108 phase 8, plan 5.8).
//!
//! When a turn completes, the store looks up the recorded pull request of each task the turn
//! acted on (its action block) or reported (its wake records) and keeps up to
//! [`MAX_TURN_LINKS`] links on the turn. A coordinator's report and a routed request stand for
//! their tasks (DPES-1108 integration): a report names its task, and a routed request is served
//! by the tasks its coordinator owns for it, so the global turn that reads a coordinator's
//! answer and the coordinator turn that answers both link the pull requests of that work. A URL is built only from `pull_requests.number` and the
//! repository in `pull_request_repos`, never from reply text, so a correct link never depends on
//! the model copying it.

use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;

use crate::sqlite_conversations::{corrupt, ConversationError};
use crate::SqliteStore;

/// Most links one turn keeps.
pub const MAX_TURN_LINKS: usize = 5;

/// One recorded pull request of a task the turn named.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestLink {
    pub task_id: String,
    pub number: u64,
    /// `https://github.com/<owner>/<name>/pull/<number>`.
    pub url: String,
}

/// The rule `RepoRef::parse` applies to an owner or a name: 1 to 100 ASCII letters, digits, `.`,
/// `_` or `-`, not starting with `-`, and not `.` or `..`. A stored part that fails it gets no
/// link.
fn plain_part(part: &str) -> bool {
    !part.is_empty()
        && part.len() <= 100
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && !part.starts_with('-')
        && part != "."
        && part != ".."
}

fn link_of(task_id: String, number: i64, owner: &str, name: &str) -> Option<PullRequestLink> {
    let number = u64::try_from(number).ok().filter(|number| *number > 0)?;
    (plain_part(owner) && plain_part(name)).then(|| PullRequestLink {
        url: format!("https://github.com/{owner}/{name}/pull/{number}"),
        task_id,
        number,
    })
}

/// Writes the links of `turn_id` from the tasks its block named (`block_tasks`, in block order)
/// and then the subjects of its wake records. Each name also stands for the tasks of the
/// coordinator report or routed request it names ([`tasks_named_by`]). A name that is no such
/// record, or a task with no recorded pull request or no bound repository, gives no link.
/// Nothing is written when there are none.
pub(crate) async fn record_turn_links(
    connection: &mut SqliteConnection,
    turn_id: &str,
    block_tasks: &[String],
) -> Result<(), sqlx::Error> {
    let subjects: Vec<String> =
        sqlx::query_scalar("SELECT subject FROM wake_records WHERE turn_id = ? ORDER BY rowid")
            .bind(turn_id)
            .fetch_all(&mut *connection)
            .await?;
    let mut tasks: Vec<String> = Vec::new();
    for name in block_tasks.iter().chain(&subjects) {
        for task in tasks_named_by(connection, name).await? {
            if !tasks.contains(&task) {
                tasks.push(task);
            }
        }
    }
    let mut links = Vec::new();
    for task in tasks {
        if links.len() == MAX_TURN_LINKS {
            break;
        }
        let row: Option<(i64, String, String)> = sqlx::query_as(
            "SELECT p.number, r.owner, r.name FROM pull_requests p \
             JOIN pull_request_repos r ON r.task_id = p.task_id WHERE p.task_id = ?",
        )
        .bind(&task)
        .fetch_optional(&mut *connection)
        .await?;
        if let Some(link) =
            row.and_then(|(number, owner, name)| link_of(task, number, &owner, &name))
        {
            links.push(link);
        }
    }
    if links.is_empty() {
        return Ok(());
    }
    let text = serde_json::to_string(&links).map_err(|error| sqlx::Error::Encode(error.into()))?;
    sqlx::query("UPDATE conversation_turns SET pull_request_links = ? WHERE id = ?")
        .bind(text)
        .bind(turn_id)
        .execute(&mut *connection)
        .await?;
    Ok(())
}

/// The tasks `name` stands for, in order: itself (a task id, or nothing if it is not one), then
/// the task a coordinator report of that id names, then the tasks serving the routed request it
/// is, or that the report names, oldest first. Each id is derived in its own namespace (a routed
/// request id also starts with `r-`), so a name matches at most one kind of record.
async fn tasks_named_by(
    connection: &mut SqliteConnection,
    name: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let mut tasks = vec![name.to_owned()];
    let report: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT task_id, request_id FROM coordinator_reports WHERE id = ?")
            .bind(name)
            .fetch_optional(&mut *connection)
            .await?;
    let request = match report {
        Some((task, request)) => {
            tasks.extend(task);
            request
        }
        None => Some(name.to_owned()),
    };
    if let Some(request) = request {
        let serving: Vec<String> =
            sqlx::query_scalar("SELECT id FROM tasks WHERE routed_request_id = ? ORDER BY rowid")
                .bind(&request)
                .fetch_all(&mut *connection)
                .await?;
        tasks.extend(serving);
    }
    Ok(tasks)
}

impl SqliteStore {
    /// The pull request links recorded with a completed turn, in the order they were written.
    /// Empty for a turn with none, and for an unknown turn.
    pub fn turn_pull_request_links(
        &self,
        turn_id: &str,
    ) -> Result<Vec<PullRequestLink>, ConversationError> {
        let turn_id = turn_id.to_owned();
        let text: Option<String> = self
            .call(move |pool| async move {
                let text: Option<Option<String>> = sqlx::query_scalar(
                    "SELECT pull_request_links FROM conversation_turns WHERE id = ?",
                )
                .bind(&turn_id)
                .fetch_optional(&pool)
                .await?;
                Ok(text.flatten())
            })
            .map_err(ConversationError::Store)?;
        match text {
            None => Ok(Vec::new()),
            Some(text) => serde_json::from_str(&text).map_err(|error| {
                ConversationError::Store(corrupt(format!("turn pull request links: {error}")))
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_is_built_only_from_plain_parts_and_a_positive_number() {
        let link = link_of("t-1".into(), 12, "octo", "demo.rs").unwrap();
        assert_eq!(link.url, "https://github.com/octo/demo.rs/pull/12");
        assert_eq!(link.number, 12);
        for (owner, name) in [
            ("", "demo"),
            ("-flag", "demo"),
            ("octo", ".."),
            ("oc to", "demo"),
            ("octo", "demo/extra"),
            ("octo", "demo?x=1"),
            ("évil", "demo"),
        ] {
            assert!(
                link_of("t".into(), 1, owner, name).is_none(),
                "{owner}/{name}"
            );
        }
        assert!(link_of("t".into(), 0, "octo", "demo").is_none());
        assert!(link_of("t".into(), -3, "octo", "demo").is_none());
        assert!(link_of("t".into(), 1, &"a".repeat(101), "demo").is_none());
    }
}
