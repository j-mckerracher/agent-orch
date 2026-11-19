# Settings

Open Settings from the sidebar. Each group has its own Save button.

## Agent dispatch defaults

Choose a default provider, then set the model and effort for each provider. Save dispatch defaults to keep these choices across app and daemon restarts. Switching the default provider preserves the other providers' choices. Model and effort options come from the installed agents' catalogs; refresh the list if a catalog is still loading.

These settings apply to new app/daemon tasks across projects, including chat-proposed tasks and workstream tasks. Explicit task choices take precedence, followed by named work classes, then these defaults. An explicit provider uses only that provider's saved choices. An explicit different model does not inherit effort from the saved model. Choosing provider defaults leaves model or effort selection to the provider.

Existing queued/running tasks, retries, and chat model choices are unchanged. Reassigning a task uses the new provider's defaults; revising a task retains its recorded choices. Standalone CLI execution is unchanged. An unavailable saved selection is refused on submission or before launch, never silently replaced. Clear dispatch defaults and save to remove them; tasks without a provider then require an explicit choice.

## Machine load limits

New tasks wait while the machine is busy. Set CPU, Memory, and Disk limits as whole percents from 50 to 100. Each limit is compared with the average of the last 60 seconds, and 100 turns that check off.

## Standing approvals

A standing approval lets the orchestrator run chosen kinds of action without asking each time. It applies only inside the project or workstream, time, count, and token limits you set. Nothing is on until you create one, and you can revoke it at any time.

### Actions that always need approval

Canceling a task, revising a task, and removing a project always need your approval.

### Unattended merges

"Allow unattended merges" lets the orchestrator merge pull requests for one project without asking each time. It merges only when every required check has passed and the head commit is the one that was checked. A merge cannot be undone from this app, so you must tick a confirmation and enter the branch prefix it may merge.

## Preferences and learnings

Preferences are your standing defaults for the orchestrator, such as "keep pull requests small". The orchestrator follows them unless your current message says otherwise. A preference never approves anything for you: merges, cancels and other actions that need your word still ask.

- **Shared preferences** apply to every conversation, including the turns the orchestrator starts on its own. Only you can change them; the global conversation may propose a change, which waits for your approval. A project coordinator or a workstream conversation can read them but not propose a change; it tells you instead.
- **Project and workstream preferences** apply to one project coordinator or one workstream conversation.
- **Learnings** are facts an orchestrator recorded while it worked, such as how long a project's checks take. They are not your wishes. A learning that nobody has confirmed for 180 days is marked stale. Click Keep to confirm it, or remove it.

Each entry has a short key and one line of text. Saving a key that already exists changes that entry, so nothing is listed twice. Each list has a size limit: 2,048 bytes for shared preferences, 2,048 for one project or workstream, and 1,024 for one list of learnings. The list shows how much is used. A save that would go over the limit is refused, with the size it would have reached.

The orchestrator can propose a new preference. You see the key and the text on the card and approve it like any other action; a preference is never saved on its own. A learning the orchestrator records after your message is saved at once while "Let the orchestrator act on its own" is on, and the card says it was done on its own; a learning from an automatic update waits for your approval. Nothing is deleted without you: a removed or replaced entry moves to Removed entries, and you can restore it from there.

Do not put passwords, tokens, or file paths in an entry. The daemon refuses entries that look like these.

## Home directory

The home directory is the folder a task runs in when it is not clear which project the task belongs to, such as a task you start from the global orchestrator. Enter the full path of a folder on this machine, for example `~/Code`, and click Save home directory. The folder must already exist. Leave the field empty to set none, and the orchestrator then asks where to run instead.

A task in the home directory runs in that folder itself, not in a separate worktree. An Implement task there can write to the folder directly. The task only reports back: nothing is reviewed or landed afterwards, and its Delivery is fixed to ReportOnly. Tasks started there at the same time share the folder. The folder is listed on the Projects page.

## Chat options

- "Let the orchestrator propose tasks that end in a pull request": you still approve each task.
- "Give the orchestrator chat read-only task tools": applies from the next chat message.

## Checks on running work

The orchestrator wakes automatically to check progress while work is active. Each extra work unit halves the interval, down to the fastest interval. As work finishes, the interval grows again; with no work, no timer is armed. At the defaults, 1, 2, 3, 4 and 5 work units give intervals of 10 minutes, 5 minutes, 2 minutes 30 seconds, 75 seconds and 60 seconds. Global counts running tasks plus active workstreams; each workstream counts its own running tasks.

Checks join the 60-second wake debounce, wait for any user turn and the machine load gate, and obey the automatic-reply limits and monitoring mode. These model turns use tokens. The separate stuck-task notices use no tokens: a task is flagged after at least 5 minutes of silence or when it exceeds its wall-time limit. You can set the base interval (minutes), fastest interval (seconds), and wall-time limit (minutes).

## Automatic replies

Automatic replies are updates the orchestrator writes without being asked, and each uses tokens. You can cap replies per hour and per day for each conversation, and set a daily token budget. The defaults are 20 replies per hour and 160 per day, with no token budget. When a limit is reached, you still see a notice for each event, and the header shows "Monitoring paused: limit reached".

Checks on running work may use only half of each limit. Once they have, the next check is not made, and no notice is shown for it; events such as a failed task, finished work or a worker that waits for help still get a reply from the other half. Stuck-task notices are not affected.

Ready scout reports trigger a Global update without waiting for a periodic check or a user message. The update reads one bounded, redacted report as untrusted data at Restricted permission. A report is marked relayed only after a successful, non-quiet reply; failed or skipped updates leave the manual relay available. Monitoring Off prevents automatic turns; Quiet stores replies without notifying.

## When settings are locked

Standing approvals, preferences, and project defaults are locked if the running daemon was not started by this app. Close the app and reopen it once the daemon has stopped, about a minute after running work finishes.

## Background animation

The sidebar has a Background switch for the animated background. It is decorative and shows no activity.
