/** DPES-1105 Phase 5: the "Standing approvals" section of the Settings page. Hidden quietly for a daemon without the capability. */
import { useId, useRef, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError } from "../../../daemon/errors";
import { basename } from "../../format";
import { notOk, observation } from "../../../sources/surfaces/result";
import type { GrantItem, GrantPolicy, GrantsSnapshot, Workstream } from "../../../sources/surfaces/types";
import { useGate, useServices, useStore, useSurface } from "../../services";
import { Badge, Button, Pill } from "../../theme";
import { Notice } from "../../widgets";
import {
  FIXTURE_TEXT,
  INTRO_TEXT,
  INVALID_REASON,
  NEVER_GRANTED_TEXT,
  NOT_CONNECTED_REASON,
  REVOKE_LOCKED_NOTE,
  SECTION_TITLE,
  TOKEN_NOTE,
  UPDATE_NOTE,
  createFailureText,
  createdText,
  hasErrors,
  initialDraft,
  isActive,
  kindLabel,
  maxHours,
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
  type GrantDraft,
} from "./grantsModel";
import { projectCode } from "./projectCode";

/** A grant ends with no push from the daemon, so the list is read again on this interval. */
export const GRANTS_POLL_MS = 15_000;

type Field = "scope" | "kinds" | "hours" | "uses" | "budget";

const describedBy = (...ids: Array<string | false | null | undefined>) => ids.filter(Boolean).join(" ") || undefined;

function GrantsBody({ grants, policy, fixture, reload }: { grants: GrantItem[]; policy: GrantPolicy; fixture: boolean; reload: () => void }) {
  const { surfaces, data } = useServices();
  const gate = useGate();
  const source = surfaces.grants;
  const { projects } = useStore(data);
  const [typed, setTyped] = useState<GrantDraft | null>(null);
  const [touched, setTouched] = useState<ReadonlySet<Field>>(new Set());
  const [busy, setBusy] = useState<string | null>(null);
  const [announcement, setAnnouncement] = useState<{ kind: "ok" | "bad"; text: string } | null>(null);
  const rows = useRef(new Map<string, HTMLLIElement>());
  const working = useRef(false);
  const ids = { reason: useId(), scope: useId(), kinds: useId(), hours: useId(), uses: useId(), budget: useId(), select: useId() };

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

  const finish = (kind: "ok" | "bad", text: string) => setAnnouncement({ kind, text });

  const create = async () => {
    if (reason !== null || working.current) return;
    working.current = true;
    setBusy("create");
    setAnnouncement(null);
    try {
      const receipt = await source.create(termsOf(draft, policy));
      if (receipt.simulated) {
        finish("ok", receipt.notice ?? FIXTURE_TEXT);
      } else {
        finish("ok", createdText(receipt.grant));
        setTyped(null);
        setTouched(new Set());
        reload();
      }
    } catch (error) {
      finish("bad", createFailureText(asDaemonError(error)));
    } finally {
      working.current = false;
      setBusy(null);
    }
  };

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

  return (
    <>
      <p className="settings-intro">{INTRO_TEXT}</p>
      <p className="settings-hint">
        {NEVER_GRANTED_TEXT}
      </p>
      <form
        noValidate
        aria-disabled={locked || undefined}
        onSubmit={(event) => {
          event.preventDefault();
          void create();
        }}
      >
        <fieldset className="settings-fieldset" disabled={locked || busy !== null}>
          <legend>Create a standing approval</legend>
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
            disabled={reason !== null || busy !== null}
            aria-busy={busy === "create"}
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

      <h3 className="settings-subhead">Your standing approvals</h3>
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
                  {fixture && <Badge tone="placeholder">Fixture data</Badge>}
                </span>
                <span className="settings-hint">{grant.kinds.map(kindLabel).join(", ")}</span>
                <span className="settings-hint">
                  {usesLeftText(grant)} · {timeLeftText(grant, nowMs)} · {tokensText(grant)}
                </span>
                {isActive(grant, nowMs) && (
                  <Button
                    aria-label={`Revoke standing approval: ${name}`}
                    aria-describedby={locked ? `${ids.reason}-revoke` : undefined}
                    disabled={locked || busy !== null}
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
      {/* One polite region for the whole section, so a create and a revoke never overwrite each other unheard. */}
      <div role="status" aria-live="polite" aria-label="Standing approvals result" className="settings-result">
        {announcement?.kind === "ok" && <p>{announcement.text}</p>}
        {announcement?.kind === "bad" && <Notice tone="bad">{announcement.text}</Notice>}
      </div>
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
      <h2 id={headingId}>{SECTION_TITLE}</h2>
      {result.status !== "ok" ? (
        <Notice tone="warn" role="status">
          Standing approvals could not be read. {result.reason}
        </Notice>
      ) : (
        <GrantsBody
          grants={result.data.grants}
          policy={result.data.policy}
          fixture={!live}
          reload={() => setNonce((value) => value + 1)}
        />
      )}
    </section>
  );
}
