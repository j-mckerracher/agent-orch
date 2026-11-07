M0-05 MACOS RUN: PASS

Run artifacts in the same directory as this file.
Checks:

jsonl-all-lines-parse        PASS  lines=7
exit-status-0                PASS  exit=0
turn-completed-present       PASS  count=1
last-message-exact           PASS  'TASK_DEMO_OK'
single-agent-message-exact   PASS  'TASK_DEMO_OK'
task-file-exact              PASS  hex=4d305f5441534b5f41465445520a
git-diff-check               PASS  ''
only-task-txt-changed        PASS  changed=['TASK.txt']
primary-checkout-isolated    PASS  tracked-identical=True, other-untracked-identical=True

Task repo (disposable, created by this run): /var/folders/0v/gssk9rmx4c1465sfxcz668d00000gr/T//agent-orch-m0-05-task-RLM7hW
Codex exit: 0
Codex last message: 'TASK_DEMO_OK'
TASK.txt bytes: 4d305f5441534b5f41465445520a
Primary checkout tracked status identical before/after: True
