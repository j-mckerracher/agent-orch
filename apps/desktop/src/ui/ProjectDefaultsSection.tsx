/** The project's Defaults tab: delivery mode, branches and checks for its tasks (DPES-1105). */
import { useEffect, useId, useRef, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError } from "../daemon/errors";
import {
  FIXTURE_SAVE_TEXT,
  MAX_CHECKS,
  MERGE_OPTIONS,
  MODE_OPTIONS,
  defaultsOf,
  draftOf,
  hasErrors,
  saveFailureText,
  validateDraft,
  type DefaultsDraft,
} from "../sources/surfaces/projectDefaults";
import type { ProjectDefaults } from "../sources/surfaces/types";
import { useGate, useServices, type SurfaceState } from "./services";
import { NOT_CONNECTED_REASON } from "./views/settings/settingsModel";
import { SaveButton, useSaveGuard } from "./views/settings/SaveBar";
import { Notice } from "./widgets";
import { Button } from "./theme";

const SAVED_TEXT = "Saved. These defaults apply to this project from now on.";
const INVALID_REASON = "Fix the fields marked as invalid before saving.";
const PREFIX_HINT =
  "Task branches start with this. Letters, digits, dot, dash, underscore and slash, ending with a slash (for example mcao/). Not orchestrator/. Leave empty to use the machine-wide prefix.";
const CHECKS_HINT = `Each command runs without a shell: type the program and its arguments separated by spaces. Quotes and variables are not interpreted. Up to ${MAX_CHECKS} commands.`;

const describedBy = (...ids: Array<string | false | null | undefined>) => ids.filter(Boolean).join(" ") || undefined;

/** Delivery defaults for one project (DPES-1105). Renders nothing when the daemon cannot offer them. */
export function ProjectDefaultsSection({ projectId, loaded }: { projectId: string; loaded: SurfaceState<ProjectDefaults> }) {
  const { surfaces } = useServices();
  const source = surfaces.projectDefaults;
  const gate = useGate();
  // What the daemon answered a save with, until a newer read replaces it; and what the user has typed (null: untouched).
  const [fresh, setFresh] = useState<ProjectDefaults | null>(null);
  const [typed, setTyped] = useState<DefaultsDraft | null>(null);
  const [saving, setSaving] = useState(false);
  const [result, setResult] = useState<{ kind: "saved" | "fixture" } | { kind: "failed"; message: string } | null>(null);
  const section = useRef<HTMLElement>(null);
  const busy = useRef(false);
  const ids = { hint: useId(), prefix: useId(), base: useId(), checks: useId(), reason: useId(), mode: useId() };

  const { tried, guard } = useSaveGuard();
  useEffect(() => setFresh(null), [loaded]);

  if (loaded.status === "unsupported") return null;
  if (loaded.status === "loading" || (loaded.status !== "ok" && gate.settling)) return <p role="status">Loading defaults…</p>;
  if (loaded.status !== "ok") {
    return (
      <section aria-label="Defaults for this project" className="ps-section">
        <Notice tone="warn" role="status">
          The defaults for this project could not be read. {loaded.reason}
        </Notice>
      </section>
    );
  }

  const saved = fresh ?? loaded.data;
  const draft = typed ?? draftOf(saved);
  const errors = validateDraft(draft);
  const live = source.provenance.kind === "daemon";
  const locked = live && !(gate.connected && gate.connection.approver);
  const reason = live && !gate.connected
    ? NOT_CONNECTED_REASON
    : locked
      ? APPROVER_REQUIRED_NOTICE
      : hasErrors(errors)
        ? INVALID_REASON
        : null;
  const update = (patch: Partial<DefaultsDraft>) => setTyped({ ...draft, ...patch });
  const setCheck = (index: number, line: string) => update({ checks: draft.checks.map((item, i) => (i === index ? line : item)) });
  const known = (options: ReadonlyArray<{ value: string }>, value: string) => options.some((option) => option.value === value);

  const save = async () => {
    if (busy.current) return;
    busy.current = true;
    setSaving(true);
    setResult(null);
    try {
      const receipt = await source.set(projectId, defaultsOf(draft));
      if (receipt.simulated) {
        setResult({ kind: "fixture" });
      } else {
        setFresh(receipt.defaults);
        setTyped(null);
        setResult({ kind: "saved" });
      }
    } catch (error) {
      setResult({ kind: "failed", message: saveFailureText(asDaemonError(error)) });
    } finally {
      busy.current = false;
      setSaving(false);
      // The Save button may be disabled by now; focus stays on the section so the result is read in place.
      section.current?.focus();
    }
  };

  return (
    <section ref={section} tabIndex={-1} aria-label="Defaults for this project" className="ps-section">
      <form noValidate className="ps-form" onSubmit={guard(reason, () => void save())}>
        <fieldset className="ps-wide" aria-describedby={describedBy(locked && ids.reason)}>
          <legend>Delivery mode</legend>
          {MODE_OPTIONS.map((option) => (
            <div key={option.value}>
              <label>
                <input
                  type="radio"
                  name={ids.mode}
                  value={option.value}
                  checked={draft.mode === option.value}
                  disabled={locked}
                  aria-describedby={describedBy(`${ids.mode}-${option.value}`, locked && ids.reason)}
                  onChange={() => update({ mode: option.value })}
                />{" "}
                {option.label}
              </label>
              <p id={`${ids.mode}-${option.value}`} className="muted">
                {option.consequence}
              </p>
            </div>
          ))}
          {!known(MODE_OPTIONS, draft.mode) && <p className="muted">The daemon reports a mode this app does not know: {draft.mode}.</p>}
        </fieldset>

        <label>
          Base branch
          <input
            value={draft.baseBranch}
            disabled={locked}
            aria-invalid={errors.baseBranch !== null}
            aria-describedby={describedBy(ids.base, errors.baseBranch !== null && `${ids.base}-error`, locked && ids.reason)}
            onChange={(e) => update({ baseBranch: e.target.value })}
          />
        </label>
        <p id={ids.base} className="muted ps-wide">
          Leave empty to branch from the current checkout
        </p>
        {errors.baseBranch !== null && (
          <Notice tone="bad" id={`${ids.base}-error`} role="alert" className="ps-wide">
            {errors.baseBranch}
          </Notice>
        )}

        <label>
          Branch prefix
          <input
            value={draft.branchPrefix}
            disabled={locked}
            aria-invalid={errors.branchPrefix !== null}
            aria-describedby={describedBy(ids.prefix, errors.branchPrefix !== null && `${ids.prefix}-error`, locked && ids.reason)}
            onChange={(e) => update({ branchPrefix: e.target.value })}
          />
        </label>
        <p id={ids.prefix} className="muted ps-wide">
          {PREFIX_HINT}
        </p>
        {errors.branchPrefix !== null && (
          <Notice tone="bad" id={`${ids.prefix}-error`} role="alert" className="ps-wide">
            {errors.branchPrefix}
          </Notice>
        )}

        <fieldset className="ps-wide" aria-describedby={describedBy(ids.checks, errors.checks !== null && `${ids.checks}-error`, locked && ids.reason)}>
          <legend>Checks</legend>
          <p id={ids.checks} className="muted">
            {CHECKS_HINT}
          </p>
          {draft.checks.length === 0 && <p className="muted">No checks. Full verification needs at least one.</p>}
          <ul className="ps-steps">
            {draft.checks.map((line, index) => (
              // Rows have no identity beyond their place, so the index is the key.
              <li key={index}>
                <label>
                  Check command {index + 1}
                  <input
                    value={line}
                    disabled={locked}
                    aria-invalid={errors.checks !== null}
                    aria-describedby={describedBy(ids.checks, locked && ids.reason)}
                    onChange={(e) => setCheck(index, e.target.value)}
                  />
                </label>{" "}
                <Button
                  disabled={locked}
                  aria-describedby={describedBy(locked && ids.reason)}
                  onClick={() => update({ checks: draft.checks.filter((_, i) => i !== index) })}
                >
                  Remove check command {index + 1}
                </Button>
              </li>
            ))}
          </ul>
          <Button
            disabled={locked || draft.checks.length >= MAX_CHECKS}
            aria-describedby={describedBy(locked ? ids.reason : draft.checks.length >= MAX_CHECKS && ids.checks)}
            onClick={() => update({ checks: [...draft.checks, ""] })}
          >
            Add check command
          </Button>
          {errors.checks !== null && (
            <Notice tone="bad" id={`${ids.checks}-error`} role="alert">
              {errors.checks}
            </Notice>
          )}
        </fieldset>

        <label>
          Merge method
          <select
            value={draft.mergeMethod}
            disabled={locked}
            aria-describedby={describedBy(locked && ids.reason)}
            onChange={(e) => update({ mergeMethod: e.target.value })}
          >
            {!known(MERGE_OPTIONS, draft.mergeMethod) && <option value={draft.mergeMethod}>{draft.mergeMethod}</option>}
            {MERGE_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>

        <div className="ps-form-footer">
          <SaveButton label="Save project defaults" reason={reason} saving={saving} tried={tried} reasonId={ids.reason} />
          <div role="status" aria-live="polite" aria-label="Project defaults save result">
            {saving && <p className="ui-action-pending">Saving…</p>}
            {result?.kind === "saved" && <p>{SAVED_TEXT}</p>}
            {result?.kind === "fixture" && <p>{FIXTURE_SAVE_TEXT}</p>}
          </div>
          {result?.kind === "failed" && (
            <Notice tone="bad" role="alert">
              {result.message}
            </Notice>
          )}
        </div>
      </form>
    </section>
  );
}

