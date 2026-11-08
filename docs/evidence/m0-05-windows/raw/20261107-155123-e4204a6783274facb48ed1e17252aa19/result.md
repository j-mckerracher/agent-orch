M0-05 WINDOWS RUN: FAIL

Run artifacts in the same directory as this file.

Checks:
exit-status-0                      PASS  exit=0
jsonl-all-lines-parse              PASS  lines=6
turn-completed-present             PASS  count=1
last-message-exact                 PASS  value='TASK_DEMO_OK'
single-agent-message-exact         PASS  value='TASK_DEMO_OK'
task-file-exact                    PASS  bytes='M0_TASK_AFTER
'
git-diff-check                     PASS
only-task-txt-changed              PASS  changed=TASK.txt
primary-checkout-isolated          FAIL  tracked identical=False; other untracked identical=True

Task repo (disposable, created by this run): C:\Users\jmckerra\AppData\Local\Temp\agent-orch-m0-05-task-fc649c2a1d9c41d59d631d8ea0274647
Task repo init commit: a4c4d5d9d6fa2cc8677768abf2cf0bd280246500
Codex exit: 0
Codex last message: 'TASK_DEMO_OK'
TASK.txt bytes: 4D 30 5F 54 41 53 4B 5F 41 46 54 45 52 0A
Primary checkout tracked status identical before/after: False
