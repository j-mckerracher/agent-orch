# Settings

Open Settings from the sidebar. Each group has its own Save button.

## Machine load limits

New tasks wait while the machine is busy. Set CPU, Memory, and Disk limits as whole percents from 50 to 100. Each limit is compared with the average of the last 60 seconds, and 100 turns that check off.

## Standing approvals

A standing approval lets the orchestrator run chosen kinds of action without asking each time. It applies only inside the project or workstream, time, count, and token limits you set. Nothing is on until you create one, and you can revoke it at any time.

### Actions that always need approval

Canceling a task, revising a task, and removing a project always need your approval.

### Unattended merges

"Allow unattended merges" lets the orchestrator merge pull requests for one project without asking each time. It merges only when every required check has passed and the head commit is the one that was checked. A merge cannot be undone from this app, so you must tick a confirmation and enter the branch prefix it may merge.

## Notes for the orchestrator

Notes are your own reminders, global or per workstream. The orchestrator reads them as data, not as instructions. Do not put passwords, tokens, or file paths in them, and the daemon refuses notes that look like these.

## Home directory

The home directory is the folder a task runs in when it is not clear which project the task belongs to, such as a task you start from the global orchestrator. Enter the full path of a folder on this machine, for example `~/Code`, and click Save home directory. The folder must already exist. Leave the field empty to set none, and the orchestrator then asks where to run instead.

A task in the home directory runs in that folder itself, not in a separate worktree. An Implement task there can write to the folder directly. The task only reports back: nothing is reviewed or landed afterwards, and its Delivery is fixed to ReportOnly. Tasks started there at the same time share the folder. The folder is listed on the Projects page.

## Chat options

- "Let the orchestrator propose tasks that end in a pull request": you still approve each task.
- "Give the orchestrator chat read-only task tools": applies from the next chat message.

## Checks on running work

The orchestrator checks for tasks that look stuck, and these checks use no tokens. A task is flagged only after it has been silent for at least 5 minutes. You can set the check interval (minutes), the fastest check interval (seconds), and the longest time a task may run before it is flagged (minutes).

## Automatic replies

Automatic replies are updates the orchestrator writes without being asked, and each uses tokens. You can cap replies per hour and per day for each conversation, and set a daily token budget. When a limit is reached, you still see a notice for each event, and the header shows "Monitoring paused: limit reached".

## When settings are locked

Standing approvals, notes, and project defaults are locked if the running daemon was not started by this app. Close the app and reopen it once the daemon has stopped, about a minute after running work finishes.

## Background animation

The sidebar has a Background switch for the animated background. It is decorative and shows no activity.
