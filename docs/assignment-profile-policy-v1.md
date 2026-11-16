# Assignment profile policy v1

Status: policy definition only. This document defines assignment classification and evidence records. It does not add a runtime classifier, change the task contract/API/persistence, run checks, grant permissions, or select delivery, model, harness, or merge authority.

## 1. Authority and immutable inputs

A decision applies to one immutable task contract and one repository change base. Record the task reference and contract digest, exact trusted policy version/source/digest, and exact base commit selected before work. The base comes from the immutable admission record associated with the task contract; never infer or replace it from a worker's later branch head. Provisional and final decisions require complete trusted policy provenance and base selection. For unresolved decisions, record unavailable policy digest/trust-anchor and base commit/selection values as `null`; never invent them.

The current Rust `TaskContract` remains unchanged. This policy is an external definition for future orchestration; its JSON decision record is not a task contract field and does not alter `VerificationPolicy::None | Commands`. The record's `task_ref` is the engine-assigned `TaskContract.id` (`crates/orchestrator-core/src/contract.rs`); its `sha256` is the contract digest pinned by the trusted admission record. Future orchestration maps a final non-docs-only decision to `VerificationPolicy::Commands` with at least one functional test command, and a `docs-only` decision to `None` only with its recorded zero-test assessment. No code enforces this mapping in v1.

Trust only the policy version and source digest pinned by the immutable task contract or its trusted admission record, outside the candidate changes. A candidate cannot establish its own policy provenance. If that pin or its integrity evidence is absent or conflicts with the recorded source, return `unresolved`.

A task that changes this policy, its trusted pin, or its classification rules is evaluated against the trusted pre-change policy snapshot. Mark `policy_self_change`, retain that snapshot's provenance (never this candidate's own digest), and classify at least `high-risk`; the changed policy cannot lower its own profile. Provisional/final decisions require trusted pre-change policy pin and integrity evidence; without it, return `unresolved`.

## 2. Surface inventory and risk

Before work, record intent and enumerate expected repository-relative paths with semantic surface kinds. After work, enumerate every changed path from the diff at the immutable change base and classify it. Each path occurs once; if a path contains mixed prose and behavior, classify it by its most consequential content. Unknown paths or unknown content are recorded separately, never guessed into `ordinary_prose` or `other`. Paths use portable repository-relative forward-slash form: no leading slash, drive prefix, or backslash; `..` path segments are forbidden.

Kinds used by the decision schema:

- `ordinary_prose`: explanatory or narrative text only; no normative workflow, agent instruction, policy, configuration, executable example, or generated output.
- `behavioral_markdown`: markdown that changes instructions, prompts, policies, contracts, commands, or other behavior.
- `configuration`: data that changes runtime, build, security, or tool behavior.
- `executable`, `executable_example`: program code or an example intended to run.
- `generated_code`: generated source or other generated output.
- `source_code`, `authorization_policy`, `security_sensitive`, `data_migration`: code, privilege/security boundary, sensitive-data, or migration surfaces; `authorization_policy` and `security_sensitive` are always high-risk.
- `policy_definition`: this assignment policy or its trust/configuration; always marks `policy_self_change` and is high-risk with trusted pre-change evidence, otherwise unresolved.
- `other`: a known surface not covered above, with its purpose in the rationale.

Assess impact and uncertainty from semantics, coupling, exposure, reversibility, and evidence quality. File count, line count, diff size, and number of prose files do not set a profile. A larger ordinary-prose diff may remain docs-only; one authorization-line change is high-risk. When facts are known but uncertainty is high, escalate to high-risk. When a path, behavior, base, policy pin, or material diff evidence is unknown/incomplete, return unresolved instead of estimating a lower profile.

## 3. Profiles

Profiles classify work and evidence depth only. They do not authorize actions, waive checks, alter autonomy, accept runtime launches or proof, or select a model, harness, delivery mode, reviewer, or merge authority; the same profile applies whichever model or harness produced the work. Choose the least profile that satisfies every applicable rule; any higher-risk surface or unresolved uncertainty wins over a lower-risk description or small diff.

Profile ids and legacy names: `docs-only`, `small`, `standard`, and `high-risk`. The PR100 legacy draft named them `documentation-only`, `small-low-risk`, `standard`, and `high-risk`; `documentation-only` is `docs-only` and `small-low-risk` is `small`, under the eligibility rules here. This document is not a port of the legacy text. Known work that fits no narrower profile is `standard`, as in the legacy draft; unknown work is `unresolved`, never defaulted. File or line counts never select a profile.

| Profile | Eligibility and expected surfaces | Minimum decision evidence |
| --- | --- | --- |
| `docs-only` | Every expected and actual surface is `ordinary_prose`; risk and uncertainty are low; no required functional tests; complete known diff. Behavioral markdown, configuration, executable content/examples, generated output, code, security/authorization, migrations, unknown surfaces, or policy changes are ineligible. Ten prose files do not change this rule. | Trusted policy/task/base provenance; intent and complete expected/actual inventories; risk rationale; diff-coverage evidence; explicit functional-test assessment saying zero required functional tests; docs-content review; security-check assessment. Any independently mandatory security or project check still runs when applicable. |
| `small` | One local, bounded, low-risk change whose effects and touched surfaces are fully understood. May include a non-prose surface; never use file/line count as the bound. | Every final small assignment requires at least one relevant functional test and result. Provisional records state planned tests without claiming results. Include all common evidence, complete diff, surface-specific validation (configuration, executable examples, or generated provenance when applicable), and security-check assessment. |
| `standard` | Normal implementation work, meaningful behavior changes, behavioral markdown, cross-surface changes, or moderate risk/uncertainty whose surfaces and effects are understood. | Every final standard assignment requires at least one relevant functional test and result, plus appropriate regression/integration evidence, affected-behavior and compatibility review, security-check assessment, and every mandatory applicable security check. Provisional records state planned tests without claiming results. |
| `high-risk` | High/critical consequence or high uncertainty; includes `authorization_policy`, `security_sensitive`, sensitive data/privacy, irreversible/external side effects, safety-critical behavior, supply-chain/execution trust, and `policy_definition` or policy/trust-anchor changes. Risk outranks diff size and manually supplied low risk/factors. | Every final high-risk assignment requires at least one relevant functional test and result, explicit security assessment, required security checks and results, and evidence addressing impact, containment/reversibility, and affected boundaries. Provisional records state planned tests and checks without claiming results. A missing required result keeps a final decision unresolved, not lower-profile. |

`common evidence` means: immutable task-contract identity/digest; trusted policy source/version/digest and trust anchor; immutable change-base commit/selection reference; intent and expected-surface inventory; risk/uncertainty rationale; actual diff coverage for a final decision; functional-test and security-check assessments; and reason codes. Provisional/final decisions require complete trusted policy and base values. In unresolved decisions, unavailable digest, trust-anchor, commit, or selection-reference values are `null`, not guessed. Evidence references identify inspected artifacts; bare assertions or invented references do not satisfy an obligation.

Only ordinary-prose `docs-only` may state zero required functional tests. Every final non-docs-only assignment MUST require at least one relevant functional test and cite its result, including source code, executable examples, runtime configuration, generated output, and behavioral markdown. A reasoned no-test assessment cannot substitute. Every provisional decision MUST have a non-null profile, state required future tests, and MUST NOT include `functional-test-results` or claim results. `docs-only` still requires a functional-test assessment stating why zero functional tests apply, plus ordinary-prose correctness/review, complete diff, policy provenance, and any independently mandatory security checks. Existing security requirements are never weakened by profile; `not-applicable` needs rationale and cannot override a check required elsewhere.

## 4. Decision lifecycle and escalation

`provisional` assigns a conservative non-null profile from intent, expected surfaces, and known risk before implementation. It is not completion evidence. It requires trusted provenance, immutable base selection, a complete expected inventory, risk assessment, test assessment, and planned security assessment. The actual-change coverage state is `not-yet-available`; no changed paths are claimed, and no functional-test results are included.

`final` is allowed only after the full diff against the immutable base is available, every changed path and expected path is classified, all evidence obligations for the selected profile are met, and there are no unknown/unclassified paths. A final `docs-only` decision cannot be recorded while any surface or diff evidence is unknown or incomplete. A missing required check/result keeps the decision unresolved.

`unresolved` has no profile (`profile: null`) and blocks completion/classification. Use it for unknown expected/actual surfaces, incomplete diff or required evidence, absent/untrusted policy provenance, or absent immutable base. Preserve known facts, record unavailable provenance/base fields as `null`, and cite only existing evidence. Never invent a digest, commit, or reference, or convert unknown to docs-only or another lower profile.

Re-evaluate when the diff reveals a new surface or a material risk. An increase in profile raises the assignment's `profile_floor`; the floor is monotonic for the decision chain. Later evidence cannot automatically lower it. A lower profile requires a new immutable task/admission record explicitly replacing the prior scope and decision; no model or worker self-declaration can do so. This rule adds no approval or permission mechanism. A change after a final decision invalidates that final record and requires a new record against the same immutable base.

## 5. Reason codes and evidence obligations

Use one or more schema-defined reason codes, with evidence references sufficient to substantiate each applicable code:

| Code | Meaning | Required supporting evidence |
| --- | --- | --- |
| `ORDINARY_PROSE_ONLY` | Docs-only eligibility: all expected/actual surfaces ordinary prose, low risk/uncertainty, zero required functional tests. | Surface inventory, full diff, risk/test assessment, docs-content review. |
| `BOUNDED_LOW_RISK` | Small profile: local, bounded, understood, low-risk effect. | Intent/surface/risk evidence and at least one relevant functional-test result for a final decision; provisional records state planned test without results. |
| `NONTRIVIAL_BEHAVIOR` | Behavior warrants standard profile. | Behavior/surface review, at least one relevant functional-test result for a final decision, and appropriate regression/integration evidence. |
| `CROSS_SURFACE_COUPLING` | Multiple related surfaces or dependencies increase integration risk. | Complete surface inventory and integration/regression evidence. |
| `BEHAVIORAL_MARKDOWN_SURFACE` | Instructions/policy/markdown changes behavior; never docs-only. | Behavioral-content review and applicable behavioral/functional validation. |
| `CONFIGURATION_SURFACE` | Configuration changes behavior; never docs-only. | Configuration validation and applicable functional/security checks. |
| `EXECUTABLE_SURFACE` | Executable source or runnable example; never docs-only. | Execution/example validation and applicable functional/security checks. |
| `GENERATED_CODE_SURFACE` | Generated output; never docs-only. | Generator/source provenance, output validation, and applicable tests. |
| `AUTHORIZATION_CHANGE` | Authorization or privilege boundary changes; always high-risk. | Provisional: planned security assessment/checks only. Final: security-boundary review and required security-check results. |
| `SECURITY_SENSITIVE_CHANGE` | Authentication/security boundary changes; always high-risk. | Provisional: planned security assessment/checks only. Final: boundary review and required security-check results. |
| `DATA_PRIVACY_CHANGE` | Sensitive-data or privacy impact; high-risk. | Provisional: planned security assessment/checks only. Final: data-flow/privacy review and required security-check results. |
| `IRREVERSIBLE_SIDE_EFFECT` | Irreversible or externally consequential effect. | Impact/containment/reversibility review and applicable checks; high-risk. |
| `SAFETY_CRITICAL_CHANGE` | Safety-critical behavior. | Safety-impact review and required tests/checks; high-risk. |
| `SUPPLY_CHAIN_TRUST_CHANGE` | Executable trust, dependency, build, or supply-chain boundary. | Provenance/security evidence and required checks; high-risk. |
| `POLICY_SELF_CHANGE` | This policy or its trust anchor changes. | Trusted pre-change policy pin and integrity evidence; high-risk or unresolved. |
| `RISK_OR_UNCERTAINTY_ESCALATION` | Risk or high uncertainty outweighs apparent scope/size. | Risk/uncertainty rationale and corresponding impact evidence; high-risk. |
| `UNKNOWN_SURFACE` | Path, surface kind, or material behavior is unknown. | Exact unknown-path/evidence reference; unresolved. |
| `INCOMPLETE_DIFF_EVIDENCE` | Full diff or a required evidence item is unavailable. | Exact missing artifact/check reference; unresolved. |
| `UNTRUSTED_POLICY_PROVENANCE` | Trusted policy version/source/digest cannot be established. | Available candidate source/pin and integrity gap; unresolved. |
| `MISSING_CHANGE_BASE` | Immutable base commit or selection reference is unavailable. | Inspected task/admission record showing missing base; unresolved. |
| `PROVISIONAL_ASSIGNMENT` | Pre-implementation decision only. | Expected inventory, intent, risk/test/security assessments, trusted provenance and base. |

A reason code is explanatory, never a permission. If multiple apply, retain all; do not replace a high-risk reason with a lower-profile code. The JSON Schema enforces structural and cross-field contradictions it can express; human evidence review still verifies that path kinds and references truthfully describe the diff. No runtime or LLM classification is defined here.

Profile floors: `ORDINARY_PROSE_ONLY` sets `docs-only`; `BOUNDED_LOW_RISK` sets `small`; `NONTRIVIAL_BEHAVIOR`, `CROSS_SURFACE_COUPLING`, `BEHAVIORAL_MARKDOWN_SURFACE`, `CONFIGURATION_SURFACE`, `EXECUTABLE_SURFACE`, and `GENERATED_CODE_SURFACE` set `standard`; `AUTHORIZATION_CHANGE`, `SECURITY_SENSITIVE_CHANGE`, `DATA_PRIVACY_CHANGE`, `IRREVERSIBLE_SIDE_EFFECT`, `SAFETY_CRITICAL_CHANGE`, `SUPPLY_CHAIN_TRUST_CHANGE`, `POLICY_SELF_CHANGE`, and `RISK_OR_UNCERTAINTY_ESCALATION` set `high-risk`. Risk `moderate` or uncertainty `moderate` sets `standard`; risk `high` or `critical`, or uncertainty `high`, sets `high-risk`. A surface kind requires its code: `behavioral_markdown` requires `BEHAVIORAL_MARKDOWN_SURFACE`; `configuration` requires `CONFIGURATION_SURFACE`; `executable` and `executable_example` require `EXECUTABLE_SURFACE`; `generated_code` requires `GENERATED_CODE_SURFACE`; `authorization_policy` requires `AUTHORIZATION_CHANGE`; `security_sensitive` requires `SECURITY_SENSITIVE_CHANGE`; `policy_definition` requires `POLICY_SELF_CHANGE`. A non-unresolved decision's profile is the strictest floor among its codes, risk, uncertainty, and `profile_floor` (order: `docs-only` < `small` < `standard` < `high-risk`), or `standard` when none applies. `UNKNOWN_SURFACE`, `INCOMPLETE_DIFF_EVIDENCE`, `UNTRUSTED_POLICY_PROVENANCE`, and `MISSING_CHANGE_BASE` force `unresolved` with `profile: null`. `PROVISIONAL_ASSIGNMENT` marks the provisional stage.

## 6. Decision record and schema

`docs/assignment-profile-decision-v1.schema.json` defines the closed, bounded JSON record. Each record includes task-contract provenance, trusted policy provenance, immutable base selection, intent, expected and actual surface inventories, risk/uncertainty, policy-self-change flag, decision stage/profile/reasons, monotonic profile floor, test and security assessments, and evidence references. Schema bounds protect record size only; they are not profile thresholds. A record that exceeds a representational bound is not truncated or split to obtain a lower profile; it cannot be accepted as a complete v1 decision.

`docs/fixtures/assignment-profile-cases-v1.json` supplies full decision objects with expected schema accept/reject outcomes. `docs/fixtures/check-assignment-profile-cases-v1.mjs` checks each record with Ajv (pinned `ajv-cli@5.0.0`) and a rule oracle for sections 1 to 5; it is a contract test, not a runtime classifier. Each accepted record must be rule-clean, and the accepted set must cover every profile and the representative outcomes; each rejected record must fail the schema and trigger the rule named for it in the checker. Non-self-change records pin this file's SHA-256, so any policy edit requires re-pinning; a self-change record pins a different pre-change snapshot. These are validation examples, not production provenance or runtime behavior.
