/** DPES-1105 Settings (`#/settings`): the machine load limits. Rendered inside the shell's `main` landmark, so it adds none. */
import { useId, useState } from "react";
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import type { LoadMeterName } from "../../../daemon/types";
import type { LoadReading, SettingsSnapshot } from "../../../sources/surfaces/types";
import { useGate } from "../../services";
import { Badge, Button, EmptyState, Pill } from "../../theme";
import { Notice } from "../../widgets";
import {
  DISK_NOTE,
  FIXTURE_SAVE_TEXT,
  INTRO_TEXT,
  INVALID_REASON,
  LOAD_METERS,
  LIMIT_LABEL,
  MAX_LIMIT,
  MIN_LIMIT,
  NOT_CONNECTED_REASON,
  OFF_NOTE,
  REFRESH_FAILED_TEXT,
  SAVED_TEXT,
  UNCHANGED_REASON,
  UNSUPPORTED_DETAIL,
  UNSUPPORTED_TITLE,
  gateStatus,
  hintText,
  limitError,
  limitPatch,
  parseLimit,
  readingText,
} from "./settingsModel";
import { ChatPullRequestSection } from "./ChatPullRequestSection";
import { McpToolsSection } from "./McpToolsSection";
import { GrantsSection } from "./GrantsSection";
import { SupervisionSections } from "./SupervisionSection";
import { useSettings, type SettingsModel } from "./useSettings";
import "./SettingsView.css";

type Typed = Partial<Record<LoadMeterName, string>>;

function LimitRow({
  meter,
  value,
  reading,
  error,
  readOnly,
  onChange,
}: {
  meter: LoadMeterName;
  value: string;
  reading: LoadReading;
  error: string | null;
  readOnly: boolean;
  onChange: (text: string) => void;
}) {
  const id = useId();
  const hintId = `${id}-hint`;
  const offId = `${id}-off`;
  const errorId = `${id}-error`;
  const off = parseLimit(value) === MAX_LIMIT;
  const describedBy = [hintId, off && offId, error && errorId].filter(Boolean).join(" ");
  return (
    <div className="settings-row">
      <label htmlFor={id}>{LIMIT_LABEL[meter]}</label>
      <div className="settings-field">
        <input
          id={id}
          type="number"
          inputMode="numeric"
          min={MIN_LIMIT}
          max={MAX_LIMIT}
          step={1}
          value={value}
          readOnly={readOnly}
          aria-invalid={error ? "true" : undefined}
          aria-describedby={describedBy}
          onChange={(event) => onChange(event.target.value)}
        />
        {off && (
          <span id={offId} className="settings-off">
            {OFF_NOTE}
          </span>
        )}
      </div>
      <p id={hintId} className="settings-hint">
        {hintText(meter, reading)}
        {meter === "disk" && (
          <>
            {" "}
            <span className="settings-hint-note">{DISK_NOTE}</span>
          </>
        )}
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

function LimitsForm({ model, snapshot, refreshFailed }: { model: SettingsModel; snapshot: SettingsSnapshot; refreshFailed: boolean }) {
  const gate = useGate();
  const reasonId = useId();
  // What the user has typed, by meter. A field that was not touched follows the daemon on every refresh.
  const [typed, setTyped] = useState<Typed>({});
  const status = gateStatus(snapshot);
  const errorOf = (meter: LoadMeterName) => {
    const text = typed[meter];
    return text === undefined ? null : limitError(text);
  };
  const patch = limitPatch(snapshot.limits, typed);
  // Only the app that started the service can change limits; the fields stay readable without it.
  const locked = model.live && !(gate.connected && gate.connection.approver);
  const disabledReason = (): string | null => {
    if (model.live && !gate.connected) return NOT_CONNECTED_REASON;
    if (locked) return APPROVER_REQUIRED_NOTICE;
    if (LOAD_METERS.some((meter) => errorOf(meter) !== null)) return INVALID_REASON;
    return Object.keys(patch).length === 0 ? UNCHANGED_REASON : null;
  };
  const reason = disabledReason();
  const { result } = model;

  return (
    <form
      noValidate
      onSubmit={(event) => {
        event.preventDefault();
        if (reason !== null) return;
        void model.save(patch).then((saved) => saved && setTyped({}));
      }}
    >
      {refreshFailed && <Notice tone="warn" role="status">{REFRESH_FAILED_TEXT}</Notice>}
      <div className="settings-strip" data-testid="settings-load-status">
        <Pill tone={status.tone}>{status.text}</Pill>
        {snapshot.feed === "sampler" && (
          <ul className="settings-means" aria-label="Current machine load">
            {LOAD_METERS.map((meter) => (
              <li key={meter}>{readingText(meter, snapshot.gate.readings[meter])}</li>
            ))}
          </ul>
        )}
      </div>
      {/* Read aloud only when the gate changes state, never for each refresh. */}
      <p role="status" aria-live="polite" aria-label="Machine load changes" className="settings-visually-hidden">
        {model.gateAnnouncement}
      </p>
      {LOAD_METERS.map((meter) => (
        <LimitRow
          key={meter}
          meter={meter}
          value={typed[meter] ?? String(snapshot.limits[meter])}
          reading={snapshot.gate.readings[meter]}
          error={errorOf(meter)}
          readOnly={locked}
          onChange={(text) => setTyped((current) => ({ ...current, [meter]: text }))}
        />
      ))}
      <div className="settings-actions">
        <Button
          type="submit"
          variant="primary"
          disabled={reason !== null}
          aria-busy={model.saving}
          aria-describedby={reason === null ? undefined : reasonId}
        >
          Save machine load limits
        </Button>
        {reason !== null && (
          <p id={reasonId} className="settings-reason">
            {reason}
          </p>
        )}
        <div role="status" aria-live="polite" aria-label="Save result" className="settings-result">
          {result?.kind === "saved" && <p>{SAVED_TEXT}</p>}
          {result?.kind === "fixture" && <p>{FIXTURE_SAVE_TEXT}</p>}
          {result?.kind === "failed" && <Notice tone="bad">{result.message}</Notice>}
        </div>
      </div>
    </form>
  );
}

function MachineLoad({ model }: { model: SettingsModel }) {
  const gate = useGate();
  const { load } = model;
  // A first read that fails while the first connection is still opening is not an error yet.
  if (load.status === "loading" || (load.status === "failed" && gate.settling)) return <p role="status">Loading settings…</p>;
  if (load.status === "unsupported") {
    return <EmptyState role="status" title={UNSUPPORTED_TITLE} description={UNSUPPORTED_DETAIL} />;
  }
  if (load.status === "failed") {
    return (
      <div>
        <Notice tone="bad" role="alert">
          The machine load limits could not be read. {load.reason}
        </Notice>
        <Button onClick={model.reload}>Retry</Button>
      </div>
    );
  }
  return <LimitsForm model={model} snapshot={load.snapshot} refreshFailed={load.refreshFailed} />;
}

export function SettingsView() {
  const model = useSettings();
  const headingId = useId();
  return (
    <div className="settings-view" data-testid="settings-view">
      <header className="settings-head">
        <h1>Settings</h1>
        {!model.live && <Badge tone="placeholder">Fixture data</Badge>}
      </header>
      <section aria-labelledby={headingId} className="settings-section">
        <h2 id={headingId}>Machine load limits</h2>
        <p className="settings-intro">{INTRO_TEXT}</p>
        <MachineLoad model={model} />
      </section>
      <SupervisionSections model={model} />
      <ChatPullRequestSection model={model} />
      <McpToolsSection model={model} />
      <GrantsSection />
    </div>
  );
}
