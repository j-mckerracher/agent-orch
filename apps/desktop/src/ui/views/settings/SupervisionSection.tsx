/** DPES-1105 Phase 3: the "Checks on running work" and "Automatic replies" sections of the Settings page. */
import { useEffect, useId, useState, type FormEvent, type ReactNode } from "react";
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import type { SettingsSnapshot, SupervisionSettings, WakeLimitSettings } from "../../../sources/surfaces/types";
import { useGate } from "../../services";
import { EmptyState } from "../../theme";
import { Notice } from "../../widgets";
import { NOT_CONNECTED_REASON } from "./settingsModel";
import { SaveButton, useSaveGuard } from "./SaveBar";
import {
  BASE_MINUTES_MAX,
  BASE_MINUTES_MIN,
  FIXTURE_SAVE_TEXT,
  INVALID_REASON,
  NO_LIMIT_NOTE,
  SUPERVISION_INTRO,
  SUPERVISION_LABEL,
  SUPERVISION_SAVED_TEXT,
  UNAVAILABLE_TEXT,
  WAKE_FIELDS,
  WAKE_HINT,
  WAKE_INTRO,
  WAKE_LABEL,
  WAKE_SAVED_TEXT,
  WALL_MINUTES_MAX,
  WALL_MINUTES_MIN,
  anyUnlimited,
  previewInputs,
  previewText,
  supervisionErrors,
  supervisionPatch,
  supervisionText,
  wakeError,
  wakePatch,
  wakeText,
  MIN_SECONDS_FLOOR,
  type SupervisionField,
  type SupervisionTyped,
  type WakeField,
  type WakeTyped,
} from "./supervisionModel";
import type { SettingsModel, SaveResult } from "./useSettings";
import { useSettingsGroups, type SettingsGroupsModel } from "./useSettingsGroups";

const HINT: Record<SupervisionField, string> = {
  baseMinutes: `Whole number from ${BASE_MINUTES_MIN} to ${BASE_MINUTES_MAX}.`,
  minSeconds: `Whole number from ${MIN_SECONDS_FLOOR} up to the check interval.`,
  wallMinutes: `Whole number from ${WALL_MINUTES_MIN} to ${WALL_MINUTES_MAX}.`,
};

function Field({
  label,
  hint,
  extraHint,
  value,
  error,
  readOnly,
  numeric,
  onChange,
}: {
  label: string;
  hint: string;
  /** Another element that describes the field, such as the interval preview. */
  extraHint?: string;
  value: string;
  error: string | null;
  readOnly: boolean;
  /** True for a number input; the wake fields take words too, so they are text. */
  numeric: boolean;
  onChange: (text: string) => void;
}) {
  const id = useId();
  const hintId = `${id}-hint`;
  const errorId = `${id}-error`;
  const describedBy = [hintId, extraHint, error && errorId].filter(Boolean).join(" ");
  return (
    <div className="settings-row">
      <label htmlFor={id}>{label}</label>
      <div className="settings-field">
        <input
          id={id}
          type={numeric ? "number" : "text"}
          inputMode="numeric"
          step={numeric ? 1 : undefined}
          value={value}
          readOnly={readOnly}
          aria-invalid={error ? "true" : undefined}
          aria-describedby={describedBy}
          onChange={(event) => onChange(event.target.value)}
        />
      </div>
      <p id={hintId} className="settings-hint">
        {hint}
      </p>
      {/* Mounted only while the value is invalid, so the alert is read when the message first appears. */}
      {error && (
        <p id={errorId} role="alert" className="settings-error">
          {error}
        </p>
      )}
    </div>
  );
}

function Actions({
  label,
  reason,
  tried,
  saving,
  result,
  savedText,
}: {
  label: string;
  reason: string | null;
  tried: boolean;
  saving: boolean;
  result: SaveResult | null;
  savedText: string;
}) {
  return (
    <div className="settings-actions">
      <SaveButton label={label} reason={reason} saving={saving} tried={tried} />
      {/* One live region per section, so two saves never overwrite each other's announcement. */}
      <div role="status" aria-live="polite" aria-label={`${label} result`} className="settings-result">
        {result?.kind === "saved" && <p>{savedText}</p>}
        {result?.kind === "fixture" && <p>{FIXTURE_SAVE_TEXT}</p>}
        {result?.kind === "failed" && <Notice tone="bad">{result.message}</Notice>}
      </div>
    </div>
  );
}

/** The reason Save is off for lack of a connection or approver, else null. */
function useLockReason(model: SettingsModel): { locked: boolean; reason: string | null } {
  const gate = useGate();
  const locked = model.live && !(gate.connected && gate.connection.approver);
  if (model.live && !gate.connected) return { locked, reason: NOT_CONNECTED_REASON };
  return { locked, reason: locked ? APPROVER_REQUIRED_NOTICE : null };
}

function Section({ title, intro, children, onSubmit }: { title: string; intro: string; children: ReactNode; onSubmit: (event: FormEvent) => void }) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="settings-section">
      <h2 id={headingId}>{title}</h2>
      <p className="settings-intro">{intro}</p>
      <form noValidate onSubmit={onSubmit}>
        {children}
      </form>
    </section>
  );
}

function ChecksSection({
  model,
  groups,
  saved,
  onSaved,
}: {
  model: SettingsModel;
  groups: SettingsGroupsModel;
  saved: SupervisionSettings;
  onSaved: () => void;
}) {
  const [typed, setTyped] = useState<SupervisionTyped>({});
  const { locked, reason: lockReason } = useLockReason(model);
  const previewId = useId();
  // An untouched field follows the daemon on every refresh; a typed one is never overwritten.
  const shown = { ...supervisionText(saved), ...typed };
  const errors = supervisionErrors(saved, typed);
  const patch = supervisionPatch(saved, typed);
  const invalid = (Object.keys(errors) as SupervisionField[]).some((field) => errors[field] !== null);
  const reason = lockReason ?? (invalid ? INVALID_REASON : null);
  const { tried, guard } = useSaveGuard();
  const inputs = previewInputs(saved, typed);
  const fields: SupervisionField[] = ["baseMinutes", "minSeconds", "wallMinutes"];

  return (
    <Section
      title="Checks on running work"
      intro={SUPERVISION_INTRO}
      onSubmit={guard(reason, () => {
        void groups.save("supervision", { supervision: patch }).then((ok) => {
          if (!ok) return;
          setTyped({});
          onSaved();
        });
      })}
    >
      {fields.map((field) => (
        <Field
          key={field}
          label={SUPERVISION_LABEL[field]}
          hint={HINT[field]}
          extraHint={field === "minSeconds" ? previewId : undefined}
          value={shown[field]}
          error={errors[field]}
          readOnly={locked}
          numeric
          onChange={(text) => setTyped((current) => ({ ...current, [field]: text }))}
        />
      ))}
      <p id={previewId} className="settings-hint">
        {inputs ? previewText(inputs.baseSecs, inputs.minSecs) : ""}
      </p>
      <Actions label="Save check settings" reason={reason} tried={tried} saving={groups.saving.supervision} result={groups.results.supervision} savedText={SUPERVISION_SAVED_TEXT} />
    </Section>
  );
}

function RepliesSection({
  model,
  groups,
  saved,
  onSaved,
}: {
  model: SettingsModel;
  groups: SettingsGroupsModel;
  saved: WakeLimitSettings;
  onSaved: () => void;
}) {
  const [typed, setTyped] = useState<WakeTyped>({});
  const { locked, reason: lockReason } = useLockReason(model);
  const shown = { ...wakeText(saved), ...typed };
  const errorOf = (field: WakeField) => (typed[field] === undefined ? null : wakeError(field, typed[field]!));
  const patch = wakePatch(saved, typed);
  const invalid = WAKE_FIELDS.some((field) => errorOf(field) !== null);
  const reason = lockReason ?? (invalid ? INVALID_REASON : null);
  const { tried, guard } = useSaveGuard();

  return (
    <Section
      title="Automatic replies"
      intro={WAKE_INTRO}
      onSubmit={guard(reason, () => {
        void groups.save("wake", { wakeLimits: patch }).then((ok) => {
          if (!ok) return;
          setTyped({});
          onSaved();
        });
      })}
    >
      {WAKE_FIELDS.map((field) => (
        <Field
          key={field}
          label={WAKE_LABEL[field]}
          hint={WAKE_HINT[field]}
          value={shown[field]}
          error={errorOf(field)}
          readOnly={locked}
          numeric={false}
          onChange={(text) => setTyped((current) => ({ ...current, [field]: text }))}
        />
      ))}
      {anyUnlimited(saved, typed) && <p className="settings-hint">{NO_LIMIT_NOTE}</p>}
      <Actions label="Save reply limits" reason={reason} tried={tried} saving={groups.saving.wake} result={groups.results.wake} savedText={WAKE_SAVED_TEXT} />
    </Section>
  );
}

/** Renders nothing until the page has read the settings; the load section already says why when it cannot. */
export function SupervisionSections({ model }: { model: SettingsModel }) {
  const groups = useSettingsGroups();
  const { fresh, dropFresh } = groups;
  const snapshot: SettingsSnapshot | null = model.load.status === "ready" ? model.load.snapshot : null;
  // A newer read replaces the snapshot a save answered with.
  useEffect(() => dropFresh(), [snapshot, dropFresh]);
  if (snapshot === null) return null;
  const current = fresh ?? snapshot;
  if (!current.supervision && !current.wakeLimits) {
    return <EmptyState role="status" title={UNAVAILABLE_TEXT} />;
  }
  return (
    <>
      {current.supervision && <ChecksSection model={model} groups={groups} saved={current.supervision} onSaved={model.reload} />}
      {current.wakeLimits && <RepliesSection model={model} groups={groups} saved={current.wakeLimits} onSaved={model.reload} />}
    </>
  );
}
