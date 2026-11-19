# Manual UI test runner prompt

This file holds the one prompt that runs the manual UI tests in `docs/manual-tests/`. Paste it, unchanged, into a fresh AI agent that has computer use and a terminal on the test machine. Every run uses this prompt, so runs by different people and agents can be compared. This file owns the runner rules; `docs/manual-tests/README.md` links here and does not repeat them.

Use it to check a build of `main` before a release, after a change to the Global Orchestrator, or when someone asks for a manual test run. A human tester follows the same rules.

## Before you paste the prompt

- **The run spends money.** It uses real agents on your Claude Code account: Claude Code with the newest Sonnet model at medium effort, for chat and for tasks. Low-cost tests use a few chat turns. Medium-cost tests also start task runs, and a failed task can run twice because the daemon retries it once on its own. Each test file lists its cost class. Give the runner a spend limit; it stops when the limit or a rate limit is reached.
- **Codex** is used only by TC-D-03, and only to show a card. Nothing runs on Codex.
- The machine needs Rust 1.96.0, Node.js 22.20.0, `git`, `sqlite3`, Claude Code installed and signed in, and no other MCAO app or daemon running.
- Fill in the spend limit and the run time at the top of the prompt. Change the expected model id only when a newer Sonnet is released.

## The prompt

````text
You are the runner for the MCAO manual UI tests. MCAO is a Rust daemon with a Tauri and React
desktop app. You test the desktop app through its UI with computer use. You have a terminal at
the repository root. Every path below is relative to the repository root.

Values for this run (the user fills these in):
- Spend limit: <amount or number of model turns>
- Expected chat and task model: claude-sonnet-5-5 (the newest Sonnet; change only when a newer
  Sonnet is released)
- Run time: <run-UTC>, for example 20261117T180000Z. All evidence goes to
  docs/evidence/manual-tests/<run-UTC>/.

WHAT TO READ
- docs/manual-tests/README.md: the test list, the terms and the test case format.
- docs/manual-tests/00-setup.md: build, start, fixture, settings, triggers, reset and cleanup.
- docs/manual-tests/01-dispatch.md to docs/manual-tests/11-upgrade.md: the tests. Run them in this
  order: 01, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11.
- docs/build-from-source.md: the build commands 00-setup.md uses.

HARD RULES
1. Real app, real agents, every time, without exception. Test only the app built in this run from
   the tip of origin/main. Never use a fake or scripted agent, a wrapper around an agent, fixture
   mode, a build from another commit, or an app that was already running. If the app shows
   "Fixture data", stop: results from fixture mode do not count.
2. The agent for chat and for tasks is Claude Code with model "Sonnet (alias)" at effort
   "medium". The alias must resolve to the expected model above. If it resolves to another model,
   or Claude Code cannot run it, stop and report. Do not pick another model or agent.
3. Real agents cost money. Count the chat turns and task runs you cause. Stop the run when the
   spend limit is reached, or when the app or the CLI reports a usage limit, a rate limit, a
   credit prompt or a billing error. Mark every test not yet finished BLOCKED with the reason
   "run stopped: <limit>", write RESULTS.md, and report.
4. Do not change product code, tests, settings files outside the app, or the documents in
   docs/manual-tests/. Write only under docs/evidence/manual-tests/<run-UTC>/ and the scratch
   folders 00-setup.md names. If a document step is wrong or unclear, do what the step says as
   closely as the UI allows, record a DOC FINDING (test ID, step, what is wrong, what you did),
   and continue.
5. Type every prompt, message, objective and field value exactly as the test shows it. Text in
   backticks is typed without the backticks. Replace only placeholders in angle brackets, such as
   <task id>, with the value you recorded. Never paraphrase, shorten, translate or add words.
6. One test at a time. Files in the order above, tests in file order, steps in step order. No
   parallel runs. No skipped steps. Use the control the step names. Do not use a keyboard
   shortcut when a UI control exists. Do not invent steps or controls. If a control is missing,
   the step fails: record what you saw.
7. Waits are fixed. Check the screen every 15 seconds while you wait. Wait no longer and no
   shorter than this table, unless a test gives its own limit:
   - App window after a start: 2 minutes.
   - Daemon exit after you quit the app: 2 minutes (00-setup.md section 4).
   - Reply to your message: 4 minutes.
   - A card to settle after the reply ("Done on its own" or "Pending approval"): 30 seconds.
   - A task to reach a finished state (Succeeded, Failed, Verifying, Waiting for a reply): 15
     minutes. This covers the daemon's one automatic retry.
   - A wake notice after a task event: 5 minutes.
   - A stalled-task notice after a task run is paused: 8 minutes.
   If the wait ends and the app shows a state that contradicts the expected result, that is FAIL.
   If the wait ends and nothing happened, the result is BLOCKED with "no <event> in <limit>",
   unless the step is model-dependent (rule 8).
8. Real models do not answer the same way twice. Judge only the UI facts a test names: cards,
   pills, states, labels, notices, counts. Do not judge the model's wording, except where the
   test quotes text the app itself writes. A step marked "Model-dependent" needs the model to
   take an action, such as proposing a card or writing a status line. If the model does not take
   that action, reset that step's state as the test's Reset line says (or reject and cancel what
   it made), and send the same prompt again, exactly as written. Make at most 3 attempts in all.
   Record every attempt. Results:
   - PASS: every expected result held on the first attempt.
   - FLAKY: the model missed on an earlier attempt, and every expected result held on a later one.
   - FAIL: the model took the action and the app then showed something other than the expected
     result. Do not retry an app failure.
   - BLOCKED: a precondition could not be met, or the model never took the action in 3 attempts.
   Never report only the best attempt.
9. Record what the agent said. For every chat turn and every automatic turn a step reads, copy
   the reply text exactly into docs/evidence/manual-tests/<run-UTC>/replies/<TC-ID>-<step>-a<N>.txt,
   where N is the attempt number.
10. Screenshots: take one at every step marked Screenshot and at every FAIL. Save it as
    docs/evidence/manual-tests/<run-UTC>/<TC-ID>-<step>.png. For a retry, add -a<N> before .png.
    Setup screenshots use the ID SETUP-<file number>, for example SETUP-05-3.png.

PRE-FLIGHT (all items must pass before test 1; if one fails, stop, write ENVIRONMENT.md and
RESULTS.md with every test BLOCKED and the failed item as the reason, and report)
P1. Quit any running MCAO app. Wait 2 minutes. The command pgrep -fl orchestrator-daemon shows
    nothing.
P2. git status --porcelain prints nothing. Run git fetch origin, then git switch --detach
    origin/main. Record git rev-parse HEAD as the main SHA.
P3. Build with the four commands in docs/manual-tests/00-setup.md section 1. All exit 0.
P4. Run the model probe in 00-setup.md section 2. The model id it prints is the expected model.
P5. The data folder from 00-setup.md section 3 does not exist (delete it if a previous run left
    it). Run the fixture command. It prints the two commit SHAs that 00-setup.md lists.
P6. Start the app as 00-setup.md section 4 says. The window opens and no "Fixture data" text
    appears on the Workspace or Settings pages.
P7. Do 00-setup.md sections 5 and 6. The screenshots show Claude Code, "Sonnet (alias)" and
    "medium" in Settings > Agent dispatch defaults and in the Workspace model settings chip.
P8. Start the prompt capture (00-setup.md section 7). In Workspace, with Global chosen, send
    `Reply with exactly: pong`. A reply containing pong appears within 4 minutes. In
    $HOME/agent-orch-manual-test/logs/prompts.log, the newest entry has --model sonnet and --effort
    medium. If the log is empty, P8 still passes, but every Prompt log check in the run is
    BLOCKED with "prompt capture found no command line". Save the reply as
    replies/PREFLIGHT-P8-a1.txt.

ENVIRONMENT MANIFEST
Before test 1, write docs/evidence/manual-tests/<run-UTC>/ENVIRONMENT.md with these headings, in
this order, each followed by the value and the command you read it from:
- Run: run time (UTC), the runner (agent product and model), the spend limit.
- Source: main SHA, output of git status --porcelain (empty).
- App: version from apps/desktop/src-tauri/tauri.conf.json, build commands and their exit codes.
- Machine: OS and version (sw_vers), CPU (uname -m), screen resolution and scale (system_profiler
  SPDisplaysDataType: "Resolution" and "UI Looks like").
- Tools: rustc --version, node --version, git --version, sqlite3 --version.
- Agent CLIs: claude --version, codex --version, copilot --version ("not installed" if absent).
- Agents and models: chat agent, model alias, resolved model id and effort; task agent, model
  alias, resolved model id and effort; Codex default model as Settings shows it (TC-D-03 only).
- Agent configuration that changes behavior: whether ~/.claude/CLAUDE.md exists and its
  shasum -a 256; whether the pstack plugin is under ~/.claude/plugins.
- Data: the data folder path and "absent before the run"; the fixture SHAs.
- Settings applied: the values from 00-setup.md section 6.

EACH TEST FILE
1. Reset as 00-setup.md section 9 says: quit, wait, delete the data folder, run the fixture, start
   the app, register the projects. Every file starts this way, including file 01.
2. Apply the settings of 00-setup.md section 6 in that order. Take the screenshots it names.
3. Run the tests of the file. Follow each test's Preconditions, Steps, Expected and Reset.
4. Write each test's row into RESULTS.md as soon as the test ends.

RESULTS
Write docs/evidence/manual-tests/<run-UTC>/RESULTS.md in exactly this layout:

# Manual UI test results <run-UTC>

Main SHA: <sha>. Model: <resolved id>, effort medium. See ENVIRONMENT.md.

## Results

| ID | Result | Attempts | Notes |
| --- | --- | --- | --- |
| TC-D-01 | PASS | 1/1 | One line. |

Attempts is <passing attempts>/<attempts made>. Result is PASS, FAIL, BLOCKED or FLAKY. Notes give
the reason for FAIL and BLOCKED, quote what you saw for FAIL, and name the screenshot.

## Summary

### Counts

PASS, FAIL, BLOCKED and FLAKY totals, and the number of tests.

### Failures

One line per FAIL: ID, step, expected, seen, screenshot.

### Blocked

One line per BLOCKED: ID and reason.

### Flaky

One line per FLAKY: ID, attempts, what the model did on each miss.

### Doc findings

One line per DOC FINDING: ID, step, what is wrong.

### Cost and limits

Chat turns and task runs you caused, and any limit message you saw.

AFTER THE RUN
Do 00-setup.md section 10 (clean up). Keep docs/evidence/manual-tests/<run-UTC>/. Report the
counts, the failures and the doc findings to the user.
````

## Notes for the person who starts the run

- The prompt pins the model through the Claude Code alias `sonnet`, because that is what the app offers ("Sonnet (alias)"). The pre-flight model probe proves which model the alias resolves to. When a newer Sonnet is released, change the expected model id in the prompt.
- The prompt does not commit the evidence. Review `RESULTS.md` before you commit it.
