# Remote crew hosts: threat model (DPES-1105, Phase 6c)

Status: design, revision 1. No feature code. Parent: DPES-940. Companion to [orchestrator-actions.md](orchestrator-actions.md).

Basis: the central worktree at `73586b6`, on branch `feat/DPES-1105_orchestrator-actions-phases-2-6`. That branch was built from `main` at `453c235`. Code facts below were read from that tree. No build or test ran. No remote host was set up or tested. No CLI was run.

## Status and scope

### Status

This document is design only. It covers Phase 6c, which is the gap in row F4 ("Remote secondmates on an SSH host") of [the capability map](orchestrator-actions-firstmate-map.md). It builds nothing and promises no date. The parent design already says the IPC is a local socket with a token handshake, so a remote daemon needs an authenticated network transport and a new threat model. This is that threat model.

Its job is to state what any remote design must withstand. The user can then decide whether they want one. If they do, nobody writes remote code until the requirements in this document have tests.

### What is in scope

- The case where the Global orchestrator on the user's desktop reaches crew agents that run on another machine, over SSH or a network.
- The threats, the requirements a design must meet, the options, and a recommended direction.

### What is out of scope

- A hosted or multi-tenant service. MCAO operates no server in this design.
- More than one person. Every host here belongs to the one user. Another person's machine is not a crew host.
- Exposing the existing daemon IPC on a network port.
- Browser, phone or other clients that reach the daemon.
- Sandboxing agents on a crew host. `docs/security.md` defers sandboxing, and this document does not change that.
- Hosting the Global orchestrator, workstream conversations, approvals, grants or the audit log anywhere but the home daemon.
- The wire format and any code.
- Relays of chat replies to public channels. Decision N6 declined those. That decision is about replies, not about this transport, and this document does not rely on it.

### Words used here

- Home daemon: the daemon on the desktop machine. The desktop app talks to it, it holds the store, and the user approves from it.
- Crew host: another machine that runs agents for the home daemon. It would run its own daemon.
- Crew channel: the connection between the home daemon and a crew host.
- User-only method: a daemon method that only the user may cause. Examples are `actions.resolve` and `approvals.resolve`. Phase 5 adds `grants.*`. Piece 0b decides the full list.

## Today's trust model

This is what the code on the basis commit does. Nothing in it assumes a network.

### Where the daemon listens

On macOS the daemon listens on a unix socket at `ipc/daemon.sock` under the data directory. The directory has mode 0700 and the socket has mode 0600. The code checks the owner before it reuses or removes either.

On Windows the daemon listens on a named pipe. Its protected DACL grants access only to the current user's SID. The pipe rejects remote clients (`PIPE_REJECT_REMOTE_CLIENTS`) and allows at most 16 instances.

There is no TCP port. Only a process that runs as the same OS user on the same machine can connect. The Unix side relies on file permissions. No peer credential check was found in `unix_transport.rs`.

### The token

The token is 32 bytes from OS randomness, written as hex to `daemon.token` in the data directory. The file has mode 0600 on Unix and a protected DACL on Windows. The daemon creates a new token at each start and removes it on orderly shutdown. The daemon compares it in constant time.

The first request on every stream must be a `handshake` that carries the token. A wrong token closes the stream with code -32001.

The token is a bearer secret. Whoever holds the bytes is the caller. One token serves every client of one daemon run. It has no expiry and no binding to a connection.

### The capability handshake

The handshake also lists protocol versions and capabilities. The daemon picks the highest common version. It grants only the capabilities that it implements and the client asked for. An unknown capability name is refused (-32002). A method outside the negotiated set is refused.

Capabilities limit what a session may ask for. They do not say who the caller is. For example, a session without `orchestrator_actions` gets -32004 on `actions.resolve`, but any session that negotiates it may call it.

### The desktop host

The Tauri host reads the token, opens the connection and forwards JSON frames for the webview without interpreting them. The webview never sees the token or the socket. The host refuses `handshake` and `shutdown` from the UI. It refuses no other method, so everything else the webview sends reaches the daemon.

The daemon runs on demand. The host starts it. It exits after 60 seconds idle unless active work keeps it alive.

### What the daemon does not know

- It has no caller identity. Any process that holds the token can call any method, including `actions.resolve` and `approvals.resolve`. At Auto and Full the agent has a shell and could read the token. Actions exist for Restricted and Auto turns only; the user accepted the Auto limit so the default chat level can propose.
- On Windows the client does not verify the server before it sends the token. This is a known limit from the M6 threat review, written in `windows_transport.rs`. Locally it is contained by the pipe name and the DACL. Across a network it would not be.
- Agent children run as the user with an environment allowlist. That is hardening, not containment. `AGENT_ORCH_` names are never forwarded.

### The approver credential

A desktop-only approver secret is being added in Phase 0 (piece 0b). It exists to give the daemon a caller identity for user-only methods. This document does not describe its mechanism. It depends on one rule only: the secret stays on the desktop host (R5).

`plan-0b.md` did not exist in the run notes when this document was written, so nothing here was checked against it. Check the requirements against it before anyone builds.

### Promises the actions design already makes

The actions design promises that no mutating action runs without a per-action approval or a matching standing grant, whatever the agent read. It does not promise that an agent never proposes something harmful, or that a user never approves a misleading card. A remote design must keep the first promise exactly. The second gap gets wider with a remote host, and T5 covers it.

## Assets

| Asset | Where it lives today | What an attacker wants it for |
| --- | --- | --- |
| Source code | Project repositories (`root_path`) and task worktrees under `worktrees/` in the data directory | Theft, or planting code that is later delivered |
| Credentials | Provider keys and sign-ins held by agent CLIs, git and `gh` credentials used for delivery, SSH keys, anything in the user's home, values forwarded by `AGENT_ORCH_ENV_PASSTHROUGH` | Publishing as the user, spending money, reaching other systems |
| The approval decision | `approvals`, `action_proposals`, their events, and Phase 5 grants, all in `store.sqlite` | Making the daemon do what the user never allowed |
| Agent session output | `sessions.events`, the `durable_events` journal, conversation history | It holds code, command output and secrets that tools echoed. It can also steer the orchestrator |
| Task and attempt state | `store.sqlite` | Deciding what counts as done or verified, and so what reaches delivery |
| The data directory | `store.sqlite`, `daemon.token`, `ipc/`, `daemon.log`, `worktrees/`, run directories | The token gives authority over the whole daemon. The rest is the other assets |
| Layout and identity | Paths, user name, host names, git author | Reconnaissance and targeting |
| Spend and capacity | Provider token budgets, Copilot credits, wake-turn tokens (N9), CPU on each host | Cost and denial of service |
| Channel keys (new) | Per-host keys and the store of pinned host keys | Impersonating a host or the home daemon |

## Actors and trust boundaries

### Actors

| Actor | Trusted to | Not trusted to |
| --- | --- | --- |
| The user | Be the only source of new authority | Never approve a misleading card |
| Desktop host and webview | Hold the token and the approver secret, forward UI frames | Check the content of frames |
| Home daemon | Own state, enforce policy, write the audit log | Know who is calling (until piece 0b) |
| Global orchestrator turn | Nothing. Its output is a proposal | Anything it read or wrote |
| Crew host, healthy | Run the attempts it is told to run | Hold authority or decide anything |
| Compromised crew host | Nothing | Report anything truthfully |
| The network | Nothing. Assume an attacker can read, change, drop, delay and replay frames | Anything |
| Malicious repository | Nothing. Its files, hooks and text are attacker input | Anything |
| Prompt-injected agent, local or remote | Nothing. It acts with its host account's permissions | Anything |

### Trust boundaries

- B1. User to desktop host. Unchanged by a remote design.
- B2. Desktop host to home daemon. Local IPC with the token, and later the approver secret. Unchanged.
- B3. Home daemon to its local agents. Child processes run as the user. A worktree is source isolation, not a sandbox. Unchanged.
- B4. Home daemon to a crew host, across a network. New. The far side is outside the home's control: its code, its clock, its keys and its repositories.
- B5. Crew daemon to its agents and repositories. This is B3 again, on a machine the user may manage less carefully. The home must survive its compromise.
- B6. Repository content to agents and to prompts. It exists today. A crew host adds repositories that the user does not look at directly.

## Threats

Today's mitigation names what the code on the basis commit does. The last column names what a remote host removes or changes.

| ID | Threat | Asset | Today's mitigation | What a remote host breaks |
| --- | --- | --- | --- | --- |
| T1 | Network eavesdropping or tampering | Source, session output, approvals in flight | No network listener. The socket sits in a 0700 directory and the Windows pipe rejects remote clients, so traffic stays on one machine | Frames cross a network the user does not control. An attacker can read, change, drop or reorder them. The frame format has no encryption or integrity of its own |
| T2 | Stolen or replayed token | Authority over every daemon method, including approvals | A random 32-byte token, mode 0600 or a protected DACL, constant-time compare, a new token at each start, only same-user processes can read it | The token is a bearer secret with no connection binding, no expiry in a run and no caller identity. Copying it to another host, or reading it over a link, extends same-user trust to every process on that host and every hop. A captured handshake replays whole |
| T3 | A rogue host impersonates a crew host | Task contracts and prompts, the orchestrator's trust | The daemon is found by data directory. Only the owning user can create the socket. On Windows the first pipe instance is claimed. The Windows client does not verify the server before it sends the token | Nothing proves who answers at a network address. A rogue host receives contracts and returns plausible results. A client that sends the token first hands it to whoever answers |
| T4 | A compromised crew host forges results or events | Task state, delivery approvals, orchestrator context | The engine observes facts (candidate commit, verification) in the same trusted process. State changes only through `task::transition`. Agent output is untrusted | The facts now arrive from another machine's daemon. "Verification passed" and "candidate commit abc" are claims the home cannot check alone. A forged success can reach an approval card |
| T5 | Confused deputy: a remote agent makes the user approve something misleading | The approval decision | Cards show canonical parameters, not agent prose. An approval binds a digest of the parameters and the candidate commit. Actions come from Restricted turns only. The user is the only approver | Names, titles, summaries, branch names and diffs now come from a host the user cannot inspect. Two hosts can hold a project with the same name. A card can show host-supplied text as if the home held it. The user may not notice which host will run the work |
| T6 | Approval spoofing | The approval decision, grants | One local approver. The allowlist refuses an agent actor, but only as a unit-level check. The daemon has no caller identity on the wire, which is why actions need Restricted. A desktop-only approver secret is being added in Phase 0 | A crew channel is a second place to inject `actions.resolve` or a grant request. A crew daemon has its own local approvals, which could be shown as home approvals. A host-sourced event could be displayed as "approved by the user" |
| T7 | Secret exfiltration through prompts or snapshots | Credentials, source, environment | The snapshot and preamble never carry paths, tokens, environment, credentials, diffs or session output. Children get an environment allowlist. The log redactor masks the local token by value | The redactor knows only local secrets. Remote agent output and tool results flow into the home's journal, conversation history and wake turns, so a secret echoed on a crew host lands in the home store and prompts. Anything the home sends to a host is visible to that host's agent, its logs and its model provider |
| T8 | Path and identity leakage across hosts | Layout and identity | Paths stay local. The snapshot excludes them. Project and workspace records with paths go over local IPC to the desktop only | Forwarding project or workspace records hands a host the home's layout and user name. A host's paths and names enter the home store. Two hosts can report the same path text for different places |
| T9 | Clock skew and replay of idempotency keys | Exactly-once effects, expiries | One store and one clock. Keys are 1 to 128 characters and are bound to a method and a payload fingerprint. The same key with a different payload is a conflict. The `action-` prefix is reserved. No code that expires idempotency records was found | Two stores mean two key spaces. The same key sent to two hosts can run twice. A key captured and resent on a new connection can hit an unrelated record. Host timestamps disagree with the home's. Expiry of approvals, grants and wake debounce could be judged on the wrong clock |
| T10 | Partial failure and split brain on task state | Task state, branches, delivery | One daemon writes all state. Restart reconciles. An approval is consumed in the same transaction as its delivery, so a crash between them leaves it unconsumed | A partition leaves the host's agent running while the home sees silence. The home may mark the task failed and retry elsewhere. Two attempts can then write one branch. An approval bound to a commit can meet a different commit. Delivery can happen twice |
| T11 | Denial of service | Availability, spend | Local only. One frame is at most 1 MiB. Stream timeouts are 2 seconds. A Windows pipe allows 16 instances. Event buffers hold 256. On Unix each accepted stream gets a thread, and no cap on sessions was found | A network listener faces anyone who can reach it before authentication. A host or an attacker can open many connections, flood events, hold streams open, trigger wake turns that burn tokens (N9 is unlimited by default) or run up spend |
| T12 | Prompt injection carried from a remote repository or agent | Orchestrator decisions, approvals | Text in the snapshot, reports and history is data, not instruction. No mutating action runs without approval. Proposals are rate-capped per turn | Remote reports and events become input to the Global orchestrator from repositories the user has not inspected. There is more volume, and it is harder to see which host sent what |
| T13 | A malicious repository on a crew host | The crew host's credentials, other repositories, its data directory | A worktree is source isolation. The agent runs as the user. The blast radius is the user's own machine | A crew host may be shared or less managed. A compromised agent reaches that account's keys. If one of them reaches the home or another crew host, the compromise spreads along the channel |
| T14 | Host key theft or missing revocation | Channel keys | Nothing to revoke. The token changes at each daemon start | Host keys are long-lived. A stolen key stays valid until someone removes it. Nothing lists which hosts are authorized or tells the user a host is gone |
| T15 | Grant scope confusion | Unattended authority | Phase 5 design: a grant names one project or workstream, kinds, an expiry and a use count, and the never-grantable list holds | A project id or name can also mean a clone on a crew host. A grant made for the home project could cover unattended work on a machine the user never named, with credentials the user never considered. A host added later could inherit it |
| T16 | Audit gaps across hosts | The ability to say what ran and where | Append-only `conversation_audit`, `action_proposal_events`, approval events and the `durable_events` journal, all in one store | Events about one task are written in two stores with two clocks. A host can drop or reorder its side. Nothing records what the home sent. A revoked host's history can vanish |
| T17 | Downgrade: an older or modified crew daemon | Credentials, source | The hard limits in `agent-permission-levels.md` live in the daemon's argv and environment code. The desktop starts a bundled daemon, and release builds never run a path taken from the environment | The hard limits are enforced where the agent CLI starts. On a crew host that is a binary the home did not ship. An old or altered daemon can authenticate and still ignore the limits |

New with a remote channel, with no local counterpart: T1, T3, T4, T10, T14, T15, T16 and T17. The rest exist today and get worse.

## Requirements

Every item says "must" and ends in a test. A remote design must show a passing test for each item before code merges. Each item names the threats it closes.

### Identity and keys

- R1. Mutual authentication with per-host keys. Before any frame beyond the handshake, both ends must prove their identity with a key that belongs to that one host. The home proves itself to the crew host, and the crew host proves itself to the home. A key must name exactly one host. Closes T1, T3, T14. Test: a connection with a wrong or unknown key, in either direction, closes before any method runs. A host that presents host A's key while it claims to be host B is refused. No frame is sent before the check passes.
- R2. No shared long-lived token across hosts. `daemon.token` must never be sent to another host, stored on another host or accepted from one. Each host has its own credential, and a credential for host A must not work for host B. The channel credential is not the daemon token. Closes T2, T14. Test: with two hosts configured, host A's credential is refused by host B. A recording transport over a full session never contains the token bytes.
- R3. The home opens every connection. A crew host must hold no credential that reaches the home daemon, the desktop host or another crew host. The channel must not forward an agent or any credential. Closes T13, T3. Test: the channel starts with agent forwarding and port forwarding off, and the home checks that before it sends a frame. No listener on the home accepts a connection from the network.
- R4. Revocation of a host. The user must be able to revoke a host from the desktop. Revocation must take effect for new messages at once, close open channels within a stated bound, delete the host's pinned key and credentials, and be audited. Tasks on that host are then reported as host unreachable. They must not start on another host without a new proposal and approval. Closes T14, T10. Test: revoke while a task runs. The next frame to or from the host is refused, the open channel closes within the bound (a candidate bound is 5 seconds), and a retry elsewhere needs a new approved proposal.

Candidate mechanisms for R1:

- SSH public-key authentication. Both ends are authenticated: the crew host's sshd checks the home's key, and the home checks the host key. The link is encrypted and integrity-protected, and the protocol resists replay. It needs an OpenSSH client at home and an sshd on the crew host. A key can be limited to one forced command. MCAO adds no cryptographic code.
- mTLS with per-host certificates. Both ends are authenticated by certificates. It needs a certificate authority or pinned per-host certificates, issuance, rotation and expiry handling. It also needs a TLS library in a listener that faces the network before authentication, and an open TCP port on each crew host.
- A pinned host public key on a custom channel. Pinning authenticates the host only, which is one direction. The home would still need its own credential toward the host, and MCAO would own the handshake and the cryptography. Pinning is a good part of a design but not a mechanism by itself.

Recommendation: SSH, with a separate key pair for each host and the host key pinned in a store that MCAO owns, not the user's `known_hosts`. Reason: SSH already gives mutual authentication, encryption, integrity and replay resistance, and it adds no parser that faces the network before authentication. mTLS is the fallback for a host that has no sshd.

### Authority

- R5. The approver credential never leaves the desktop host. It must not be sent over a crew channel, derived on a crew host or requested through one. No crew host may hold an approver credential that the home accepts. Closes T6. Test: record every frame in a run that approves an action and a delivery. The secret bytes appear in none of them. The crew channel's method allowlist has no method that returns or accepts the secret.
- R6. User-only methods are never accepted over a remote channel. The methods are at least `actions.resolve`, `approvals.resolve`, `grants.*` and whatever piece 0b classes as user-only. They must be refused on any session that arrived over a crew channel, whatever credential the session holds. The home must never send them to a crew host. The crew channel has its own allowlist, and a method is denied unless it is listed. Closes T6, T15. Test: a table-driven test states, for every method in the daemon's method table, whether the crew channel allows it. A method that is not classified fails the test. Each user-only method gets a typed refusal over the crew channel and changes nothing.
- R7. Approval cards show the host and home-held facts. A card for work on a crew host must show the host name as the home holds it, and the project and task names the home holds. Text that a host supplied (titles, summaries, branch names, diffs) must be labeled host-reported. It must never be a card heading, a button name or a digest-bound parameter without being shown verbatim. Closes T5, T12. Test: a fixture host sends a task title that names another host and project. The card still shows the true host and project, and the accessible name of the Approve button holds the home-held host name.
- R8. Grants never extend to a remote host unless the user names that host. A Phase 5 grant applies to the home daemon only, unless it lists a host id. A grant that lists a host shows the host name on its header chip and on each card. A grant never covers a host added or renamed after the grant was created. Revoking a host ends its grants. The never-grantable list applies unchanged on a crew host. Closes T15. Test: take a grant for project P that names no host. A proposal for P on host H gets a normal card. A grant that names H covers it, and each use is audited with the host id. Adding host H2 changes neither result. Revoking H stops the grant for H.

### Scope and data

- R9. Per-host and per-project scope. Each host record lists the projects it may work on. The home must send a host only ids and data for those projects. It must refuse and audit any message, in either direction, that names another project, workstream, task or conversation. A host is never told that other projects or hosts exist. Closes T7, T8, T15. Test: host A asks for the events of a task in host B's project and gets a typed refusal. A snapshot sent to host A contains only ids in A's scope.
- R10. No raw paths or secrets in anything sent to a crew host. Messages carry opaque ids and per-host aliases. The crew host resolves an alias to its own path. Contracts, prompts and snapshots sent to a host follow the Phase 1 snapshot rule: no paths, tokens, environment, credentials, diffs or session output. The home must refuse to send a contract that holds an absolute path or a credential-shaped value. Closes T7, T8. Test: scan every outbound message of a fixture run for the home's worktree paths, data directory, user name, host name, token bytes and environment values. None are found. A contract with an absolute path is refused before it is sent. Values listed in `AGENT_ORCH_ENV_PASSTHROUGH` never appear.
- R11. Host data stays labeled and is never forwarded to another host. Everything a host reports (paths, user names, text, timestamps) is stored with its host id and shown as host-reported. It must not enter a snapshot, prompt or message for any other host. Secrets the home knows (its token, its approver secret, each host's channel credential) must never appear in the home log, journal or conversation, even when a host echoes them. Closes T7, T8, T12. Test: a fixture host reports an absolute path and echoes the home's token. Neither appears in any message to a second host. The token does not appear in the home log or journal.

### Integrity and replay

- R12. Idempotency across hosts. Every mutating cross-host message must carry a key built from home-owned stable ids and the host id, for example host id, task id, attempt number and message kind. The key must fit the existing 128-character limit, so it is hashed. Both sides store the key before the effect. The key is bound to the method and a digest of the parameters. The same key with different parameters is a conflict. The same message resent after a partition, a restart or a reconnect has one effect. Closes T9, T10. Test: send each mutating message twice on one connection, across a reconnect and across a crew daemon restart. Each gives one effect. The same key with a changed payload is a conflict. The same task id on two hosts does not collide.
- R13. Replay protection and clocks. Each connection must bind its messages to that connection with fresh values from both ends, so a frame captured on one connection fails on another. Mutating messages also carry a unique id that the receiver records. No decision may depend on the two clocks agreeing. Expiries (approvals, grants, wake debounce) are judged on the home clock only. Ordering uses sequence numbers the home assigns. A host's timestamps are shown as host-reported. Closes T2, T9, T16. Test: replay a captured mutating frame on a new connection and on the same connection. Both are refused. Set the crew host's clock one hour ahead and one hour behind. No expiry, ordering or grant decision changes.
- R14. One owner per task, with the home as system of record. Each task has exactly one owning daemon, recorded when the task is created. Only the home writes state for tasks it owns. A crew host reports facts (events and attempt results), never state. A message about a task the host does not own is refused. Loss of the connection is a connection event, host unreachable, and never a task state, as the desktop bridge already treats connection loss. A task on an unreachable host must not start on a second host without a new approved proposal. On reconnect, the crew host's view is reconciled to the home's record, never the reverse. Closes T4, T10. Test: partition during a running attempt. The task stays running in the home's record, the host shows unreachable and no second attempt starts anywhere. After reconnect, a late "succeeded" for a task the home cancelled is stored as an event and changes no state. A forged event for another host's task is refused.
- R15. Results from a host are untrusted facts. A task must not become succeeded on a host's report alone. The home must fetch a reported candidate commit by hash, and every card must say whether verification ran at the home or is host-reported. A delivery approval must bind the commit hash that the home itself fetched, which is the existing approval-integrity rule. Closes T4, T5. Test: a forged succeeded event leaves the task not succeeded. A reported commit hash that cannot be fetched blocks the delivery request. A card for host-reported verification says so.
- R16. Audit of every cross-host message. For each message sent to or received from a host, the home must append one record with the host id, direction, method, a digest of the parameters, the outcome, the home timestamp and a correlation id. The table is append-only: triggers refuse update and delete, as for `conversation_audit`. It keeps digests, not content. The record must be written before the message is sent, and a failed write means the message is not sent. Revoking a host must not remove its records. Closes T16, T6. Test: the message count on a recording transport equals the audit row count. An induced audit failure sends nothing. Update and delete are refused. Records for a revoked host remain.

### Limits and versions

- R17. Bounded resources. The channel must have a handshake timeout that applies before any untrusted data is parsed, a frame size cap (the existing 1 MiB or less), a cap on concurrent channels per host and in total, a rate limit per host on inbound events and outbound commands, bounded event buffers, and a spend ceiling for work on a host. Events from a host must pass through the same debounce and the same wake-turn limits as local events (N9, N11), so a noisy host cannot make the orchestrator burn tokens. Closes T11, T12. Test: an unauthenticated connection closes within the handshake timeout. The connection past the cap is refused. A host that sends ten times the allowed event rate is throttled and the user sees a notice. Wake turns that a host triggers never exceed a cap the user set.
- R18. Version and hard-limit check. The handshake must report the crew daemon's version, and the home must refuse a crew daemon older than the release that carries the hard limits in `agent-permission-levels.md`. The home must also refuse a version it does not recognize. Closes T17. Test: a fixture crew daemon that reports an old or unknown version is refused with a typed error before any task is sent.

## Options considered

All four options must meet the requirements above. The question is which one gets there with the least new code and the smallest attack surface.

### Option 1: SSH to a unix socket

There are two forms.

- Form A, raw forward. Forward the crew daemon's unix socket to a local socket with `ssh`. The home then speaks today's protocol to the crew daemon with the crew daemon's token.
- Form B, forced-command channel. A key is limited to one command that starts a small proxy on the crew host. The proxy talks to the local crew daemon over its local IPC, using the crew host's own token. It reads that token locally and never sends it. The proxy enforces a method allowlist and the project alias map. The protocol runs over the SSH session's standard input and output.

Needs:

- OpenSSH on the home machine and an sshd on the crew host.
- One key pair per host and a store of pinned host keys that MCAO owns.
- A crew daemon on the crew host.
- For Form B, the proxy and the allowlist as new code.

Risks:

- Form A gives the home a client with the full crew daemon surface under one token. The token must be read over SSH and stored at home. Any process of the same account on the crew host can use the same token. User-only methods stay reachable, and paths flow back unchanged. It fails R2, R6, R9 and R10.
- Form B adds new security-sensitive code on every crew host. An SSH misconfiguration, such as agent forwarding or a shell, breaks R3 without any visible sign, so the home must check the options on each connection. A Windows crew host needs an sshd, which was not checked.

Verdict: reject Form A. Form B is the recommended direction if the user wants remote crew hosts.

### Option 2: mTLS over TCP

Needs:

- A listener on each crew host, with a TCP port the home can reach.
- A certificate authority or pinned per-host certificates, plus issuance, rotation, expiry and revocation.
- A TLS library and certificate parsing in the daemon, before authentication.
- A way to put keys on hosts, and a path through firewalls and NAT.

Risks:

- It is the largest new network-facing surface, and it sits inside the daemon's own process.
- Certificate checks, such as host name and expiry, are easy to get wrong.
- Keys at rest are readable by any process of the same account.
- It needs more code than Option 1 Form B for the same result.

Verdict: fallback only for a host that has no sshd. Do not build it first.

### Option 3: outbound-only relay

Each crew host opens an outbound connection to a relay. The home connects to the same relay. No machine accepts an inbound connection.

Needs:

- Someone to run the relay. The user could host it. A service run by MCAO is out of scope.
- End-to-end authentication between the home and each host anyway, so the relay cannot impersonate either one.
- A long-lived outbound credential on each crew host.
- Queueing and delivery rules for messages that wait at the relay.

Risks:

- The relay is a new trusted party. It sees who talks, when and how much, and it sees content unless the traffic is end-to-end encrypted.
- It is a single point of failure and a denial-of-service target.
- It removes none of R1 to R18 and adds a component.
- The crew host's relay credential is another long-lived secret (R2), and it gives a crew host a path toward the home (R3).

Verdict: reject for now. Reconsider only if a crew host cannot accept inbound SSH and the user runs the relay.

### Option 4: do not build it

Needs: nothing. F4 stays a gap. A user who wants a second machine runs MCAO there, with its own desktop, daemon and orchestrator. Nothing crosses machines.

Risks: no cross-host orchestration, and the user switches between two apps. The crew host stays out of the Global orchestrator's view. It adds no threat.

Verdict: this is the default. Choose it unless the user names a concrete need, such as a machine with a specific toolchain, GPU or operating system.

### Summary of the options

| Option | Mutual authentication | Inbound port on a crew host | New code | Verdict |
| --- | --- | --- | --- | --- |
| 1A SSH socket forward | Yes, by SSH | Port 22 | Little | Reject |
| 1B SSH forced-command channel | Yes, by SSH | Port 22 | A proxy and an allowlist | Recommended if built |
| 2 mTLS over TCP | Yes, by certificates | A new daemon port | TLS, certificates, a listener | Fallback only |
| 3 Outbound-only relay | Needs its own layer | None | A relay and end-to-end auth | Reject for now |
| 4 Do not build | Not applicable | None | None | Default |

## Recommended direction

### Direction

Keep F4 as a gap until the user decides they want remote crew hosts. If they do, build Option 1 Form B. The picture below shows the layers.

```text
Desktop host (the user, the approver secret)
   |  local IPC with the token
Home daemon (state, audit, grants, approvals)
   |  SSH: per-host key, pinned host key, forced command
Crew-channel proxy on the crew host (method allowlist, project aliases)
   |  local IPC with the crew host's own token
Crew daemon (runs agents in worktrees on that host)
```

The approver secret, the token, grants, approvals, conversations and the audit log stay in the top two layers. A crew host can run attempts and report facts. It cannot decide anything.

### Order of work if the user wants it

1. The user answers the decisions below. Nothing starts before that.
2. Finish the prerequisites that are not remote work: piece 0b merged with wire-level tests, Phase 5 grants with scope, and the method classification table that R6 needs.
3. Write the host record and the crew-channel allowlist as a spec, still without code. The host record holds a host id, an alias, the pinned key fingerprint, the project scope and a state. Review the spec against R1 to R18.
4. Build read-only observation first. The home can see a crew host's status and events, and nothing goes out except reads. This proves R1, R2, R3, R9, R10, R11, R13, R16 and R17 with the least authority. Inbound data is the first untrusted input to prove out.
5. Add commands next: submit a task to a host by alias, and cancel it. This proves R4, R7, R8, R12 and R14.
6. Add result handling: the candidate commit reaches the home and is verified or labeled (R15). Delivery stays at the home.
7. Run the failure drills before real use: partition, clock skew, replay, revoke during a run, and a daemon restart on each side. Each drill is an automated test, not a manual check.
8. Only then allow a grant that names a host (R8).

### Decisions the user needs to make

- D1. Which hosts, and who owns them: only the user's own machines, or also shared or rented ones? Shared machines raise T13.
- D2. Which operating systems run crew hosts? macOS and Windows differ in SSH and file permissions, and sshd on Windows was not checked.
- D3. Where delivery runs. The recommendation is the home only, so no crew host holds publishing credentials. The cost is that the candidate commit must reach the home.
- D4. Whether verification reruns at the home, or a host-reported result is accepted with a label. A rerun needs the repository and the toolchain at the home.
- D5. Whether tasks on a crew host count toward the wake-turn limits and the workload interval (N9, N11). The safer answer is yes.

## Open questions and what is not verified

### Open questions

- How does a candidate commit travel from a crew host to the home? Through the channel, as a bundle the proxy streams, or through a git remote that both can reach. The channel needs no path. A shared git remote needs credentials on the crew host.
- How is a project's clone on a crew host created and kept current? By a request from the home, or by the user? Who owns the clone's remote URL?
- Do agent sign-ins on a crew host use the same provider account as the home? How is spend attributed?
- A secret that lives only on the crew host and is echoed by its agent will reach the home store, and the home cannot know it. R11 covers only secrets the home knows. Is the rest acceptable?
- The daemon exits after 60 seconds idle and is started by the desktop host. A crew host has no desktop host. Does the forced-command proxy start the crew daemon?
- Should replay protection use counters or time windows (R13)? Counters need persistent state on both sides.
- What is the right revocation bound (R4 suggests 5 seconds)?
- How does the daemon's method table mark user-only methods so that a new method is denied by default? This depends on piece 0b.

### Not verified

- No remote host was set up. No SSH, mTLS or relay connection was made. Nothing here was run, built or tested. Code facts come from reading files at `73586b6`.
- `plan-0b.md` did not exist when this was written. The approver credential is described only as "a desktop-only approver secret, being added in Phase 0".
- OpenSSH client and server availability and behavior on the user's Windows machines. Not checked.
- That SSH forced commands and the `restrict` key option behave as assumed on both operating systems. This rests on general knowledge of OpenSSH, not on a test here.
- That the daemon has no connection cap other than the Windows pipe limit. Only `server.rs` and `session.rs` were read for this.
- That no code expires idempotency records. A search of `orchestrator-store/src` for deletes found none.
- That the Unix transport has no peer credential check. A search of `unix_transport.rs` found none.
- How any agent CLI behaves on a crew host.
