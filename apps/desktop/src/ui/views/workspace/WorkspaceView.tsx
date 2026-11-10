import { useEffect, useId, useMemo, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { asDaemonError, type DaemonError } from "../../../daemon/errors";
import { isFixtureId } from "../../../daemon/api";
import type { ProjectRow } from "../../../daemon/types";
import type { ConversationEntry, ConversationTranscript, Workstream } from "../../../sources/surfaces/types";
import { useGate, useServices, useStore, useSurface, type SurfaceState } from "../../services";
import { NavLink } from "../../shell/NavLink";
import { Badge, Bubble, Button, EmptyState } from "../../theme";
import { ErrorNotice } from "../../widgets";
import { ConversationTargetPicker } from "./ConversationTargetPicker";
import { ModelPicker } from "./ModelPicker";
import { GLOBAL_TARGET, parseTargetKey, projectLabel, targetKey, type TargetOption } from "./targets";
import "./workspace.css";

const GLOBAL_OPTION: TargetOption = {
  key: "global",
  target: GLOBAL_TARGET,
  label: "Global orchestrator",
  scope: "All projects",
  placeholder: true,
};
const GLOBAL_LIVE_REASON =
  "The daemon has a coordinator for each project and no Global orchestrator, so nothing typed here can be sent.";
const NOTE_COPY =
  "Notes only, no reply: a message is recorded as a note for the coordinator, which does not respond. It does not create tasks or approvals, and it can be neither approved nor undone from here.";
const SIMULATED_COPY = "Nothing typed here is sent or recorded anywhere; a send is only simulated in this window.";
const GLOBAL_NOTE_COPY = "The Global orchestrator is placeholder data; nothing typed here is recorded.";

interface SimulatedEntry {
  id: string;
  text: string;
  at: string;
}

export function WorkspaceView() {
  const { surfaces, drafts, data } = useServices();
  const snapshot = useStore(drafts);
  const projectList = useStore(data).projects;
  const live = surfaces.conversation.provenance.kind === "daemon";
  // Live coordinators are per project, so the registered projects are the targets; a workstream
  // can span projects and omits projects that have no linked task. Only fixtures list workstreams.
  const workstreams = useSurface(
    () => (live ? new Promise<never>(() => {}) : surfaces.workstreams.workstreams()),
    "workspace-targets",
    surfaces.workstreams.provenance,
  );
  const [simulated, setSimulated] = useState<Record<string, SimulatedEntry[]>>({});

  const options = useMemo(() => buildOptions(workstreams, live ? projectList.data : null), [workstreams, live, projectList.data]);
  const stored = snapshot.workspace?.target ?? null;
  const known = stored !== null && options.some((option) => option.key === stored);
  const listLoading = live ? projectList.data === null && projectList.status !== "error" : workstreams.status === "loading";
  const listError = live ? projectList.error?.message ?? null : workstreams.status !== "ok" && workstreams.status !== "loading" ? workstreams.reason : null;
  const resolved = resolveKey(stored, known, options, listLoading, live);

  if (resolved === null) {
    return (
      <div className="ws-root">
        <h1 className="ws-visually-hidden">Orchestrator Workspace</h1>
        <p role="status">Loading conversations…</p>
      </div>
    );
  }

  const option = options.find((item) => item.key === resolved) ?? fallbackOption(resolved);

  return (
    <div className="ws-root">
      <h1 className="ws-visually-hidden">Orchestrator Workspace</h1>
      {listError !== null && (
        <p role="status" className="ws-note">
          Project conversations could not be listed: {listError}
        </p>
      )}
      <Conversation
        option={option}
        options={options}
        live={live}
        draft={drafts.workspaceDraft(resolved)}
        simulated={simulated[resolved] ?? []}
        onSelect={(next) => drafts.setWorkspaceTarget(next.key)}
        onDraft={(text) => drafts.setWorkspaceDraft(resolved, text)}
        onSimulated={(entry) => setSimulated((all) => ({ ...all, [resolved]: [...(all[resolved] ?? []), entry] }))}
      />
    </div>
  );
}

function buildOptions(workstreams: SurfaceState<Workstream[]>, liveProjects: readonly ProjectRow[] | null): TargetOption[] {
  if (liveProjects !== null) {
    return [
      GLOBAL_OPTION,
      ...liveProjects.map<TargetOption>((project) => ({
        key: targetKey({ kind: "project", projectId: project.id }),
        target: { kind: "project", projectId: project.id },
        label: projectLabel(project.id),
        scope: project.id,
        placeholder: false,
      })),
    ];
  }
  const projects: TargetOption[] =
    workstreams.status === "ok"
      ? workstreams.data.map((ws) => ({
          key: targetKey({ kind: "project", projectId: ws.id }),
          target: { kind: "project", projectId: ws.id },
          label: `${ws.title} coordinator`,
          scope: isFixtureId(ws.id) ? "Placeholder workstream" : ws.id,
          placeholder: false,
        }))
      : [];
  return [GLOBAL_OPTION, ...projects];
}

function fallbackOption(key: string): TargetOption {
  const target = parseTargetKey(key) ?? GLOBAL_TARGET;
  return target.kind === "global"
    ? GLOBAL_OPTION
    : { key, target, label: projectLabel(target.projectId), scope: target.projectId, placeholder: false };
}

/**
 * The target to show. A stored choice wins while the list is still loading or once it is
 * confirmed; otherwise the default is Global in fixture mode and the first project live,
 * because the daemon has no Global orchestrator. `null` means "wait for the list".
 */
function resolveKey(
  stored: string | null,
  known: boolean,
  options: readonly TargetOption[],
  loading: boolean,
  live: boolean,
): string | null {
  if (stored !== null && parseTargetKey(stored) !== null && (known || loading)) return stored;
  if (loading && live) return null;
  if (live) return options.find((item) => item.target.kind === "project")?.key ?? "global";
  return "global";
}

interface ConversationProps {
  option: TargetOption;
  options: readonly TargetOption[];
  live: boolean;
  draft: string;
  simulated: readonly SimulatedEntry[];
  onSelect: (option: TargetOption) => void;
  onDraft: (text: string) => void;
  onSimulated: (entry: SimulatedEntry) => void;
}

function Conversation({ option, options, live, draft, simulated, onSelect, onDraft, onSimulated }: ConversationProps) {
  const { surfaces, drafts } = useServices();
  const gate = useGate();
  const ids = useId();
  const [nonce, setNonce] = useState(0);
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<{ key: string; notice: string | null; error: DaemonError | null } | null>(null);
  const notice = feedback?.key === option.key ? feedback.notice : null;
  const error = feedback?.key === option.key ? feedback.error : null;
  const logRef = useRef<HTMLDivElement>(null);
  const { target } = option;

  const transcript = useSurface(
    () => surfaces.conversation.transcript(target),
    `${option.key}#${nonce}`,
    surfaces.conversation.provenance,
  );

  const lines = transcript.status === "ok" ? transcript.data.entries.length + simulated.length : simulated.length;
  useEffect(() => {
    const log = logRef.current;
    if (log) log.scrollTop = log.scrollHeight;
  }, [lines, transcript.status]);

  let blocked: string | null = null;
  if (live) {
    if (target.kind === "global") blocked = GLOBAL_LIVE_REASON;
    else if (isFixtureId(target.projectId)) blocked = "This is a placeholder project; it has no coordinator to send to.";
    else if (!gate.can("coordinator")) blocked = "Sending needs a live daemon connection that supports coordinator messages.";
  }
  const empty = draft.trim() === "";
  const canSend = blocked === null && !empty && !busy;

  const send = async () => {
    if (!canSend) return;
    const key = option.key;
    const sent = draft;
    setBusy(true);
    setFeedback(null);
    try {
      const receipt = await surfaces.conversation.send(target, sent);
      if (receipt.simulated) {
        onSimulated({ id: receipt.entryId, text: sent, at: receipt.at });
        setFeedback({ key, notice: "SIMULATED: this message was not sent or recorded anywhere.", error: null });
      } else {
        setFeedback({ key, notice: receipt.notice, error: null });
        setNonce((value) => value + 1);
      }
      // Keep anything typed while the send was in flight.
      if (drafts.workspaceDraft(key) === sent) onDraft("");
    } catch (thrown) {
      setFeedback({ key, notice: null, error: asDaemonError(thrown) });
    } finally {
      setBusy(false);
    }
  };

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    void send();
  };
  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      void send();
    }
  };

  const headingId = `${ids}-heading`;
  const hintId = `${ids}-hint`;
  const keysId = `${ids}-keys`;
  const noteCopy = target.kind === "global" ? GLOBAL_NOTE_COPY : live ? NOTE_COPY : SIMULATED_COPY;
  const sendId = `${ids}-send-reason`;
  const modelCatalog = useSurface(
    () => surfaces.models.catalog(),
    "workspace-models",
    surfaces.models.provenance,
  );

  return (
    <section className="ws-conversation" aria-labelledby={headingId}>
      <header className="ws-head">
        <h2 id={headingId}>Talking to: {option.label}</h2>
        {(target.kind === "global" || !live) && <Badge tone="placeholder">PLACEHOLDER DATA (D10)</Badge>}
        {target.kind === "project" && <Badge tone="neutral">Notes, no reply</Badge>}
        <p className="ws-scope">{option.scope}</p>
      </header>

      <div
        ref={logRef}
        className="ws-log"
        role="log"
        aria-label={`Conversation with ${option.label}`}
        tabIndex={0}
      >
        <TranscriptBody
          transcript={transcript}
          label={option.label}
          simulated={simulated}
          onRetry={() => setNonce((value) => value + 1)}
        />
      </div>

      <p className="ws-small" id={hintId}>
        {noteCopy}
      </p>
      {target.kind === "project" && (
        <div className="ws-chips">
          <Button variant="quiet" disabled aria-describedby={`${ids}-chips`}>
            Escalate to Global
          </Button>
          <NavLink to={{ name: "global", view: "workstreams" }} className="ws-chip-link">
            Show dependency plan
          </NavLink>
          <Button variant="quiet" disabled aria-describedby={`${ids}-chips`}>
            Request review
          </Button>
          <span id={`${ids}-chips`} className="ws-small">
            Escalate and review requests are not available: the daemon has no operation for them.
          </span>
        </div>
      )}

      <form className="ws-composer" onSubmit={onSubmit} aria-label={`Message ${option.label}`}>
        <label className="ws-visually-hidden" htmlFor={`${ids}-draft`}>
          Message {option.label}
        </label>
        <textarea
          id={`${ids}-draft`}
          rows={3}
          value={draft}
          placeholder={`Message ${option.label}`}
          aria-describedby={`${hintId} ${keysId}${blocked ? ` ${sendId}` : ""}`}
          onChange={(event) => onDraft(event.target.value)}
          onKeyDown={onKeyDown}
        />
        <div className="ws-composer-row">
          <ConversationTargetPicker options={options} selectedKey={option.key} onSelect={onSelect} />
          <ModelPicker catalog={modelCatalog} provenance={surfaces.models.provenance} />
          <Button
            type="submit"
            variant="primary"
            className="ws-send"
            disabled={!canSend}
            aria-label={`Send message to ${option.label}`}
            aria-describedby={blocked ? sendId : undefined}
          >
            {busy ? "…" : "↑"}
          </Button>
        </div>
        <p id={keysId} className="ws-small">
          Press Ctrl or Cmd + Enter to send.
        </p>
        {blocked && (
          <p id={sendId} className="ws-small ws-blocked">
            Sending is disabled. {blocked}
          </p>
        )}
      </form>
      <div aria-live="polite" className="ws-feedback">
        {notice && <p role="status">{notice}</p>}
        {error && (
          <>
            <ErrorNotice error={error} action="Sending the message" />
            {error.indeterminate && (
              <p className="ws-small">
                The daemon may have recorded it. Your draft is kept; sending again reuses the same request key.
              </p>
            )}
          </>
        )}
      </div>
    </section>
  );
}

function TranscriptBody({
  transcript,
  label,
  simulated,
  onRetry,
}: {
  transcript: SurfaceState<ConversationTranscript>;
  label: string;
  simulated: readonly SimulatedEntry[];
  onRetry: () => void;
}) {
  if (transcript.status === "loading") return <p role="status">Loading conversation…</p>;
  const sim = simulated.map((entry) => (
    <Bubble
      key={entry.id}
      from="human"
      author="You"
      time={
        <>
          {formatAt(entry.at)} <Badge tone="placeholder">SIMULATED, not recorded</Badge>
        </>
      }
    >
      {entry.text}
    </Bubble>
  ));
  if (transcript.status === "unavailable") {
    return (
      <>
        <div role="alert" className="ws-state">
          <p>The conversation is unavailable: {transcript.reason}</p>
          <Button onClick={onRetry}>Retry</Button>
        </div>
        {sim}
      </>
    );
  }
  if (transcript.status === "unsupported" || transcript.status === "unknown") {
    return (
      <>
        <p role="status" className="ws-state">
          {transcript.status === "unsupported" ? "Not supported" : "Not known"}: {transcript.reason}
        </p>
        {sim}
      </>
    );
  }
  if (transcript.status !== "ok") return <>{sim}</>;
  const { entries, truncated, repliesExpected } = transcript.data;
  if (entries.length === 0 && sim.length === 0) {
    return (
      <EmptyState
        title="No messages yet"
        description={`Nothing is recorded for ${label}.${repliesExpected ? "" : " The coordinator does not reply."}`}
      />
    );
  }
  return (
    <>
      {truncated && <p className="ws-small">Older messages are not shown: the log was cut off.</p>}
      {entries.map((entry) => (
        <EntryBubble key={entry.id} entry={entry} label={label} />
      ))}
      {sim}
    </>
  );
}

function EntryBubble({ entry, label }: { entry: ConversationEntry; label: string }) {
  const human = entry.role === "user";
  return (
    <Bubble
      from={human ? "human" : "coordinator"}
      author={human ? "You" : label}
      time={
        <>
          {formatAt(entry.at)}
          {entry.kind === "proposal" && (
            <>
              {" "}
              <Badge tone="info">Proposal</Badge>
            </>
          )}
        </>
      }
    >
      {entry.text}
    </Bubble>
  );
}

function formatAt(at: string): string {
  const date = new Date(at);
  return Number.isNaN(date.getTime()) ? at : date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}
