/**
 * DPES-1108 phase 6: the "Preferences and learnings" section of the Settings page. Three lists of keyed one-line
 * entries: shared preferences, one project's or workstream's preferences, and learnings. Hidden quietly for a daemon
 * without the capability or the methods.
 */
import { useId, useRef, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError } from "../../../daemon/errors";
import { notOk, observation } from "../../../sources/surfaces/result";
import type { MemoryArchived, MemoryEntry, MemoryKind, MemoryReceipt, MemorySnapshot, Workstream } from "../../../sources/surfaces/types";
import { basename } from "../../format";
import { useGate, useServices, useStore, useSurface, type SurfaceState } from "../../services";
import { Badge, Button, Pill } from "../../theme";
import { Notice } from "../../widgets";
import {
  FIXTURE_TEXT,
  GLOBAL_SCOPE,
  LEARNINGS_LIST,
  LOCAL_LIST,
  NOT_CONNECTED_REASON,
  SECTION_TITLE,
  SHARED_LIST,
  authorText,
  bytesText,
  changeFailureText,
  checkKey,
  checkText,
  dayText,
  overBudget,
  overBudgetText,
  projectScope,
  projectedBytes,
  setOutcomeText,
  staleCaption,
  workstreamScope,
  type ListCopy,
} from "./preferencesModel";

interface Doc {
  result: SurfaceState<MemorySnapshot>;
  reload: () => void;
}

interface ScopeOption {
  value: string;
  label: string;
}

/** One document, read again on `reload`. The last answer for the same document stays on screen while a refresh loads. */
function useMemoryDocument(kind: MemoryKind, scope: string | null, enabled: boolean): Doc {
  const { surfaces } = useServices();
  const source = surfaces.memory;
  const [nonce, setNonce] = useState(0);
  const loaded = useSurface(
    () =>
      enabled && scope !== null
        ? source.list(kind, scope)
        : Promise.resolve(notOk<MemorySnapshot>(source.provenance, "unsupported", "Nothing to read.", observation(Date.now()))),
    `memory:${kind}:${scope}#${nonce}`,
    source.provenance,
  );
  const last = useRef<typeof loaded | null>(null);
  if (loaded.status !== "loading") last.current = loaded;
  const kept = last.current;
  const result =
    loaded.status === "loading" && kept?.status === "ok" && kept.data.kind === kind && kept.data.scope === scope ? kept : loaded;
  return { result, reload: () => setNonce((value) => value + 1) };
}

type Announcement = { kind: "ok" | "bad"; text: string } | null;

function MemoryBody({ copy, snapshot, fixture, reload }: { copy: ListCopy; snapshot: MemorySnapshot; fixture: boolean; reload: () => void }) {
  const { surfaces } = useServices();
  const gate = useGate();
  const source = surfaces.memory;
  const [addKey, setAddKey] = useState("");
  const [addText, setAddText] = useState("");
  const [editing, setEditing] = useState<{ key: string; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [announcement, setAnnouncement] = useState<Announcement>(null);
  const keyInput = useRef<HTMLInputElement>(null);
  const working = useRef(false);
  const ids = { reason: useId(), key: useId(), text: useId(), edit: useId() };
  const { kind, scope, entries, archived, budget } = snapshot;

  const live = !fixture;
  const locked = live && !(gate.connected && gate.connection.approver);
  const accessReason = live && !gate.connected ? NOT_CONNECTED_REASON : locked ? APPROVER_REQUIRED_NOTICE : null;
  const keyError = addKey === "" ? null : checkKey(addKey, snapshot);
  const textError = addText === "" ? null : checkText(addText, snapshot);
  const addAfter = projectedBytes(snapshot, addKey, addText);
  const addOver = overBudget(snapshot, addAfter) ? overBudgetText(addAfter, budget) : null;
  const formReason = accessReason ?? addOver;
  const addReason = formReason ?? checkKey(addKey, snapshot) ?? checkText(addText, snapshot);
  const inUse = entries.some((entry) => entry.key === addKey);
  const editError = editing === null ? null : checkText(editing.text, snapshot);
  const editAfter = editing === null ? snapshot.bytes : projectedBytes(snapshot, editing.key, editing.text);
  const editOver = editing !== null && overBudget(snapshot, editAfter) ? overBudgetText(editAfter, budget) : null;
  const describedBy = accessReason === null ? undefined : ids.reason;

  /** Sends one change. `answer` reads the daemon's result; `saved` false keeps the typed text and the open editor. */
  const run = async <T,>(send: () => Promise<MemoryReceipt<T>>, answer: (result: T) => { saved: boolean; text: string }, then: () => void) => {
    if (working.current) return;
    working.current = true;
    setBusy(true);
    setAnnouncement(null);
    try {
      const receipt = await send();
      if (receipt.simulated) {
        setAnnouncement({ kind: "ok", text: receipt.notice || FIXTURE_TEXT });
        return;
      }
      const { saved, text } = answer(receipt.result);
      setAnnouncement({ kind: saved ? "ok" : "bad", text });
      if (saved) then();
      reload();
    } catch (error) {
      setAnnouncement({ kind: "bad", text: changeFailureText(asDaemonError(error)) });
    } finally {
      working.current = false;
      setBusy(false);
    }
  };

  const save = (key: string, text: string, keeping: boolean, then: () => void) =>
    void run(
      () => source.set(kind, scope, key, text),
      (result) =>
        result.outcome === "over_budget"
          ? { saved: false, text: overBudgetText(result.bytes, result.budget) }
          : { saved: true, text: setOutcomeText(result.outcome, key, keeping) },
      then,
    );

  const add = () => {
    if (addReason !== null) return;
    save(addKey, addText, false, () => {
      setAddKey("");
      setAddText("");
      keyInput.current?.focus();
    });
  };
  const change = () => {
    if (editing === null || accessReason !== null || editError !== null || editOver !== null) return;
    save(editing.key, editing.text, false, () => setEditing(null));
  };
  const keep = (entry: MemoryEntry) => {
    if (accessReason !== null) return;
    save(entry.key, entry.text, true, () => undefined);
  };
  const remove = (key: string) => {
    if (accessReason !== null) return;
    void run(
      () => source.remove(kind, scope, key),
      (result) =>
        result.removed
          ? { saved: true, text: `Removed ${key}. It is under Removed entries.` }
          : { saved: false, text: `${key} was already removed. Nothing was changed.` },
      () => setEditing(null),
    );
  };
  const restore = (row: MemoryArchived) => {
    if (accessReason !== null) return;
    void run(
      () => source.restore(row.id),
      (result) =>
        result.outcome === "restored"
          ? { saved: true, text: `Restored ${row.key}.` }
          : {
              saved: false,
              text:
                result.bytes !== null && result.budget !== null
                  ? overBudgetText(result.bytes, result.budget)
                  : "Over the budget. Shorten or remove an entry.",
            },
      () => undefined,
    );
  };

  return (
    <>
      {snapshot.staleAfterDays !== null && <p className="settings-hint">{staleCaption(snapshot.staleAfterDays)}</p>}
      <p className="settings-hint">{bytesText(snapshot.bytes, budget)} used.</p>
      {entries.length === 0 ? (
        <p className="settings-hint">No entries here yet.</p>
      ) : (
        <ul className="settings-grants" aria-label={copy.title}>
          {entries.map((entry) => (
            <li key={entry.key} className="settings-grant">
              <span className="settings-grant-title">
                <code>{entry.key}</code>
                {entry.stale && <Pill tone="warn">Stale</Pill>}
                {fixture && <Badge tone="placeholder">Fixture data</Badge>}
              </span>
              {editing?.key === entry.key ? (
                <form
                  noValidate
                  className="settings-row"
                  onSubmit={(event) => {
                    event.preventDefault();
                    change();
                  }}
                >
                  <label htmlFor={`${ids.edit}-${entry.key}`}>Text for {entry.key}</label>
                  <input
                    id={`${ids.edit}-${entry.key}`}
                    type="text"
                    autoComplete="off"
                    value={editing.text}
                    disabled={busy}
                    aria-invalid={editError || editOver ? "true" : undefined}
                    aria-describedby={`${ids.edit}-${entry.key}-hint${editError || editOver ? ` ${ids.edit}-${entry.key}-error` : ""}`}
                    onChange={(event) => setEditing({ key: entry.key, text: event.target.value })}
                  />
                  <p id={`${ids.edit}-${entry.key}-hint`} className="settings-hint">
                    With this text: {bytesText(editAfter, budget)}.
                  </p>
                  {(editError || editOver) && (
                    <p id={`${ids.edit}-${entry.key}-error`} role="alert" className="settings-error">
                      {editError ?? editOver}
                    </p>
                  )}
                  <div className="settings-actions">
                    <Button
                      type="submit"
                      variant="primary"
                      aria-label={`Save ${copy.noun} ${entry.key}`}
                      disabled={busy || accessReason !== null || editError !== null || editOver !== null}
                      aria-describedby={describedBy}
                    >
                      Save
                    </Button>
                    <Button disabled={busy} onClick={() => setEditing(null)}>
                      Cancel
                    </Button>
                  </div>
                </form>
              ) : (
                <>
                  <span>{entry.text}</span>
                  <span className="settings-hint">
                    Written by {authorText(entry.writtenBy)}. Confirmed {dayText(entry.confirmedAtMs)}.
                  </span>
                  <span className="settings-actions">
                    {entry.stale && (
                      <Button
                        aria-label={`Keep ${copy.noun} ${entry.key}`}
                        aria-describedby={describedBy}
                        disabled={busy || accessReason !== null}
                        onClick={() => keep(entry)}
                      >
                        Keep
                      </Button>
                    )}
                    <Button
                      aria-label={`Edit ${copy.noun} ${entry.key}`}
                      aria-describedby={describedBy}
                      disabled={busy || accessReason !== null}
                      onClick={() => setEditing({ key: entry.key, text: entry.text })}
                    >
                      Edit
                    </Button>
                    <Button
                      aria-label={`Remove ${copy.noun} ${entry.key}`}
                      aria-describedby={describedBy}
                      disabled={busy || accessReason !== null}
                      onClick={() => remove(entry.key)}
                    >
                      Remove
                    </Button>
                  </span>
                </>
              )}
            </li>
          ))}
        </ul>
      )}

      <form
        noValidate
        aria-disabled={locked || undefined}
        onSubmit={(event) => {
          event.preventDefault();
          add();
        }}
      >
        <div className="settings-row">
          <label htmlFor={ids.key}>New {copy.noun} key</label>
          <input
            ref={keyInput}
            id={ids.key}
            type="text"
            autoComplete="off"
            value={addKey}
            disabled={busy || accessReason !== null}
            aria-invalid={keyError ? "true" : undefined}
            aria-describedby={`${ids.key}-hint${keyError ? ` ${ids.key}-error` : ""}`}
            onChange={(event) => setAddKey(event.target.value)}
          />
          <p id={`${ids.key}-hint`} className="settings-hint">
            {inUse
              ? "This key is in use. Saving replaces its text, and the old text goes under Removed entries."
              : `Lowercase letters, digits and hyphens, at most ${snapshot.maxKeyBytes} bytes.`}
          </p>
          {keyError && (
            <p id={`${ids.key}-error`} role="alert" className="settings-error">
              {keyError}
            </p>
          )}
        </div>
        <div className="settings-row">
          <label htmlFor={ids.text}>New {copy.noun} text</label>
          <input
            id={ids.text}
            type="text"
            autoComplete="off"
            value={addText}
            disabled={busy || accessReason !== null}
            aria-invalid={textError ? "true" : undefined}
            aria-describedby={`${ids.text}-hint${textError ? ` ${ids.text}-error` : ""}`}
            onChange={(event) => setAddText(event.target.value)}
          />
          <p id={`${ids.text}-hint`} className="settings-hint">
            One line, at most {snapshot.maxTextBytes} bytes. With this entry: {bytesText(addAfter, budget)}.
          </p>
          {textError && (
            <p id={`${ids.text}-error`} role="alert" className="settings-error">
              {textError}
            </p>
          )}
        </div>
        <div className="settings-actions">
          <Button
            type="submit"
            variant="primary"
            disabled={addReason !== null || busy}
            aria-busy={busy}
            aria-describedby={formReason === null ? undefined : ids.reason}
          >
            Add {copy.noun}
          </Button>
        </div>
      </form>
      {formReason !== null && (
        <p id={ids.reason} className="settings-reason">
          {formReason}
        </p>
      )}

      {archived.length > 0 && (
        <details>
          <summary>Removed entries ({archived.length})</summary>
          <ul className="settings-grants" aria-label={`Removed entries: ${copy.title}`}>
            {archived.map((row) => (
              <li key={row.id} className="settings-grant">
                <span className="settings-grant-title">
                  <code>{row.key}</code>
                  {row.restored && <Pill tone="neutral">Restored</Pill>}
                </span>
                <span>{row.text}</span>
                <span className="settings-hint">
                  {row.reason === "removed" ? "Removed" : "Replaced"} {dayText(row.archivedAtMs)}. Written by {authorText(row.writtenBy)}.
                </span>
                {!row.restored && (
                  <span className="settings-actions">
                    <Button
                      aria-label={`Restore ${copy.noun} ${row.key}`}
                      aria-describedby={describedBy}
                      disabled={busy || accessReason !== null}
                      onClick={() => restore(row)}
                    >
                      Restore
                    </Button>
                  </span>
                )}
              </li>
            ))}
          </ul>
        </details>
      )}
      {/* One polite region per list, so a change in one list never overwrites another's answer unheard. */}
      <div role="status" aria-live="polite" aria-label={`${copy.title} result`} className="settings-result">
        {announcement?.kind === "ok" && <p>{announcement.text}</p>}
        {announcement?.kind === "bad" && <Notice tone="bad">{announcement.text}</Notice>}
      </div>
    </>
  );
}

/** One list: its heading and caption, an optional picker, and the document once read. */
function MemoryList({
  copy,
  doc,
  fixture,
  picker,
  scope,
  emptyHint,
}: {
  copy: ListCopy;
  doc: Doc;
  fixture: boolean;
  picker?: { label: string; value: string; options: { first: ScopeOption; projects: ScopeOption[]; workstreams: ScopeOption[] }; onChange: (value: string) => void };
  scope: string | null;
  emptyHint?: string;
}) {
  const headingId = useId();
  const selectId = useId();
  const { result } = doc;
  return (
    <section aria-labelledby={headingId}>
      <h3 id={headingId} className="settings-subhead">
        {copy.title}
      </h3>
      <p className="settings-intro">{copy.caption}</p>
      {picker && (
        <div className="settings-row">
          <label htmlFor={selectId}>{picker.label}</label>
          <select id={selectId} value={picker.value} onChange={(event) => picker.onChange(event.target.value)}>
            <option value={picker.options.first.value}>{picker.options.first.label}</option>
            {picker.options.projects.length > 0 && (
              <optgroup label="Projects">
                {picker.options.projects.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </optgroup>
            )}
            {picker.options.workstreams.length > 0 && (
              <optgroup label="Workstreams">
                {picker.options.workstreams.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </optgroup>
            )}
          </select>
        </div>
      )}
      {scope === null ? (
        <p className="settings-hint">{emptyHint}</p>
      ) : result.status === "loading" ? (
        <p className="settings-hint" role="status">
          Loading {copy.title.toLowerCase()}.
        </p>
      ) : result.status !== "ok" ? (
        <Notice tone="warn" role="status">
          {copy.title} could not be read. {result.reason}
        </Notice>
      ) : (
        <MemoryBody key={`${result.data.kind}|${result.data.scope}`} copy={copy} snapshot={result.data} fixture={fixture} reload={doc.reload} />
      )}
    </section>
  );
}

export function PreferencesSection() {
  const { surfaces, data } = useServices();
  const gate = useGate();
  const { projects } = useStore(data);
  const live = surfaces.memory.provenance.kind === "daemon";
  // Read from the negotiated list, so the section does not flicker while the connection restarts.
  const supported = !live || gate.connection.capabilities.includes("standing_approvals");
  const [localScope, setLocalScope] = useState("");
  const [learningScope, setLearningScope] = useState(GLOBAL_SCOPE);
  const headingId = useId();

  const loadedWorkstreams = useSurface(
    () =>
      supported
        ? surfaces.workstreams.workstreams()
        : Promise.resolve(notOk<Workstream[]>(surfaces.workstreams.provenance, "unsupported", "not asked for", observation(Date.now()))),
    `preferences-workstreams:${supported}`,
    surfaces.workstreams.provenance,
  );
  const workstreamOptions = (loadedWorkstreams.status === "ok" ? loadedWorkstreams.data : []).map((item) => ({
    value: workstreamScope(item.id),
    label: item.title,
  }));
  const projectOptions = (projects.data ?? [])
    .filter((row) => row.archived !== true)
    .map((row) => ({ value: projectScope(row.id), label: basename(row.id) }));

  const shared = useMemoryDocument("preference", GLOBAL_SCOPE, supported);
  const local = useMemoryDocument("preference", localScope === "" ? null : localScope, supported);
  const learnings = useMemoryDocument("learning", learningScope, supported);

  if (!supported || (gate.settling && shared.result.status !== "ok")) return null;
  // A daemon that has the capability but not these methods.
  if (shared.result.status === "unsupported") return null;
  const fixture = !live;
  const nothingChosen = { value: "", label: projectOptions.length + workstreamOptions.length === 0 ? "No projects or workstreams to choose" : "Choose a project or workstream" };
  return (
    <section aria-labelledby={headingId} className="settings-section">
      <h2 id={headingId}>{SECTION_TITLE}</h2>
      <MemoryList copy={SHARED_LIST} doc={shared} fixture={fixture} scope={GLOBAL_SCOPE} />
      <MemoryList
        copy={LOCAL_LIST}
        doc={local}
        fixture={fixture}
        scope={localScope === "" ? null : localScope}
        emptyHint="Choose a project or workstream to see its preferences."
        picker={{
          label: "Preferences for",
          value: localScope,
          options: { first: nothingChosen, projects: projectOptions, workstreams: workstreamOptions },
          onChange: setLocalScope,
        }}
      />
      <MemoryList
        copy={LEARNINGS_LIST}
        doc={learnings}
        fixture={fixture}
        scope={learningScope}
        picker={{
          label: "Learnings for",
          value: learningScope,
          options: { first: { value: GLOBAL_SCOPE, label: "Global" }, projects: projectOptions, workstreams: workstreamOptions },
          onChange: setLearningScope,
        }}
      />
    </section>
  );
}
