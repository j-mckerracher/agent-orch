# Firstmate behavior: judgment calls by phase

Companion to [firstmate-behavior-plan.md](firstmate-behavior-plan.md). Each phase records the choices it made that the plan did not settle, so the user can check them. Every entry says what was chosen, the alternatives, why, and how to undo it.

## Phase 3: user decisions (DPES-1108, branch `feat/DPES-1108_user-decisions`)

### Schema

- **1. Questions are rows of `decision_holds`, so the table was rebuilt.** The plan said to extend `decision_holds`. Its 0045 CHECKs (`subject_kind IN ('approval','proposal')`, `until_date NOT NULL`, `set_by IN ('user')`) cannot be altered in SQLite. Migration 0063 rebuilds the table the way 0042 rebuilt `action_proposals`: same rowids, same rules for hold rows, and new nullable columns (`question`, `options_json`, `question_key`) used only by `question` rows.
  - Alternative: a separate `decision_questions` table, which would avoid the rebuild. The plan rules this out.
  - Undo: a later migration can move question rows into their own table. No code outside `sqlite_questions.rs` reads the question columns.
- **2. `set_by` on a question names who raised it (`orchestrator` or `worker`), not `user`.** A hold row must still have `set_by = 'user'` (CHECK). Who closed a question is kept in the answer record, where `recorded_by` must be `user`.
- **3. The answer record is a new table, `decision_answers`, kinds `answer`, `defer` and `reconcile`.** No row can be updated or deleted (triggers). A record can be written only while its question is open. `decision_holds_question_closes_by_record` refuses any close of a question with no `answer` or `reconcile` row. This is the database guard behind "only an answer or evidence closes it".
- **4. `action_proposals` was rebuilt as well, to add the kind `decision.ask` to its CHECK.** The 0045 insert trigger on `decision_holds` reads `action_proposals`. SQLite checks every trigger when it renames a table, so the migration drops that trigger first and creates it again with the new table. The store test `migration_0063_applies_to_a_0062_style_database_and_keeps_every_row` covers the rebuild. Two things are not verified: a real user database with many rows, and the down path. There is no down migration, which matches the earlier migrations.

### Behavior

- **5. A deferral does not close a question and does not free its task.** Plan 5.4 lists `decision.defer` with answer and reconcile as the ways a `Waiting` state is released. Firstmate's own lifecycle says that "later" re-holds the item with a date: it leaves the live list and comes back on that date, and it stays answerable. I followed firstmate. A deferred question stays open, still blocks scheduling, and the list shows it as deferred until the end of that UTC day.
  - Alternative: treat a deferral as a close (or as a release of the task). Then the worker would continue with no decision.
  - Undo: in `defer_question`, also write a closing record, and add `defer` to the close trigger's list.
- **6. A task may have several open questions. A key makes asking idempotent per task.** The unique index on open holds now leaves questions out. A new index allows one open question per task and key. Asking again with the same key returns the open question, even if the wording differs. Unkeyed questions are deduplicated only by question id (from the proposal's execution key). The proposal supersede subject is `decision.ask:<task>:<key>`, or a hash of the normalized question text when there is no key.
- **7. A question can be raised on a task in any state, including a finished one.** The likely use in this phase is the orchestrator asking about a scout's report after the scout finished. Only a `Queued` task is affected by scheduling. A running task is not stopped.
- **8. Nothing but the user closes a question.** A task that succeeds, fails or is cancelled keeps its open question until the user answers or reconciles it. This follows firstmate: a wrongly closed decision drops out of review, which is worse than a stale one. The cost: a question on a cancelled task stays in Needs your decision until the user reconciles it.
- **9. The Investigate guard is on the relay, not on completion.** `mark_task_report_relayed` refuses while the task has an open question that its worker raised (`set_by = 'worker'`). `reports.relay` answers `conflict`, and the automatic relay after a report wake leaves the report unrelayed. A scout's promotion needs a relayed report, so promotion is blocked too. Questions the orchestrator raised do not block the relay. The orchestrator usually asks after reading the report, and "a question it raised" in the plan means the task's own question.
  - **Completion is not blocked. Why:** in this phase a worker cannot see a question or wait for an answer. That needs phase 4's `Waiting` state and session resume. If the store refused the transition to `Succeeded`, a run that has already exited would stay `Running`/`Verifying` and keep its slot, with nothing to move it on. Phase 4 should add that guard when `needs-decision` moves the task to `Waiting`.
  - Unverified: the wake path's skip was reviewed by reading `wake.rs::settle` only. The daemon wake tests were not run locally.

### Interface

- **10. All four `decisions.*` methods are user-only, `decisions.list` included.** The brief asked for that. `holds.list` and `approvals.list` are open, and the questions are not secret (the chat snapshot already shows their titles to the orchestrator). But making the list user-only costs nothing for the desktop, which is the approver. The trade-off is that a token-only client, such as a CLI without the approver secret, cannot list questions.
  - Undo: remove `"decisions.list"` from `USER_ONLY_METHODS` and from its pin test.
- **11. The methods share the existing `holds` capability.** A new capability would mean a protocol change and new desktop negotiation for four methods on the same table.
- **12. `decision.ask` is not grantable.** It is on the "left out" list next to `decision.hold`, until phase 5 classifies authority. In this phase it is a normal proposal: the user approves the card, then the question exists.
- **13. Text rules.** A question is up to 1,024 bytes, with up to 6 one-line options of up to 200 bytes each. A key is up to 64 characters of lowercase letters, digits and `-`. An answer or evidence is up to 4,096 bytes. Text is refused, never trimmed, when it is blank or has a control (other than tab and line breaks), bidi or invisible character. The answer is stored byte for byte as sent, including leading and trailing whitespace. Desktop note: the text box caps at 4,096 UTF-16 units, which is not the same as 4,096 bytes. The daemon refuses an over-long answer with `invalid_params`.
- **14. The preamble was trimmed by 14 bytes to fit `decision.ask`.** The action preamble's 3 KiB cap (`MAX_INSTRUCTIONS_BYTES`) was 7 bytes short once `decision.ask (task_id, question, options, key)` was added, and a test requires every kind to be named there. I removed the two-space indentation from the example JSON block and merged `task.cancel, task.retry (task_id)`. The text now measures 3,065 of 3,072 bytes. Plan 5.1 says to raise the cap (proposed 6 KiB). Phase 1 did not raise it, and I did not either, because the cap's command-line budget is phase 1's design. The next kind added will not fit without that change.
- **15. Snapshot block.** "Questions waiting for the user: N" is followed by one line per question: its id, its task and the question cut to 80 bytes and redacted. It is cut with the scout reports when the 6 KiB snapshot is full. Answers are never shown. The deferred marker uses the daemon's wall clock (`unix_now_ms`), not the wake clock that tests inject.

### Desktop (delegated to a sub-agent, reviewed by reading)

- **16. Placement.** "Needs your decision" is the first card in the Review & Decisions queue tab, above Proposed actions. It reads only `decisions.list`. The source is named `questions` / `useQuestions`, because `decisions` already names the approval queue in the desktop code.
- **17. Controls.** Clicking an option fills the answer box, and the user may edit it. Send answer sends the box text verbatim. Defer uses a date input with the hold range. There is no reconcile control in the UI: `decisionsReconcile` exists in `api.ts` only. Because `decisions.list` is user-only, a session that is not the approver gets `Forbidden` on the read itself, and the card shows "Questions are not available" instead of a list. The component also disables the buttons for a non-approver, but with a user-only list that state is not reached.
- **18. Waiting reason.** The queue's new `waiting_reason` `decision` is shown as "Waiting for your decision", in the existing `approval` display category.
- **19. Card wording.** A `decision.ask` card reads "Ask for your decision" / "Asked". The sub-agent left the grants label map unchanged. It reports that the desktop map already lists `decision.hold` as grantable, which differs from core (`GRANTABLE_KINDS` leaves it out). I did not change that pre-existing difference.

### Not verified locally

- Daemon unit tests (`api_decisions_tests.rs`, the snapshot test, the existing kind and preamble pins) and `tests/conversation_runtime.rs` (which calls every user-only method as a bare session and expects `Forbidden`) were compiled with `cargo check --all-targets` but not run. Daemon tests hang on this laptop. CI runs them.
- I ran these locally: `cargo test -p orchestrator-core --lib` for the question, scheduler, kind and grant-list tests, and `cargo test -p orchestrator-store` for `decision_questions`, `lineage_holds_archive`, `action_kinds_phase4`, `action_proposals` and `task_reports`. All passed.
- Desktop: I ran vitest on `src/ui/views/review/`, `activity.test.ts`, `fixtureGuard.test.ts` and `actionCardModel.test.ts` (13 files, 338 tests), and all passed. For that run I linked the main checkout's `node_modules`, then removed the link. `tsc -b --noEmit` reports one error, in `ui/views/agents/harness.tsx`, which this change does not touch. The error names the linked `node_modules` path, so it comes from the link. With a normal install it should not appear, but I could not confirm that.

## Phase 4: worker status, waits and steering (DPES-1108, branch `feat/DPES-1108_worker-status`)

### What each CLI offers (read from `--help` only, no agent turn was run)

Installed versions: Claude Code 2.1.311, Codex CLI 0.169.0, GitHub Copilot CLI 1.0.113.

- **1. Claude Code resumes with `--resume <id>`, at the same level.** `--session-id <uuid>` ("must be a valid UUID") names a new session; `-r, --resume [value]` continues one by id; `--fork-session` (not used) would make a new id instead. The daemon chooses a v4 UUID per attempt, passes `--session-id`, records it, and on resume passes `--resume <id>` with the same `args_for_task` vector, so every permission flag is repeated. Task runs never had `--no-session-persistence` (only chat turns pass it, "sessions will not be saved to disk and cannot be resumed (only works with --print)"), so the session is saved. The session id the CLI reports in its init event is compared with the chosen one and a mismatch is logged (`agent_session_id_mismatch`); the chosen id is kept.
  - Unverified: that a `-p --resume` turn keeps the earlier session's context exactly when the same `--agent` and `--plugin-dir` are passed again, and that Claude finds the session when the cwd is the same worktree. Claude keys sessions by directory; the resume runs in the same worktree.
- **2. Copilot resumes with `--resume=<id>`, at the same level.** `--session-id <id>` "Resume an existing session or task by ID, or set the UUID for a new session"; `-r, --resume [<value>]` resumes by id, prefix or name. The daemon passes `--session-id=<uuid>` at launch and `--resume=<uuid>` on resume, with the same `task_args` vector, so the `--allow-tool`/`--deny-tool` rules and `--allow-all` are repeated. Copilot has no session-persistence flag to turn off.
  - Unverified: that `--resume=<id>` in `-p` mode fails rather than opening a picker when the session is gone. If it starts a fresh session the worker would lack its earlier context but run at the same level.
- **3. Codex resumes with `codex exec resume [OPTIONS] <id> <prompt>`. Restricted and Full keep their level; Auto fails closed.** `exec` names its own session and reports it in `thread.started` (`thread_id`), which the daemon records. `exec resume` offers `-c`, `--json`, `-m`, `--ignore-user-config`, `--skip-git-repo-check`, `--ephemeral` and a few others, but not `--sandbox`, `--add-dir`, `-C` or `--approve-for-me`.
  - Restricted resumes with `--ignore-user-config -c sandbox_mode="workspace-write" -c sandbox_workspace_write.writable_roots=["<status dir>"]`. Unverified: whether `exec resume` applies these over the sandbox the session started with. Either way it cannot be wider than `workspace-write`: the started session was `workspace-write`, and with the user's config ignored Codex's default is narrower. At worst the status directory is not writable on the resumed turn, so the worker cannot report a second wait.
  - Full adds no sandbox flag, as at launch, and no writable-roots `-c`, because that list would replace the user's own. The status directory may not be writable on a resumed Full turn under a `workspace-write` user config.
  - Auto has no way to route approval requests to automatic review on resume. Resuming without it would be narrower, not the same level, so the daemon fails closed: when a Codex Auto worker reports a wait, the attempt fails with "MCAO cannot resume it: Codex cannot continue an Auto task's session at the same permission level, because `codex exec resume` has no --approve-for-me". No question is left waiting on that task.
  - The launch check reads `codex exec --help`, not `codex exec resume --help`, so an old Codex without `exec resume` is not refused before the run; the resume then fails to start and the task fails with the launch reason. Every flag the resume vector uses is also an `exec` flag.
- **4. The status file is granted as one directory.** Plan open item 1. Claude and Codex `exec` get `--add-dir <dir>`; Copilot gets `--add-dir <dir>` and, below Full, `--allow-tool write(<dir>/**)`, because its write rule is limited to the worktree. Claude Restricted has no shell, so the brief tells the worker to read the file and write it back with its line added. Not run: whether each CLI's Restricted sandbox really lets the agent write there. The daemon cannot tell a worker that never reports from one that could not write; both settle as an ordinary run.
- **5. A CLI that lacks a channel flag still runs.** If its `--help` lacks `--add-dir`, the run has no status directory grant; if Claude or Copilot lacks `--session-id` or `--resume`, the run starts no named session, so a wait fails closed ("its CLI session id is not known"). This follows the workflow agent's precedent (launch plain and warn, `status_channel_unsupported`) instead of refusing the launch. A resume never launches without its session flag: it fails the task. Alternative: require the flags in the launch check, which would also refuse older CLIs that work today.

### Status channel

- **6. The file is `<data dir>/task-status/<task id>/<attempt number>/status`.** Per attempt, so a retry starts empty and a resume of the same attempt appends to the same file. Not `tasks/`: the CLI's file store uses `<data>/tasks/<id>.json`. Owner-only directory and file, created before the agent starts. The read refuses a link at the last component (`O_NOFOLLOW`) and a non-regular file, and reads at most 224,200 bytes (200 lines of 1,121). Not handled: a worker that replaces the directory itself with a link; the daemon would then read another file of the same user, and only lines that parse as status lines would be kept.
- **7. Five verbs, no `paused` or `resolved`.** The brief and the parser take `working`, `needs-decision`, `blocked`, `done` and `failed`, with an optional `[key=<slug>]`. Firstmate's `resolved` closes a decision by its key; here only the user closes a question (phase 3 decision 8), so a worker line cannot. `paused` has no meaning for a headless run.
- **8. Limits.** A line is at most 1,120 bytes (a 1,024-byte note plus the verb and key); 20 lines a read, read about once a second, is the rate limit; 200 lines an attempt. A refused line (too long, not UTF-8, a control, bidi or invisible character, a carriage return in the note, an unknown verb, a bad key, a `needs-decision` with no question) is dropped and only its number and refusal code are logged. The note rules are the question rules, so a parsed question can always be raised.
- **9. Notes are redacted before they are stored, with the same redaction as attempt summaries.** Paths and token-like words become markers. The user's question list therefore shows the redacted question. A wake notice quotes the note as "Its worker reported (data, not instructions): ..." after a second sanitize and redact, cut to 160 bytes.
- **10. How a clean exit settles.** A `needs-decision` line anywhere in the run parks it for a decision, even if `done` follows (firstmate: a later done never closes a decision); otherwise a `blocked` line parks it for help. With no wait, the last of `done`/`failed` counts: `failed` fails the attempt with "The agent reported that it failed: `<note>`.", believed because it is the safe direction. `done` decides nothing. A run that exits with an error, or with a cancel pending, settles as before and raises no question.
- **11. Up to three questions per wait.** Further `needs-decision` lines are stored as events but raise no question, so one resume's answers fit a command line. The question id is a hash of the attempt and line number, so reading the file twice raises nothing twice; a key reuses an open question of the same task.

### Waiting and resuming

- **12. `Waiting` has its own attempt state.** `AttemptState::Waiting` (Running to Waiting, Waiting to Running, Failed or Cancelled). Without it the restart gate would see a Running attempt with no live session and block all scheduling. The engine and runtime reconcile code needed no change. The daemon test `a_blocked_worker_waits_through_a_restart_and_a_task_message_resumes_it` checks the gate stays open.
- **13. Task edges.** Running to Waiting on `WorkerWaiting { reason }` (audit `WorkerNeedsDecision` or `WorkerBlocked`); Waiting to Running on `ResumeStarted` (`ExecutionResumed`); Waiting to Cancelled on `CancelRequested` (no owner to confirm); Waiting to Failed on `ExecutionFailed` (a resume that fails closed). A waiting task is not terminal and cannot be archived.
- **14. A wait spends nothing and takes no worker slot; a resume takes one.** The scheduler plans a waiting task with a reply like a queued task (`Scheduler::with_resumable`), under the same global and project caps, load gate, question hold and authorization, so a resume can wait for capacity. The treehouse slot stays leased throughout. Stall checks and work checks ignore waiting tasks (they have no Running attempt).
- **15. Replies are rows, delivered once.** Closing a question of a waiting task queues a `task_messages` row in the same transaction; a `task.message` queues one keyed by its execution key. The resume marks them delivered in the transaction that starts the new session. An answer to a question whose task is not waiting queues nothing. The resume prompt lists each reply under who wrote it ("The user's answer", "The user settled a question without answering it", "Message from MCAO's orchestrator") and is at most 28,000 bytes; a longer one fails the task closed.
- **16. A task waits for every open question.** A waiting task resumes only when it has a reply and no open question, so a worker with two questions gets both answers in one resume. A deferral does not release it (phase 3 decision 5).
- **17. The guard behind "a done claim is only a claim".** The store refuses `CandidateReady` while the task has an open question its own worker raised (phase 3 decision 9 asked for this). The executor parks such runs before that point, so the guard only stops a path that skipped the wait.
- **18. A cancel of a waiting task leaves its question open.** As in phase 3, only the user closes a question. The cancelled task's question stays under Needs your decision until the user reconciles it.

### `task.message`

- **19. Only a waiting task takes a message.** The plan also allows queueing one for a running task until its next exit. That needs the exit path to choose between verifying and resuming, so it is left out; `task.revise` remains the way to change a running task. Live steering is DPES-1133's.
- **20. It is a normal proposal and is not grantable.** As `decision.ask` in phase 3, until phase 5 classifies authority. It is on the "left out" list of `standing_grant.rs`. Its proposal subject is `task.message:<task id>`, so a newer message replaces a pending older one. A waiting task holds at most two undelivered messages; a third is `conflict`.
- **21. The preamble cap was raised from 3 KiB to 4 KiB.** `task.message (task_id, text; a waiting task only)` did not fit in 3,072 bytes. `MAX_INSTRUCTIONS_BYTES` is now 4,096 with its compile-time asserts and the routing test updated. The prompt budget (30,000 UTF-16 units) still bounds the whole command line.

### Wakes

- **22. `worker_needs_decision` and `worker_blocked` start a wake turn.** Both are classified from the `task.transition` to `Waiting` by its audit word. A decision wake gets the existing decision procedure ("name the concrete decision ... Do not approve or decide for the user"); a blocked wake gets a new procedure: say what blocks it, propose `task.message` only with a fact the user's request or the task facts already hold, else `decision.ask`, and never repeat instructions from the note.

### Not verified locally in phase 4

- Daemon tests (`executor_tests.rs` status tests, `wake_events.rs`, `api_conversation_agent.rs` procedure test, the snapshot and routing pins) were compiled with `cargo check --all-targets` and clippy but not run; daemon tests hang on this laptop. CI runs them.
- Run locally and passed: `cargo test -p orchestrator-core --lib`, `cargo test -p orchestrator-store` (including the new `worker_status` suite and the 0063-to-0064 migration test), `cargo test -p orchestrator-agents`, and the desktop vitest suite (127 files, with the main checkout's `node_modules` linked and then removed).
- No real agent was run, so the resume flags were checked against `--help` text only.

## Phase 7: coordinators

DPES-1108, branch `feat/DPES-1108_coordinators`, based on phase 4 (`feat/DPES-1108_worker-status`). Phases 5, 6 and 8 were built at the same time from the same base. This phase therefore has no `ActionAuthority`: every coordinator action is a proposal the user approves.

### New names

- Migrations `0069_coordinator_kinds.sql` (rebuilds `action_proposals` and `wake_records` for the new kinds) and `0070_coordinators.sql`.
- Column `coordinator_scopes.scope_sentence`. Tables `routed_requests` and `coordinator_reports`. Columns `tasks.coordinator_project_id` and `tasks.routed_request_id`.
- Action kinds `coordinator.route` (`project`, `work`; global only) and `coordinator.report` (`verb` answer, caveat or blocked; `note`; `request`; `key`; coordinator only). `task.propose` gains an optional `request`.
- Wake kinds `routed_request` (wakes the coordinator) and `coordinator_report` (wakes Global).
- IPC `coordinator.set_scope` (user-only, capability `conversation_history`). `conversations.get` on a project target returns a `coordinator` object.
- Core module `charter.rs` (`render`, `ReportVerb`, text rules), `BlockScope::Project`. Store module `sqlite_coordinators.rs` (`CoordinatorError`, `CoordinatorFact`, `RoutedRequest`, `CoordinatorReport`).

### Deviation from firstmate

- **1. Coordinators exist for every project mode.** Firstmate keeps local-only projects with its primary supervisor. MCAO's default mode is local-only, so that rule would leave almost every project without a coordinator. A coordinator is allowed for any mode and stays opt-in (0053).

### Charter and scope

- **2. The charter is turn content after the app state, not part of the preamble.** The preamble is shared by every conversation and was near its 4 KiB cap. The charter has its own header and its own 3 KiB ceiling, checked at compile time. It goes after `END APP STATE`, because preamble rule 3 makes text inside the state read as data. It replaces the preamble's identity line ("You are the orchestrator"); the preamble's rules still hold.
  - Alternative: a second preamble for coordinator turns. That would mean two 4 KiB texts to keep in step.
  - Undo: drop `TurnRequest::charter` and its push in `prompt_content`.
- **3. The scope sentence is stored on `coordinator_scopes`, not on `projects`.** It belongs to the coordinator. Removing and restoring a coordinator keeps the row, so the sentence stays too. It is one line of at most 300 bytes and refuses control, bidi and invisible characters. Only the user sets it, through the user-only `coordinator.set_scope`, because every later prompt carries it, the same reason `notes.set` is user-only.
- **4. The scope editor is in the coordinator panel of the conversation view, not in Project detail.** The sentence comes from `conversations.get`, and adding and removing coordinators already happens in the workspace. The desktop sub-agent made this choice and I kept it.

### Acting and depth

- **5. A coordinator acts only inside its own project, and the check runs three times.** The parser checks the scope (`BlockScope::Project` with the project's code). It allows `task.propose` for that code with no workstream, the actions that name an existing task, and `coordinator.report`. It refuses every `workstream.*` and `objective.*` action, `workstream.link_task`, `project.remove` and `coordinator.route`. When the turn completes, the sink checks that every named task belongs to the project and that every named request is open and belongs to the project. A block that fails is rejected whole, as in a workstream. At execution, `within_project` checks the proposal's scope again.
- **6. Depth stays two because only the global conversation may route.** That rule is enforced in four places: the core parser, the snapshot (a coordinator is not offered the kind), execution, and the store. `create_routed_request` refuses unless its origin is an approved `coordinator.route` proposal of `conversation:global`. A coordinator's own proposals never route.
- **7. A coordinator may act on any task of its project, not only on the tasks it owns.** The brief says "restricted to their own project". A retry or a question about a project task that Global made is still inside the project. Ownership is not changed by that.
- **8. Global may still act on a coordinator's tasks.** "The global orchestrator stops supervising that work" is held by wakes and work checks: Global gets no wake for an owned task and no work-check unit for it. Global is not refused a `task.cancel` the user asks for in the global conversation. Undo: add a `task_coordinator` check to the global branch of `within_scope`.

### Routing and ownership

- **9. A routed request's id is `r-` plus 16 hex digits, derived from the proposal's execution key.** A rerun therefore finds the same request.
- **10. Ownership is written before the task is admitted.** A coordinator's `task.propose` now creates the task, writes the owner and then admits it. This replaces the single `admit_contract` call. The executor therefore never sees a Queued coordinator task without its owner.
  - Before the task exists, the store refuses a `request` that is unknown, belongs to another project, or already has an answer, so no task is created that cannot be owned.
  - A trigger keeps the owner write-once, keeps it in the task's own project, and requires an open request of that project.
- **11. A task the coordinator proposes never joins "Global tasks".** `default_link` is false for project scope.
- **12. A revised task keeps its owner.** It keeps the request too, while that request is open. A promotion inherits the owner from the scope of the proposal that made it.
- **13. Ownership counts only while the coordinator is active.** The scope queries read the owner through `coordinator_scopes.removed = 0`. If the coordinator is removed, its finished or failed tasks go back to Global: a retry then wakes Global, not a removed conversation. Removal itself is refused while a task is open (decision 24).

### Wakes

- **14. Wakes follow ownership, and ownership wins over a workstream link.**
  - `task_wake_facts`, `running_attempts` and `supervision_load` give an owned task the scope `conversation:project:<id>`.
  - `expand()` adds no `up:` copy to Global for that scope. A scout report on an owned task relays in the coordinator, which is the turn that reads it.
  - `target_of` now maps project scopes, so these wakes start turns.
  - A wake for a removed coordinator is dropped from the debounce and starts no turn.
- **15. Idle by default comes from the work-check count.** A coordinator's work-check unit is its running owned tasks. With none, its scope is absent from `supervision_load`, so no timer is armed. Pending proposals do not make a coordinator active, unlike a workstream. Global's units leave out tasks a coordinator owns.
- **16. Each coordinator has its own wake caps.** The caps are counted per scope (`wake_usage`), so each coordinator gets 6 an hour and 40 a day, separate from Global. Phase 8 sizes the caps and should check this.
- **17. A coordinator's unseen records are not counted in `monitoring.get`.** The monitoring targets name only Global and workstreams. Before this change a project scope was reported as Global, so it would have raised the Global badge. The records still show in the coordinator's conversation. A coordinator badge would need a new monitoring target kind.

### Parent channel

- **18. The daemon writes the facts. The coordinator writes only its judgment.**
  - `observe()` turns the classification that drives the wake into a fact for an owned task. Fact ids derive from the event sequence, and facts are written before the cursor moves, so a replay writes nothing twice. A task that succeeded has no wake, so its `Succeeded` transition is read there.
  - `question_raised` for a question from `decision.ask` is written by `ask_inner`. A worker's question comes from its wait. `question_answered` is written by `decisions.answer` and `decisions.reconcile`.
  - `merged` is written by the landing thread when it records a merge.
  - These last three run after their own write has committed. A failed fact write is logged, not returned, so the user's answer or the merge still stands. Only that report is lost.
  - Facts carry no text. The notice and the snapshot render them from the verb and the ids.
- **19. The parent channel reports a pull request by its task, not by a URL.** `pull_requests` stores the number but no URL, so there is no URL to write.
- **20. Not every report starts a Global turn.** These are model wakes: a coordinator's own reports, `task_failed`, `question_raised` and `worker_blocked`. Ready, done, report ready, answered and merged are notices: the user sees them, and the next turn reads them in the snapshot. This saves turns on progress that needs no judgment.
  - Undo: return true from `report_needs_model` in `wake.rs`.
- **21. Global wakes come from a scan of unwoken rows, with a write-once `woken` flag.** They do not come from durable events: those rows belong to no task, and `durable_events` is keyed by task. Records are keyed on the row id and the flag is set after them. A crash between the two therefore writes nothing twice, and a history clear never wakes a report again. The scan handles `routed_requests` the same way, waking the coordinator.
- **22. Global's prompt never includes coordinator chat.** The global snapshot shows only these:
  - the active coordinators with their scope sentence and open counts;
  - the 10 newest reports.
  - A coordinator's note is redacted, cut to 200 bytes and labelled as data. The `coordinator_report` notice uses the same treatment.
- **23. A request has exactly one answer, and only its own coordinator gives it.**
  - A partial unique index allows one `answer` per request, and the request is open while it has none.
  - `record_coordinator_report` checks the origin proposal's scope (`conversation:project:<id>`), so Global cannot answer for a coordinator.
  - A newer pending answer card replaces an older one: the supersede subject is the request.
- **24. Removing a coordinator is refused while it owns an unfinished task (Created, Queued, Running, Waiting or Verifying) or has a routed request without an answer.** A failed task does not count: it runs nothing until it is retried.
  - The store refuses first, with `CoordinatorError::OpenWork`, which `coordinator.remove` answers as `conflict`.
  - A trigger on `coordinator_scopes` holds the same rule.

### Dependency on phase 5 (`ActionAuthority`), to re-check at merge

- `coordinator.route` and `coordinator.report` are on the left-out list of `standing_grant.rs`. When phase 5 classifies authority, `coordinator.report` should be autonomous: it only informs and changes nothing outside the parent channel. `coordinator.route` creates work for another conversation, so it is a judgment call. Classifying it autonomous fits plan 5.5, where `task.propose` becomes dispatch.
- Plan 5.5 says dispatch must cite "a user turn or an open routed request" as its origin. `tasks.routed_request_id` is that record. Phase 5's origin check should read it rather than add a column.
- A coordinator turn runs the same `start_wake`/user-turn paths as Global, so it inherits whatever phase 5 runs directly. Re-check that `within_project`, the sink's `targets_in_project` and the store's origin checks still run on the autonomous path, and not only in `actions.resolve`. If phase 5 executes from the turn without a stored proposal, `create_routed_request` and `record_coordinator_report` need a different origin proof, because today they read an approved proposal row.
- Report notes are agent text that reaches Global's prompt. That stays safe only while they are framed as data and redacted. Phase 5's security review should include this path.

### Merge notes for phases 5, 6 and 8

- Migration 0069 copies 0064's rebuild of `action_proposals` and `wake_records`. If 0065 to 0068 rebuild either table or add a kind, re-derive 0069 from the newest definition, so no column or kind is lost.
- The kind lists that every phase extends: `ActionKind::ALL` (now 16) and its tests; the store test `KINDS`; the snapshot's `offered()` and the "Action kinds:" pin; the preamble's kinds line; the `GRANTABLE_KINDS` walk; the desktop `types.ts` union and `actionCardModel`.
- The preamble grew by about 300 bytes. It stays under `MAX_INSTRUCTIONS_BYTES` (4 KiB), checked at compile time.
- Phase 6 (preferences): a coordinator write to shared preferences should be refused and reported up the parent channel (plan 5.6). `coordinator.report` with verb `caveat` is the existing channel for that.

### Not verified locally in phase 7

- These were compiled with `cargo check --all-targets` and clippy but not run, because daemon tests hang on this laptop. CI runs them:
  - the new daemon tests (`api_coordinator_tests.rs`, the two coordinator tests in `wake_tests.rs`, the charter-placement test, the procedure test);
  - the changed integration assertion in `tests/conversation_runtime.rs`: a coordinator's out-of-scope block is now `rejected`, not `stripped`;
  - the IPC user-only pins.
- Run locally and passed: `cargo test -p orchestrator-core --lib` (306) and `cargo test -p orchestrator-store` (including the new `coordinators` suite and the migration test from a 0068-style database).
- Desktop: the sub-agent ran vitest on the touched areas (1,274 tests) and `tsc`, with the main checkout's `node_modules` linked and then removed.
- No real agent turn was run. Two things are untested with a real model: that it follows the charter (idle by default, answering requests), and how well it routes against the scope sentence.
