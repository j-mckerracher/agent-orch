/** DPES-1105 5b: the "Notes for the orchestrator" section of the Settings page. Hidden quietly for a daemon without the capability. */
import { useId, useRef, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError } from "../../../daemon/errors";
import { notOk, observation } from "../../../sources/surfaces/result";
import type { NotesSnapshot, Workstream } from "../../../sources/surfaces/types";
import { useGate, useServices, useSurface } from "../../services";
import { Button } from "../../theme";
import { Notice } from "../../widgets";
import {
  FIXTURE_TEXT,
  GLOBAL_SCOPE,
  HELPER_TEXT,
  NOT_CONNECTED_REASON,
  SECTION_TITLE,
  addRefusal,
  bytesText,
  checkNote,
  saveFailureText,
  savedText,
  workstreamScope,
} from "./notesModel";

type ScopeKind = "global" | "workstream";

function NotesBody({ snapshot, fixture, reload }: { snapshot: NotesSnapshot; fixture: boolean; reload: () => void }) {
  const { surfaces } = useServices();
  const gate = useGate();
  const source = surfaces.notes;
  const [adding, setAdding] = useState("");
  const [editing, setEditing] = useState<{ index: number; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [announcement, setAnnouncement] = useState<{ kind: "ok" | "bad"; text: string } | null>(null);
  const addInput = useRef<HTMLInputElement>(null);
  const rows = useRef(new Map<number, HTMLLIElement>());
  const working = useRef(false);
  const ids = { reason: useId(), add: useId(), edit: useId() };

  const limits = snapshot;
  const notes = snapshot.notes;
  const live = !fixture;
  const locked = live && !(gate.connected && gate.connection.approver);
  const accessReason = live && !gate.connected ? NOT_CONNECTED_REASON : locked ? APPROVER_REQUIRED_NOTICE : null;
  const addError = adding === "" ? null : checkNote(adding, limits);
  const editError = editing === null ? null : checkNote(editing.text, limits);
  // What is said under the form: why nothing can be added or changed. A bad note is said at its own field.
  const listReason = accessReason ?? addRefusal(notes.length, limits);
  const addReason = listReason ?? checkNote(adding, limits);

  /** Replaces the scope's whole list; the next list comes from the daemon, so a refused save leaves the old one on screen. */
  const save = async (next: string[], action: "added" | "changed" | "deleted", then: () => void) => {
    if (working.current) return;
    working.current = true;
    setBusy(true);
    setAnnouncement(null);
    try {
      const receipt = await source.set(snapshot.scope, next);
      if (receipt.simulated) {
        setAnnouncement({ kind: "ok", text: receipt.notice ?? FIXTURE_TEXT });
      } else {
        setAnnouncement({ kind: "ok", text: savedText(action) });
        then();
        reload();
      }
    } catch (error) {
      setAnnouncement({ kind: "bad", text: saveFailureText(asDaemonError(error)) });
    } finally {
      working.current = false;
      setBusy(false);
    }
  };

  const add = () => {
    if (addReason !== null) return;
    void save([...notes, adding], "added", () => {
      setAdding("");
      addInput.current?.focus();
    });
  };
  const change = () => {
    if (editing === null || accessReason !== null || editError !== null) return;
    const { index, text } = editing;
    void save(notes.map((note, position) => (position === index ? text : note)), "changed", () => {
      setEditing(null);
      rows.current.get(index)?.focus();
    });
  };
  const remove = (index: number) => {
    if (accessReason !== null) return;
    void save(notes.filter((_, position) => position !== index), "deleted", () => {
      setEditing(null);
      addInput.current?.focus();
    });
  };

  return (
    <>
      <p className="settings-intro">{HELPER_TEXT}</p>
      {notes.length === 0 ? (
        <p className="settings-hint">No notes here yet.</p>
      ) : (
        <ul className="settings-grants" aria-label="Notes">
          {notes.map((note, index) => (
            <li
              // A note has no id: its place in the list is its identity, and every save replaces the list.
              key={index}
              className="settings-grant"
              tabIndex={-1}
              ref={(node) => {
                if (node === null) rows.current.delete(index);
                else rows.current.set(index, node);
              }}
            >
              {editing?.index === index ? (
                <form
                  noValidate
                  className="settings-row"
                  onSubmit={(event) => {
                    event.preventDefault();
                    change();
                  }}
                >
                  <label htmlFor={`${ids.edit}-${index}`}>Edit note {index + 1}</label>
                  <input
                    id={`${ids.edit}-${index}`}
                    type="text"
                    autoComplete="off"
                    value={editing.text}
                    disabled={busy}
                    aria-invalid={editError ? "true" : undefined}
                    aria-describedby={`${ids.edit}-${index}-hint${editError ? ` ${ids.edit}-${index}-error` : ""}`}
                    onChange={(event) => setEditing({ index, text: event.target.value })}
                  />
                  <p id={`${ids.edit}-${index}-hint`} className="settings-hint">
                    {bytesText(editing.text, limits)}
                  </p>
                  {editError && (
                    <p id={`${ids.edit}-${index}-error`} role="alert" className="settings-error">
                      {editError}
                    </p>
                  )}
                  <div className="settings-actions">
                    <Button
                      type="submit"
                      variant="primary"
                      disabled={busy || accessReason !== null || editError !== null}
                      aria-describedby={accessReason === null ? undefined : ids.reason}
                    >
                      Save note {index + 1}
                    </Button>
                    <Button disabled={busy} onClick={() => setEditing(null)}>
                      Cancel
                    </Button>
                  </div>
                </form>
              ) : (
                <>
                  <span>{note}</span>
                  <span className="settings-actions">
                    <Button
                      aria-label={`Edit note ${index + 1}`}
                      aria-describedby={accessReason === null ? undefined : ids.reason}
                      disabled={busy || accessReason !== null}
                      onClick={() => setEditing({ index, text: note })}
                    >
                      Edit
                    </Button>
                    <Button
                      aria-label={`Delete note ${index + 1}`}
                      aria-describedby={accessReason === null ? undefined : ids.reason}
                      disabled={busy || accessReason !== null}
                      onClick={() => remove(index)}
                    >
                      Delete
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
          <label htmlFor={ids.add}>New note</label>
          <input
            ref={addInput}
            id={ids.add}
            type="text"
            autoComplete="off"
            value={adding}
            disabled={busy || accessReason !== null}
            aria-invalid={addError ? "true" : undefined}
            aria-describedby={`${ids.add}-hint${addError ? ` ${ids.add}-error` : ""}`}
            onChange={(event) => setAdding(event.target.value)}
          />
          <p id={`${ids.add}-hint`} className="settings-hint">
            {bytesText(adding, limits)}. At most {limits.maxNotes} notes.
          </p>
          {addError && (
            <p id={`${ids.add}-error`} role="alert" className="settings-error">
              {addError}
            </p>
          )}
        </div>
        <div className="settings-actions">
          <Button
            type="submit"
            variant="primary"
            disabled={addReason !== null || busy}
            aria-busy={busy}
            aria-describedby={listReason === null ? undefined : ids.reason}
          >
            Add note
          </Button>
        </div>
      </form>
      {listReason !== null && (
        <p id={ids.reason} className="settings-reason">
          {listReason}
        </p>
      )}
      {/* One polite region for the whole section, so an add, an edit and a delete never overwrite each other unheard. */}
      <div role="status" aria-live="polite" aria-label="Notes result" className="settings-result">
        {announcement?.kind === "ok" && <p>{announcement.text}</p>}
        {announcement?.kind === "bad" && <Notice tone="bad">{announcement.text}</Notice>}
      </div>
    </>
  );
}

export function NotesSection() {
  const { surfaces } = useServices();
  const gate = useGate();
  const source = surfaces.notes;
  const live = source.provenance.kind === "daemon";
  // Read from the negotiated list, so the section does not flicker while the connection restarts.
  const supported = !live || gate.connection.capabilities.includes("standing_approvals");
  const [scopeKind, setScopeKind] = useState<ScopeKind>("global");
  const [workstreamId, setWorkstreamId] = useState("");
  const [nonce, setNonce] = useState(0);
  const headingId = useId();
  const ids = { kind: useId(), select: useId() };

  const wantWorkstreams = scopeKind === "workstream";
  const loadedWorkstreams = useSurface(
    () =>
      wantWorkstreams
        ? surfaces.workstreams.workstreams()
        : Promise.resolve(notOk<Workstream[]>(surfaces.workstreams.provenance, "unsupported", "not asked for", observation(Date.now()))),
    `notes-workstreams:${wantWorkstreams}`,
    surfaces.workstreams.provenance,
  );
  const workstreams = loadedWorkstreams.status === "ok" ? loadedWorkstreams.data : [];

  // No scope to read until a workstream is chosen.
  const scope = scopeKind === "global" ? GLOBAL_SCOPE : workstreamId === "" ? null : workstreamScope(workstreamId);
  const loaded = useSurface(
    () =>
      supported && scope !== null
        ? source.list(scope)
        : Promise.resolve(notOk<NotesSnapshot>(source.provenance, "unsupported", "No notes to read.", observation(Date.now()))),
    `notes:${scope}#${nonce}`,
    source.provenance,
  );
  // The last answer of the same scope stays on screen while a refresh loads, so a typed note is never torn down.
  const last = useRef<typeof loaded | null>(null);
  if (loaded.status !== "loading") last.current = loaded;
  const kept = last.current;
  const result =
    loaded.status === "loading" && kept?.status === "ok" && kept.data.scope === scope ? kept : loaded;

  if (!supported || (gate.settling && result.status !== "ok")) return null;
  if (scope !== null && result.status === "unsupported") return null;
  return (
    <section aria-labelledby={headingId} className="settings-section">
      <h2 id={headingId}>{SECTION_TITLE}</h2>
      <div className="settings-row" role="radiogroup" aria-labelledby={ids.kind}>
        <span id={ids.kind} className="settings-label">
          Notes for
        </span>
        <div className="settings-field">
          {(["global", "workstream"] as const).map((kind) => (
            <label key={kind} className="settings-choice">
              <input
                type="radio"
                name={`${ids.kind}-kind`}
                value={kind}
                checked={scopeKind === kind}
                onChange={() => {
                  setScopeKind(kind);
                  setWorkstreamId("");
                }}
              />
              {kind === "global" ? "Global" : "One workstream"}
            </label>
          ))}
        </div>
      </div>
      {scopeKind === "workstream" && (
        <div className="settings-row">
          <label htmlFor={ids.select}>Workstream</label>
          <select id={ids.select} value={workstreamId} onChange={(event) => setWorkstreamId(event.target.value)}>
            <option value="">{workstreams.length === 0 ? "No workstreams to choose" : "Choose a workstream"}</option>
            {workstreams.map((item) => (
              <option key={item.id} value={item.id}>
                {item.title}
              </option>
            ))}
          </select>
        </div>
      )}
      {scope === null ? (
        <p className="settings-hint">Choose a workstream to see its notes.</p>
      ) : result.status === "loading" ? (
        <p className="settings-hint" role="status">
          Loading notes.
        </p>
      ) : result.status !== "ok" ? (
        <Notice tone="warn" role="status">
          Notes could not be read. {result.reason}
        </Notice>
      ) : (
        <NotesBody key={result.data.scope} snapshot={result.data} fixture={!live} reload={() => setNonce((value) => value + 1)} />
      )}
    </section>
  );
}
