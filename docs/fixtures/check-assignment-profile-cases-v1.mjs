// Contract check for docs/assignment-profile-policy-v1.md and its decision fixtures.
// Ajv (pinned ajv-cli) gives each record's schema verdict. The rule oracle encodes policy
// sections 1-5 and names every rule a record breaks. Contract test only: nothing here
// classifies, launches, proves, or grants anything at runtime.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const docsDir = join(dirname(fileURLToPath(import.meta.url)), '..');
const schemaFile = join(docsDir, 'assignment-profile-decision-v1.schema.json');
const policyFile = join(docsDir, 'assignment-profile-policy-v1.md');
const fixtureFile = join(docsDir, 'fixtures', 'assignment-profile-cases-v1.json');
const POLICY_SOURCE = 'docs/assignment-profile-policy-v1.md';
const POLICY_VERSION = 'assignment-profile-policy-v1';
const AJV = ['exec', '--yes', '--package', 'ajv-cli@5.0.0', '--', 'ajv'];

const RANK = { 'docs-only': 0, small: 1, standard: 2, 'high-risk': 3 };
const TRUSTED_BY = ['immutable-task-contract', 'trusted-admission-record'];
const UNRESOLVED_CODES = ['UNKNOWN_SURFACE', 'INCOMPLETE_DIFF_EVIDENCE', 'UNTRUSTED_POLICY_PROVENANCE', 'MISSING_CHANGE_BASE'];
const HIGH_RISK_CODES = [
  'AUTHORIZATION_CHANGE',
  'SECURITY_SENSITIVE_CHANGE',
  'DATA_PRIVACY_CHANGE',
  'IRREVERSIBLE_SIDE_EFFECT',
  'SAFETY_CRITICAL_CHANGE',
  'SUPPLY_CHAIN_TRUST_CHANGE',
  'POLICY_SELF_CHANGE',
  'RISK_OR_UNCERTAINTY_ESCALATION',
];
// Sections 3 and 5: the floor each reason code sets.
const CODE_FLOOR = {
  ORDINARY_PROSE_ONLY: 'docs-only',
  BOUNDED_LOW_RISK: 'small',
  NONTRIVIAL_BEHAVIOR: 'standard',
  CROSS_SURFACE_COUPLING: 'standard',
  BEHAVIORAL_MARKDOWN_SURFACE: 'standard',
  CONFIGURATION_SURFACE: 'standard',
  EXECUTABLE_SURFACE: 'standard',
  GENERATED_CODE_SURFACE: 'standard',
  ...Object.fromEntries(HIGH_RISK_CODES.map((code) => [code, 'high-risk'])),
};
const RISK_FLOOR = { low: null, moderate: 'standard', high: 'high-risk', critical: 'high-risk' };
const UNCERTAINTY_FLOOR = { low: null, moderate: 'standard', high: 'high-risk' };
// Section 2: surface kinds that must carry their reason code.
const SURFACE_CODE = {
  behavioral_markdown: 'BEHAVIORAL_MARKDOWN_SURFACE',
  configuration: 'CONFIGURATION_SURFACE',
  executable: 'EXECUTABLE_SURFACE',
  executable_example: 'EXECUTABLE_SURFACE',
  generated_code: 'GENERATED_CODE_SURFACE',
  authorization_policy: 'AUTHORIZATION_CHANGE',
  security_sensitive: 'SECURITY_SENSITIVE_CHANGE',
  policy_definition: 'POLICY_SELF_CHANGE',
};
// Section 4: evidence every provisional and final record carries.
const COMMON_EVIDENCE = [
  'task-contract',
  'policy-source',
  'change-base',
  'intent-surface-inventory',
  'risk-assessment',
  'functional-test-assessment',
  'security-assessment',
];
const SELF_CHANGE_EVIDENCE = ['trusted-prechange-policy', 'policy-integrity'];
// Section 5: evidence a final record needs per reason code.
const FINAL_CODE_EVIDENCE = {
  ORDINARY_PROSE_ONLY: ['docs-content-review'],
  BEHAVIORAL_MARKDOWN_SURFACE: ['behavioral-content-review'],
  CONFIGURATION_SURFACE: ['configuration-validation'],
  EXECUTABLE_SURFACE: ['executable-validation'],
  GENERATED_CODE_SURFACE: ['generation-provenance'],
  AUTHORIZATION_CHANGE: ['security-check-results'],
  SECURITY_SENSITIVE_CHANGE: ['security-check-results'],
  DATA_PRIVACY_CHANGE: ['security-check-results'],
};
// Section 5: evidence an unresolved record cites per reason code.
const UNRESOLVED_EVIDENCE = {
  UNKNOWN_SURFACE: ['unknown-surface'],
  INCOMPLETE_DIFF_EVIDENCE: ['incomplete-diff'],
  UNTRUSTED_POLICY_PROVENANCE: ['policy-integrity'],
};
// Each rejected fixture must fail the schema and trip the rule named here.
const REJECTIONS = {
  'invalid-docs-only-behavior-config-executable-generated': 'DOCS_ONLY_ELIGIBILITY',
  'invalid-docs-only-requires-functional-tests': 'DOCS_ONLY_ELIGIBILITY',
  'invalid-docs-only-unknown-path': 'UNKNOWN_BLOCKS_COMPLETION',
  'invalid-final-incomplete-diff': 'FINAL_COMPLETE',
  'invalid-automatic-downgrade-after-escalation': 'NO_AUTO_DOWNGRADE',
  'invalid-small-one-line-authorization': 'PROFILE_FROM_REASONS',
  'invalid-docs-only-drive-absolute-path': 'PATH_FORM',
  'invalid-docs-only-unc-path': 'PATH_FORM',
  'invalid-docs-only-backslash-traversal-path': 'PATH_FORM',
};

const kindsOf = (r) => [...r.expected_surfaces, ...r.actual_change_coverage.changed_surfaces].map((s) => s.kind);
const highest = (profiles) => profiles.reduce((a, b) => (RANK[b] > RANK[a] ? b : a));
const isNonPortable = (p) => p.startsWith('/') || /^[A-Za-z]:/.test(p) || p.includes('\\') || p.split('/').includes('..');

// Representative outcomes the acceptance criteria name; each needs an accepted record.
const SCENARIOS = [
  [
    'ten prose files: final docs-only with zero functional tests',
    (r) =>
      r.expected_surfaces.length >= 10 &&
      kindsOf(r).every((k) => k === 'ordinary_prose') &&
      r.decision.status === 'final' &&
      r.decision.profile === 'docs-only' &&
      !r.test_assessment.functional_tests_required,
  ],
  [
    'single authorization line: high-risk',
    (r) =>
      r.decision.reason_codes.includes('AUTHORIZATION_CHANGE') &&
      r.decision.profile === 'high-risk' &&
      r.actual_change_coverage.changed_surfaces.length === 1,
  ],
  [
    'mixed docs and code: never docs-only',
    (r) =>
      kindsOf(r).includes('ordinary_prose') &&
      kindsOf(r).includes('source_code') &&
      r.decision.profile !== null &&
      r.decision.profile !== 'docs-only',
  ],
  [
    'unknown surface: unresolved, null profile',
    (r) =>
      r.decision.reason_codes.includes('UNKNOWN_SURFACE') &&
      r.decision.status === 'unresolved' &&
      r.decision.profile === null,
  ],
  [
    'incomplete diff: unresolved, null profile',
    (r) =>
      r.decision.reason_codes.includes('INCOMPLETE_DIFF_EVIDENCE') &&
      r.decision.status === 'unresolved' &&
      r.decision.profile === null,
  ],
  [
    'provisional: non-null profile, no actual diff',
    (r) =>
      r.decision.status === 'provisional' &&
      r.decision.profile !== null &&
      r.actual_change_coverage.status === 'not-yet-available',
  ],
];

// Sorted ids of every policy rule the record breaks; empty means the record is consistent.
function ruleViolations(r, policyDigest) {
  const v = new Set();
  const { status, profile, reason_codes: codes } = r.decision;
  const has = (code) => codes.includes(code);
  const coverage = r.actual_change_coverage;
  const evidence = new Set(r.evidence.map((e) => e.kind));
  const unclassified = [...r.unclassified_expected_paths, ...coverage.unclassified_paths];
  const unresolvedCodes = codes.filter((c) => UNRESOLVED_CODES.includes(c));
  const pin = r.policy_provenance;
  const base = r.change_base;
  const tests = r.test_assessment.functional_tests_required;
  const docsOnly = profile === 'docs-only';

  // Section 2: portable repo-relative paths; each path occurs once per inventory list.
  const pathLists = [
    r.expected_surfaces.map((s) => s.path),
    coverage.changed_surfaces.map((s) => s.path),
    r.unclassified_expected_paths,
    coverage.unclassified_paths,
  ];
  for (const paths of pathLists) {
    if (new Set(paths).size !== paths.length) v.add('PATH_UNIQUE');
    if (paths.some(isNonPortable)) v.add('PATH_FORM');
  }
  // Sections 2 and 4: unknown or unclassified paths keep the decision unresolved.
  if (unclassified.length > 0 && status !== 'unresolved') v.add('UNKNOWN_BLOCKS_COMPLETION');
  // Section 4: unresolved has no profile and cites why; other stages have a profile and no unresolved reason.
  if (status === 'unresolved' ? profile !== null || unresolvedCodes.length === 0 : profile === null || unresolvedCodes.length > 0) {
    v.add('UNRESOLVED_SHAPE');
  }
  // Section 4: final needs the complete diff with every path classified.
  if (status === 'final' && (coverage.status !== 'complete' || unclassified.length > 0 || unresolvedCodes.length > 0)) {
    v.add('FINAL_COMPLETE');
  }
  // Section 4: provisional has no actual diff and no results.
  if (status === 'provisional') {
    const results = ['diff-coverage', 'functional-test-results', 'security-check-results'].some((k) => evidence.has(k));
    if (!has('PROVISIONAL_ASSIGNMENT') || coverage.status !== 'not-yet-available' || coverage.changed_surfaces.length > 0 || results) {
      v.add('PROVISIONAL_SHAPE');
    }
  } else if (has('PROVISIONAL_ASSIGNMENT')) {
    v.add('PROVISIONAL_SHAPE');
  }
  // Sections 3 and 5: the profile is the highest floor (standard when none); floors never lower; tests follow profile.
  if (profile !== null) {
    const floors = [
      ...codes.map((c) => CODE_FLOOR[c]),
      RISK_FLOOR[r.risk_assessment.level],
      UNCERTAINTY_FLOOR[r.risk_assessment.uncertainty],
      r.profile_floor.minimum_profile,
    ].filter(Boolean);
    if (profile !== (floors.length > 0 ? highest(floors) : 'standard')) v.add('PROFILE_FROM_REASONS');
    if (r.profile_floor.minimum_profile && RANK[profile] < RANK[r.profile_floor.minimum_profile]) v.add('NO_AUTO_DOWNGRADE');
    if (tests !== !docsOnly) v.add('TESTS_BY_PROFILE');
  }
  // Section 3: docs-only is ordinary prose only, low risk and uncertainty, zero tests, complete diff, no other reason.
  if (status !== 'unresolved' && (docsOnly || has('ORDINARY_PROSE_ONLY'))) {
    const onlyDocsCodes = codes.every((c) => c === 'ORDINARY_PROSE_ONLY' || c === 'PROVISIONAL_ASSIGNMENT');
    const eligible =
      docsOnly &&
      kindsOf(r).every((k) => k === 'ordinary_prose') &&
      unclassified.length === 0 &&
      !tests &&
      r.risk_assessment.level === 'low' &&
      r.risk_assessment.uncertainty === 'low' &&
      !r.policy_self_change &&
      onlyDocsCodes;
    if (!eligible) v.add('DOCS_ONLY_ELIGIBILITY');
  }
  // Section 2: a behavior-bearing surface kind carries its reason code.
  for (const kind of new Set(kindsOf(r))) {
    if (SURFACE_CODE[kind] && !has(SURFACE_CODE[kind])) v.add('SURFACE_CODE');
  }
  // Sections 1 and 5: the self-change flag and its code agree; a self-change is high-risk unless unresolved.
  if (r.policy_self_change !== has('POLICY_SELF_CHANGE')) v.add('SELF_CHANGE');
  if (r.policy_self_change && status !== 'unresolved' && profile !== 'high-risk') v.add('SELF_CHANGE');
  // Section 1: provisional and final need trusted provenance and a selected base; unresolved needs its provenance reason.
  const trusted = TRUSTED_BY.includes(pin.trusted_by) && pin.sha256 !== null;
  const baseSelected =
    base.commit !== null && base.selected_from === 'trusted-admission-record' && base.selected_before_work === true;
  if (status === 'unresolved' ? !trusted && !has('UNTRUSTED_POLICY_PROVENANCE') : !trusted || !baseSelected) {
    v.add('PROVENANCE_AND_BASE');
  }
  if (trusted && has('UNTRUSTED_POLICY_PROVENANCE')) v.add('PROVENANCE_AND_BASE');
  if ((base.commit === null) !== has('MISSING_CHANGE_BASE')) v.add('PROVENANCE_AND_BASE');
  // Section 1: a pin names this policy file; a self-change pins the pre-change snapshot, never this file.
  if (pin.sha256 !== null) {
    if (pin.source !== POLICY_SOURCE || pin.version !== POLICY_VERSION) v.add('POLICY_PIN');
    if (r.policy_self_change ? pin.sha256 === policyDigest : pin.sha256 !== policyDigest) v.add('POLICY_PIN');
  }
  // Sections 4 and 5: evidence for the stage and for each reason code.
  const needed = [];
  if (status !== 'unresolved') needed.push(...COMMON_EVIDENCE);
  if (status !== 'unresolved' && has('POLICY_SELF_CHANGE')) needed.push(...SELF_CHANGE_EVIDENCE);
  if (status === 'final') {
    needed.push('diff-coverage');
    if (!docsOnly) needed.push('functional-test-results');
    if (profile === 'high-risk') needed.push('security-check-results');
    for (const c of codes) needed.push(...(FINAL_CODE_EVIDENCE[c] ?? []));
  }
  if (status === 'unresolved') {
    for (const c of unresolvedCodes) needed.push(...(UNRESOLVED_EVIDENCE[c] ?? []));
  }
  if (needed.some((k) => !evidence.has(k))) v.add('EVIDENCE');
  return [...v].sort();
}

// Ajv verdict through the pinned ajv-cli. Exit 1 with " invalid" is a rejection; anything else throws.
function schemaVerdict(record, file) {
  writeFileSync(file, JSON.stringify(record));
  try {
    execFileSync('npm', [...AJV, 'validate', '--spec=draft2020', '--errors=json', '-s', schemaFile, '-d', file], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    return { valid: true, message: '' };
  } catch (error) {
    const text = `${error.stdout ?? ''}${error.stderr ?? ''}`;
    if (error.status !== 1 || !text.includes(' invalid')) {
      throw new Error(`ajv failed on ${file}: ${text.trim() || error.message}`);
    }
    const errors = JSON.parse(text.slice(text.indexOf('[')));
    return { valid: false, message: errors.map((e) => `${e.instancePath || '/'} ${e.keyword}`).join(', ') };
  }
}

const fixture = JSON.parse(readFileSync(fixtureFile, 'utf8'));
const policyDigest = createHash('sha256').update(readFileSync(policyFile)).digest('hex');
const workDir = mkdtempSync(join(tmpdir(), 'assignment-profile-check-'));
const failures = [];
const clean = [];
let schemaAgreed = 0;
try {
  fixture.cases.forEach((c, index) => {
    const schema = schemaVerdict(c.decision, join(workDir, `case-${index}.json`));
    let rules;
    try {
      rules = ruleViolations(c.decision, policyDigest);
    } catch (error) {
      rules = [`ORACLE_ERROR:${error.message}`];
    }
    const problems = [];
    if (schema.valid === c.expected_schema_valid) schemaAgreed += 1;
    else problems.push(`schema ${schema.valid ? 'accepted' : `rejected (${schema.message})`}`);
    if (c.expected_schema_valid) {
      if (rules.length > 0) problems.push(`rules ${rules.join(',')}`);
      else clean.push(c.decision);
    } else if (!REJECTIONS[c.id]) {
      problems.push('no expected rule recorded for this reject');
    } else if (!rules.includes(REJECTIONS[c.id])) {
      problems.push(`expected ${REJECTIONS[c.id]}, found ${rules.join(',') || 'none'}`);
    }
    const d = c.decision.decision;
    const outcome = `${d.status}/${d.profile ?? 'null'} tests=${c.decision.test_assessment.functional_tests_required} [${d.reason_codes.join('+')}]`;
    if (problems.length > 0) failures.push(c.id);
    const suffix = problems.length > 0 ? ` :: ${problems.join('; ')}` : '';
    console.log(`${problems.length > 0 ? 'FAIL' : 'PASS'} ${String(index).padStart(2, '0')} ${c.id} ${c.expected_schema_valid ? 'accept' : 'reject'} ${outcome}${suffix}`);
  });
  const report = (name, ok) => {
    if (!ok) failures.push(name);
    console.log(`${ok ? 'PASS' : 'FAIL'} coverage ${name}`);
  };
  for (const [name, predicate] of SCENARIOS) report(name, clean.some(predicate));
  for (const profile of ['docs-only', 'small', 'standard', 'high-risk', null]) {
    report(`accepted profile ${profile ?? 'null (unresolved)'}`, clean.some((r) => r.decision.profile === profile));
  }
  console.log(`schema verdicts agreeing with fixture: ${schemaAgreed}/${fixture.cases.length}`);
  console.log(`accepted and rule-clean: ${clean.length}/${fixture.cases.filter((c) => c.expected_schema_valid).length}`);
  console.log(`policy sha256: ${policyDigest}`);
  console.log(failures.length === 0 ? 'RESULT: PASS' : `RESULT: FAIL (${failures.length} problems)`);
  process.exitCode = failures.length === 0 ? 0 : 1;
} finally {
  rmSync(workDir, { recursive: true, force: true });
}
