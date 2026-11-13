# Targeted G07 rerun setup notes

Attempt 1: Git checkout settings were applied after checkout. The exact candidate SHA matched, but line-ending changes made the working tree appear modified. No test command ran.

Attempt 2: The clone command received git -c settings, which were not persisted in local config. SHA, tree, and working-tree status were correct, but the explicit config check stopped before testing.

The same isolated clone was then configured locally with core.autocrlf=false and core.longpaths=true. Its exact candidate SHA and clean tree were verified before G07 ran. The original setup diagnostics remain in the disposable local run roots; this evidence branch records the setup sequence and the final valid run.