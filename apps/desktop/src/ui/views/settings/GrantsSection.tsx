/** DPES-1105 Phase 5: the "Standing approvals" section of the Settings page. Hidden quietly for a daemon without the capability. */
import { useId, useRef, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError } from "../../../daemon/errors";
import { basename } from "../../format";
import { notOk, observation } from "../../../sources/surfaces/result";
import type { GrantItem, GrantPolicy, GrantTerms, GrantsSnapshot, Workstream } from "../../../sources/surfaces/types";
import { CLEAR_MS } from "../../action";
import { useGate, useServices, useStore, useSurface } from "../../services";
import { Badge, Button, Disclosure, Pill } from "../../theme";
import { Dialog } from "../../Dialog";
import { Notice } from "../../widgets";
import { SaveButton, useSaveGuard } from "./SaveBar";
import {
  FIXTURE_TEXT,
  INTRO_TEXT,
  INVALID_REASON,
  MERGE_BADGE,
  MERGE_BUDGET_NOTE,
  MERGE_CONFIRM_TEXT,
  MERGE_INTRO_TEXT,
  MERGE_INVALID_REASON,
  MERGE_TITLE,
  NEVER_GRANTED_TEXT,
  NOT_CONNECTED_REASON,
  REVOKE_LOCKED_NOTE,
  SECTION_TITLE,
  TOKEN_NOTE,
  UPDATE_NOTE,
  createFailureText,
  createdText,
  branchPrefixText,
  hasErrors,
  hasMergeErrors,
  initialDraft,
  initialMergeDraft,
  isActive,
  isMergeGrant,
  kindLabel,
  maxHours,
  mergeCreateFailureText,
  mergeLimits,
  mergeOffered,
  mergeTermsOf,
  offeredKinds,
  revokeFailureText,
  revokedText,
  scopeText,
  shortGrantId,
  statusChip,
  termsOf,
  timeLeftText,
  tokensText,
  usesLeftText,
  validateDraft,
  validateMerge,
  type GrantDraft,
  type MergeDraft,
} from "./grantsModel";
import { projectCode } from "./projectCode";

/** A grant ends with no push from the daemon, so the list is read again on this interval. */
export const GRANTS_POLL_MS = 15_000;

type Field = "scope" | "kinds" | "hours" | "uses" | "budget";
type MergeField = "scope" | "prefix" | "hours" | "uses" | "budget" | "confirmed";

const describedBy = (...ids: Array<string | false | null | undefined>) => ids.filter(Boolean).join(" ") || undefined;

function GrantsBody({ grants, policy, fixture, reload, headingId }: { grants: GrantItem[]; policy: GrantPolicy; fixture: boolean; reload: () => void; headingId: string }) {
  const { surfaces, data } = useServices();
  const gate = useGate();
  const source = surfaces.grants;
  const { projects } = useStore(data);
  const [typed, setTyped] = useState<GrantDraft | null>(null);
  const [touched, setTouched] = useState<ReadonlySet<Field>>(new Set());
  const [busy, setBusy] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [announcement, setAnnouncement] = useState<{ kind: "ok" | "bad"; text: string } | null>(null);
  const [mergeTyped, setMergeTyped] = useState<MergeDraft | null>(null);
  const [mergeTouched, setMergeTouched] = useState<ReadonlySet<MergeField>>(new Set());
  const mergeGuard = useSaveGuard();
  const rows = useRef(new Map<string, HTMLLIElement>());
  const working = useRef(false);
  const ids = { reason: useId(), scope: useId(), kinds: useId(), hours: useId(), uses: useId(), budget: useId(), select: useId() };
  const mergeIds = { reason: useId(), select: useId(), prefix: useId(), hours: useId(), uses: useId(), budget: useId(), confirm: useId() };

  const draft = typed ?? initialDraft(policy);
  const errors = validateDraft(draft, policy);
  const live = !fixture;
  const locked = live && !(gate.connected && gate.connection.approver);
  const reason =
    live && !gate.connected ? NOT_CONNECTED_REASON : locked ? APPROVER_REQUIRED_NOTICE : hasErrors(errors) ? INVALID_REASON : null;
  const shown = (field: Field) => (touched.has(field) ? errors[field] : null);
  const update = (patch: Partial<GrantDraft>, field: Field) => {
    setTyped({ ...draft, ...patch });
    setTouched((current) => new Set(current).add(field));
  };
  const offered = offeredKinds(policy);
  const mergeDraft = mergeTyped ?? initialMergeDraft(policy);
  const mergeErrors = validateMerge(mergeDraft, policy);
  const mergeLimit = mergeLimits(policy);
  const mergeReason =
    live && !gate.connected ? NOT_CONNECTED_REASON : locked ? APPROVER_REQUIRED_NOTICE : hasMergeErrors(mergeErrors) ? MERGE_INVALID_REASON : null;
  const mergeShown = (field: MergeField) => (mergeTouched.has(field) ? mergeErrors[field] : null);
  const updateMerge = (patch: Partial<MergeDraft>, field: MergeField) => {
    setMergeTyped({ ...mergeDraft, ...patch });
    setMergeTouched((current) => new Set(current).add(field));
  };
  const nowMs = Date.now();

  // Workstreams are read only once the person asks for that scope.
  const wantWorkstreams = draft.scopeKind === "workstream";
  const loadedWorkstreams = useSurface(
    () =>
      wantWorkstreams
        ? surfaces.workstreams.workstreams()
        : Promise.resolve(notOk<Workstream[]>(surfaces.workstreams.provenance, "unsupported", "not asked for", observation(Date.now()))),
    `grants-workstreams:${wantWorkstreams}`,
    surfaces.workstreams.provenance,
  );
  const workstreams = loadedWorkstreams.status === "ok" ? loadedWorkstreams.data : [];
  const projectOptions = (projects.data ?? [])
    .filter((row) => row.archived !== true)
    .map((row) => ({ value: projectCode(row.id), label: basename(row.id) }));
  const scopeOptions = wantWorkstreams
    ? workstreams.map((item) => ({ value: item.id, label: item.title }))
    : projectOptions;

  // A minor confirmation (a new approval) clears itself; a destructive or merge result stays until the next action.
  const clearTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const finish = (kind: "ok" | "bad", text: string, minor = false) => {
    clearTimeout(clearTimer.current);
    setAnnouncement({ kind, text });
    if (minor && kind === "ok") clearTimer.current = setTimeout(() => setAnnouncement(null), CLEAR_MS);
  };

  /** One create for both forms: `blocked` is the form's disabled reason, `reset` clears the form once the grant is saved. */
  const submit = async (blocked: string | null, busyKey: string, terms: GrantTerms, reset: () => void, failure: typeof createFailureText) => {
    if (blocked !== null || working.current) return;
    working.current = true;
    setBusy(busyKey);
    setAnnouncement(null);
    try {
      const receipt = await source.create(terms);
      if (receipt.simulated) {
        finish("ok", receipt.notice ?? FIXTURE_TEXT, busyKey === "create");
      } else {
        finish("ok", createdText(receipt.grant), busyKey === "create");
        reset();
        reload();
      }
    } catch (error) {
      finish("bad", failure(asDaemonError(error)));
    } finally {
      working.current = false;
      setBusy(null);
    }
  };

  const create = () =>
    submit(reason, "create", termsOf(draft, policy), () => {
      setAdding(false);
      setTyped(null);
      setTouched(new Set());
    }, createFailureText);
  const createMerge = () =>
    submit(mergeReason, "merge", mergeTermsOf(mergeDraft), () => {
      setAdding(false);
      // The prefix stays: it is a per-machine choice, not part of one approval.
      setMergeTyped({ ...initialMergeDraft(policy), prefix: mergeDraft.prefix });
      setMergeTouched(new Set());
    }, mergeCreateFailureText);

  const revoke = async (grant: GrantItem) => {
    if (locked || working.current) return;
    working.current = true;
    setBusy(`revoke:${grant.id}`);
    setAnnouncement(null);
    try {
      const receipt = await source.revoke(grant.id);
      finish("ok", receipt.simulated ? `${revokedText(0)} ${receipt.notice ?? FIXTURE_TEXT}` : revokedText(receipt.stopped));
      reload();
    } catch (error) {
      finish("bad", revokeFailureText(asDaemonError(error)));
    } finally {
      working.current = false;
      setBusy(null);
      // The Revoke button is off once the grant ends: focus moves to its row so it is not lost to the page.
      rows.current.get(grant.id)?.focus();
    }
  };

  // One polite region for the whole section, so a create and a revoke never overwrite each other unheard. A failure is an
  // alert beside it, not inside it. It sits in the dialog while that is open, so an error is seen.
  const busyLabel = busy === null ? null : busy === "create" ? "Creating…" : busy === "merge" ? "Saving…" : "Revoking…";
  const announced = (
    <div className="settings-result">
      <div role="status" aria-live="polite" aria-label="Standing approvals result">
        {busyLabel !== null && <p className="ui-action-pending">{busyLabel}</p>}
        {announcement?.kind === "ok" && <p>{announcement.text}</p>}
      </div>
      {announcement?.kind === "bad" && (
        <Notice tone="bad" role="alert">
          {announcement.text}
        </Notice>
      )}
    </div>
  );

  return (
    <>
      <div className="settings-head-row">
        <h3 id={headingId}>{SECTION_TITLE}</h3>
        <Button onClick={() => setAdding(true)}>Add</Button>
      </div>
      {adding && (
        <Dialog title="Add a standing approval" onClose={() => setAdding(false)} footer={<Button onClick={() => setAdding(false)}>Cancel</Button>}>
          <p>{INTRO_TEXT}</p>
          <p className="settings-hint">{NEVER_GRANTED_TEXT}</p>
      <form
        noValidate
        aria-disabled={locked || undefined}
        onSubmit={(event) => {
          event.preventDefault();
          void create();
        }}
      >
        <fieldset className="settings-fieldset" disabled={locked || busy !== null}>
          <div className="settings-row" role="radiogroup" aria-labelledby={ids.scope}>
            <span id={ids.scope} className="settings-label">
              Covers
            </span>
            <div className="settings-field">
              {(["project", "workstream"] as const).map((scopeKind) => (
                <label key={scopeKind} className="settings-choice">
                  <input
                    type="radio"
                    name={`${ids.scope}-kind`}
                    value={scopeKind}
                    checked={draft.scopeKind === scopeKind}
                    onChange={() => update({ scopeKind, scopeId: "" }, "scope")}
                  />
                  {scopeKind === "project" ? "One project" : "One workstream"}
                </label>
              ))}
            </div>
          </div>
          <div className="settings-row">
            <label htmlFor={ids.select} className="settings-label">
              {draft.scopeKind === "project" ? "Project" : "Workstream"}
            </label>
            <select
              id={ids.select}
              value={draft.scopeId}
              aria-invalid={shown("scope") ? "true" : undefined}
              aria-describedby={describedBy(shown("scope") && `${ids.select}-error`)}
              onChange={(event) => update({ scopeId: event.target.value }, "scope")}
            >
              <option value="">{scopeOptions.length === 0 ? `No ${draft.scopeKind}s to choose` : `Choose a ${draft.scopeKind}`}</option>
              {scopeOptions.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
            {shown("scope") && (
              <p id={`${ids.select}-error`} role="alert" className="settings-error">
                {errors.scope}
              </p>
            )}
          </div>

          <fieldset className="settings-row settings-kinds" aria-describedby={describedBy(ids.kinds, shown("kinds") && `${ids.kinds}-error`)}>
            <legend>Kinds of action</legend>
            {offered.map((kind) => (
              <label key={kind} className="settings-choice">
                <input
                  type="checkbox"
                  value={kind}
                  checked={draft.kinds.includes(kind)}
                  onChange={(event) =>
                    update({ kinds: event.target.checked ? [...draft.kinds, kind] : draft.kinds.filter((item) => item !== kind) }, "kinds")
                  }
                />
                {kindLabel(kind)}
              </label>
            ))}
            <p id={ids.kinds} className="settings-hint">
              {offered.includes("workstream.update") ? `${UPDATE_NOTE} ` : ""}
              Choose at least one.
            </p>
            {shown("kinds") && (
              <p id={`${ids.kinds}-error`} role="alert" className="settings-error">
                {errors.kinds}
              </p>
            )}
          </fieldset>

          <NumberField
            id={ids.hours}
            label="Expires in (hours)"
            hint={`Default ${Math.round(policy.defaultExpiresInSecs / 3600)}, at most ${maxHours(policy)} (7 days). The approval ends then.`}
            value={draft.hours}
            error={shown("hours")}
            onChange={(hours) => update({ hours }, "hours")}
          />
          <NumberField
            id={ids.uses}
            label="Uses"
            hint={`Default ${policy.defaultMaxUses}, at most ${policy.maxMaxUses}. Each action it approves is one use.`}
            value={draft.uses}
            error={shown("uses")}
            onChange={(uses) => update({ uses }, "uses")}
          />
          <NumberField
            id={ids.budget}
            label="Token budget"
            hint={TOKEN_NOTE}
            value={draft.budget}
            error={shown("budget")}
            onChange={(budget) => update({ budget }, "budget")}
          />
        </fieldset>
        <div className="settings-actions">
          <Button
            type="submit"
            variant="primary"
            disabled={reason !== null}
            pending={busy === "create"}
            aria-describedby={reason === null ? undefined : ids.reason}
          >
            Create standing approval
          </Button>
          {reason !== null && (
            <p id={ids.reason} className="settings-reason">
              {reason}
            </p>
          )}
        </div>
      </form>

      {mergeOffered(policy) && (
        <Disclosure summary="Unattended merges (advanced)">
        <form
          noValidate
          aria-label={MERGE_TITLE}
          aria-disabled={locked || undefined}
          className="settings-danger"
          onSubmit={(event) => {
            mergeGuard.guard(mergeReason, () => void createMerge())(event);
            // A blocked save shows every field's error, not only the ones already touched.
            if (mergeReason !== null) setMergeTouched(new Set<MergeField>(["scope", "prefix", "hours", "uses", "budget", "confirmed"]));
          }}
        >
          <fieldset className="settings-fieldset" disabled={locked || busy !== null}>
            <legend>{MERGE_TITLE}</legend>
            <Notice tone="warn">{MERGE_INTRO_TEXT}</Notice>
            <div className="settings-row">
              <label htmlFor={mergeIds.select} className="settings-label">
                Project for unattended merges
              </label>
              <select
                id={mergeIds.select}
                value={mergeDraft.scopeId}
                aria-invalid={mergeShown("scope") ? "true" : undefined}
                aria-describedby={describedBy(mergeShown("scope") && `${mergeIds.select}-error`)}
                onChange={(event) => updateMerge({ scopeId: event.target.value }, "scope")}
              >
                <option value="">{projectOptions.length === 0 ? "No projects to choose" : "Choose a project"}</option>
                {projectOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
              {mergeShown("scope") && (
                <p id={`${mergeIds.select}-error`} role="alert" className="settings-error">
                  {mergeErrors.scope}
                </p>
              )}
            </div>
            <div className="settings-row">
              <label htmlFor={mergeIds.prefix}>Branch prefix</label>
              <input
                id={mergeIds.prefix}
                type="text"
                autoComplete="off"
                value={mergeDraft.prefix}
                aria-invalid={mergeShown("prefix") ? "true" : undefined}
                aria-describedby={describedBy(`${mergeIds.prefix}-hint`, mergeShown("prefix") && `${mergeIds.prefix}-error`)}
                onChange={(event) => updateMerge({ prefix: event.target.value }, "prefix")}
              />
              <p id={`${mergeIds.prefix}-hint`} className="settings-hint">
                Merge only pull requests whose branch starts with this, for example mcao/. Required.
              </p>
              {mergeShown("prefix") && (
                <p id={`${mergeIds.prefix}-error`} role="alert" className="settings-error">
                  {mergeErrors.prefix}
                </p>
              )}
            </div>
            <NumberField
              id={mergeIds.hours}
              label="Merge approval expires in (hours)"
              hint={`Default ${mergeLimit.defaultHours}, at most ${mergeLimit.maxHours}. The approval ends then.`}
              value={mergeDraft.hours}
              error={mergeShown("hours")}
              onChange={(hours) => updateMerge({ hours }, "hours")}
            />
            <NumberField
              id={mergeIds.uses}
              label="Merge approval uses"
              hint={`Default ${mergeLimit.defaultUses}, at most ${mergeLimit.maxUses}. Each merge is one use.`}
              value={mergeDraft.uses}
              error={mergeShown("uses")}
              onChange={(uses) => updateMerge({ uses }, "uses")}
            />
            <NumberField
              id={mergeIds.budget}
              label="Merge approval token budget"
              hint={MERGE_BUDGET_NOTE}
              value={mergeDraft.budget}
              error={mergeShown("budget")}
              onChange={(budget) => updateMerge({ budget }, "budget")}
            />
            <div className="settings-row">
              <label className="settings-choice">
                <input
                  type="checkbox"
                  checked={mergeDraft.confirmed}
                  aria-invalid={mergeShown("confirmed") ? "true" : undefined}
                  aria-describedby={describedBy(mergeShown("confirmed") && `${mergeIds.confirm}-error`)}
                  onChange={(event) => updateMerge({ confirmed: event.target.checked }, "confirmed")}
                />
                {MERGE_CONFIRM_TEXT}
              </label>
              {mergeShown("confirmed") && (
                <p id={`${mergeIds.confirm}-error`} role="alert" className="settings-error">
                  {mergeErrors.confirmed}
                </p>
              )}
            </div>
          </fieldset>
          <div className="settings-actions">
            <SaveButton label="Save unattended merge approval" reason={mergeReason} saving={busy === "merge"} tried={mergeGuard.tried} reasonId={mergeIds.reason} />
          </div>
        </form>
        </Disclosure>
      )}

          {announced}
        </Dialog>
      )}
      {grants.length === 0 ? (
        <p className="settings-hint">None yet. Nothing runs without your approval.</p>
      ) : (
        <ul className="settings-grants" aria-label="Standing approvals">
          {grants.map((grant) => {
            const chip = statusChip(grant, nowMs);
            const name = `${scopeText(grant)}, ${shortGrantId(grant.id)}`;
            return (
              <li
                key={grant.id}
                className="settings-grant"
                tabIndex={-1}
                ref={(node) => {
                  if (node === null) rows.current.delete(grant.id);
                  else rows.current.set(grant.id, node);
                }}
              >
                <span className="settings-grant-title">
                  {scopeText(grant)} <span className="settings-hint">{shortGrantId(grant.id)}</span>
                  <Pill tone={chip.tone}>{chip.text}</Pill>
                  {isMergeGrant(grant) && <Pill tone="bad">{MERGE_BADGE}</Pill>}
                  {fixture && <Badge tone="placeholder">Fixture data</Badge>}
                </span>
                <span className="settings-hint">{grant.kinds.map(kindLabel).join(", ")}</span>
                {isMergeGrant(grant) && <span className="settings-hint">{branchPrefixText(grant)}</span>}
                <span className="settings-hint">
                  {usesLeftText(grant)} · {timeLeftText(grant, nowMs)} · {tokensText(grant)}
                </span>
                {isActive(grant, nowMs) && (
                  <Button
                    aria-label={`Revoke standing approval: ${name}`}
                    aria-describedby={locked ? `${ids.reason}-revoke` : undefined}
                    disabled={locked || (busy !== null && busy !== `revoke:${grant.id}`)}
                    pending={busy === `revoke:${grant.id}`}
                    onClick={() => void revoke(grant)}
                  >
                    Revoke
                  </Button>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {locked && (
        <p id={`${ids.reason}-revoke`} className="settings-reason">
          {REVOKE_LOCKED_NOTE}
        </p>
      )}
      {!adding && announced}
    </>
  );
}

function NumberField({
  id,
  label,
  hint,
  value,
  error,
  onChange,
}: {
  id: string;
  label: string;
  hint: string;
  value: string;
  error: string | null;
  onChange: (text: string) => void;
}) {
  return (
    <div className="settings-row">
      <label htmlFor={id}>{label}</label>
      <input
        id={id}
        type="text"
        inputMode="numeric"
        autoComplete="off"
        value={value}
        aria-invalid={error ? "true" : undefined}
        aria-describedby={describedBy(`${id}-hint`, error && `${id}-error`)}
        onChange={(event) => onChange(event.target.value)}
      />
      <p id={`${id}-hint`} className="settings-hint">
        {hint}
      </p>
      {error && (
        <p id={`${id}-error`} role="alert" className="settings-error">
          {error}
        </p>
      )}
    </div>
  );
}

export function GrantsSection() {
  const { surfaces } = useServices();
  const gate = useGate();
  const source = surfaces.grants;
  const live = source.provenance.kind === "daemon";
  // Read from the negotiated list, so the section does not flicker while the connection restarts.
  const supported = !live || gate.connection.capabilities.includes("standing_approvals");
  const [nonce, setNonce] = useState(0);
  const headingId = useId();
  const loaded = useSurface(
    () =>
      supported
        ? source.list()
        : Promise.resolve(notOk<GrantsSnapshot>(source.provenance, "unsupported", "This daemon has no standing approvals.", observation(Date.now()))),
    `grants#${nonce}`,
    source.provenance,
    supported ? { pollMs: GRANTS_POLL_MS, onFocus: true } : undefined,
  );
  // The last answer stays on screen while a refresh loads, so a typed form is never torn down by a poll.
  const last = useRef<typeof loaded | null>(null);
  if (loaded.status !== "loading") last.current = loaded;
  const result = loaded.status === "loading" && last.current !== null ? last.current : loaded;

  if (!supported || result.status === "unsupported" || result.status === "loading") return null;
  if (gate.settling && result.status !== "ok") return null;
  return (
    <section aria-labelledby={headingId} className="settings-section">
      {result.status !== "ok" ? (
        <>
          <h3 id={headingId}>{SECTION_TITLE}</h3>
          <Notice tone="warn" role="status">
            Standing approvals could not be read. {result.reason}
          </Notice>
        </>
      ) : (
        <GrantsBody
          grants={result.data.grants}
          policy={result.data.policy}
          fixture={!live}
          headingId={headingId}
          reload={() => setNonce((value) => value + 1)}
        />
      )}
    </section>
  );
}
