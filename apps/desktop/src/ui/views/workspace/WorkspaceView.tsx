import { useCallback, useEffect, useId, useMemo, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { asDaemonError, type DaemonError } from "../../../daemon/errors";
import { isFixtureId } from "../../../daemon/api";
import type { ConversationEntry, ConversationTranscript, Workstream } from "../../../sources/surfaces/types";
import { useGate, useServices, useStore, useSurface, type SurfaceState } from "../../services";
import { NavLink } from "../../shell/NavLink";
import { Badge, Bubble, Button, EmptyState } from "../../theme";
import { ErrorNotice } from "../../widgets";
import { ConversationTargetPicker } from "./ConversationTargetPicker";
import { ModelPicker } from "./ModelPicker";
import { GLOBAL_TARGET, parseTargetKey, projectLabel, targetId, targetKey, type TargetOption } from "./targets";
import { useRetained } from "./useRetained";
import "./workspace.css";

const globalOption = (live: boolean): TargetOption => ({
  key: "global",
  target: GLOBAL_TARGET,
  label: "Global orchestrator",
  scope: "All projects",
  placeholder: !live,
});
const POLL_MS = 1000;
const LIVE_COPY =
  "Messages are recorded by the daemon. A reply appears only if the daemon's conversation runtime produces one; nothing is simulated here.";
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
  const { surfaces, drafts } = useServices();
  const snapshot = useStore(drafts);
  const live = surfaces.conversation.provenance.kind === "daemon";
  const workstreams = useSurface(
    () => surfaces.workstreams.workstreams(),
    "workspace-targets",
    surfaces.workstreams.provenance,
  );
  const [simulated, setSimulated] = useState<Record<string, SimulatedEntry[]>>({});

  const options = useMemo(() => buildOptions(workstreams, live), [workstreams, live]);
  const stored = snapshot.workspace?.target ?? null;
  const known = stored !== null && options.some((option) => option.key === stored);
  const resolved = resolveKey(stored, known, options, workstreams.status === "loading", live);

  if (resolved === null) {
    return (
      <div className="ws-root">
        <h1 className="ws-visually-hidden">Orchestrator Workspace</h1>
        <p role="status">Loading conversations…</p>
      </div>
    );
  }

  const option = options.find((item) => item.key === resolved) ?? fallbackOption(resolved, live);

  return (
    <div className="ws-root">
      <h1 className="ws-visually-hidden">Orchestrator Workspace</h1>
      {workstreams.status !== "ok" && workstreams.status !== "loading" && (
        <p role="status" className="ws-note">
          Project conversations could not be listed: {workstreams.reason}
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

function buildOptions(workstreams: SurfaceState<Workstream[]>, live: boolean): TargetOption[] {
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
  // A workstream target exists only when D9 supplies the daemon's workstream id (M7-07 lookup).
  const daemonWorkstreams: TargetOption[] =
    live && workstreams.status === "ok"
      ? workstreams.data.flatMap((ws) =>
          ws.workstreamId && !isFixtureId(ws.workstreamId)
            ? [
                {
                  key: targetKey({ kind: "workstream", workstreamId: ws.workstreamId }),
                  target: { kind: "workstream" as const, workstreamId: ws.workstreamId },
                  label: `${ws.title} orchestrator`,
                  scope: ws.workstreamId,
                  placeholder: false,
                },
              ]
            : [],
        )
      : [];
  return [globalOption(live), ...daemonWorkstreams, ...projects];
}

function fallbackOption(key: string, live: boolean): TargetOption {
  const target = parseTargetKey(key) ?? GLOBAL_TARGET;
  if (target.kind === "global") return globalOption(live);
  const id = targetId(target) ?? key;
  return {
    key,
    target,
    label: target.kind === "project" ? projectLabel(id) : `${id} orchestrator`,
    scope: id,
    placeholder: false,
  };
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
  const [modelNonce, setModelNonce] = useState(0);
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<{
    key: string;
    notice: string | null;
    error: DaemonError | null;
    action?: string;
  } | null>(null);
  const notice = feedback?.key === option.key ? feedback.notice : null;
  const error = feedback?.key === option.key ? feedback.error : null;
  const logRef = useRef<HTMLDivElement>(null);
  const { target } = option;

  const loaded = useSurface(
    () => surfaces.conversation.transcript(target),
    `${option.key}#${nonce}`,
    surfaces.conversation.provenance,
  );
  const transcript = useRetained(loaded, option.key);

  const lines = transcript.status === "ok" ? transcript.data.entries.length + simulated.length : simulated.length;
  useEffect(() => {
    const log = logRef.current;
    if (log) log.scrollTop = log.scrollHeight;
  }, [lines, transcript.status]);

  // The daemon has no push stream: read again while the latest turn is still running.
  const turn = transcript.status === "ok" ? (transcript.data.turn ?? null) : null;
  const running = live && turn?.state === "running";
  useEffect(() => {
    if (!running) return;
    const timer = setTimeout(() => setNonce((value) => value + 1), POLL_MS);
    return () => clearTimeout(timer);
  }, [running, nonce]);

  let blocked: string | null = null;
  if (live) {
    const id = targetId(target);
    if (id !== null && isFixtureId(id)) blocked = "This is a placeholder target; it has nothing to send to.";
    else if (!gate.can("coordinator")) blocked = "Sending needs a live daemon connection that supports conversations.";
    else if (running) blocked = "The orchestrator is still working on your last message. Wait for it to finish or cancel it.";
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

  const cancel = async () => {
    if (!turn || !surfaces.conversation.cancel) return;
    const key = option.key;
    try {
      const result = await surfaces.conversation.cancel(target, turn.id);
      setFeedback({
        key,
        notice: result.applied ? "Cancelled the reply." : "The reply had already finished; nothing was cancelled.",
        error: null,
      });
      setNonce((value) => value + 1);
    } catch (thrown) {
      setFeedback({ key, notice: null, error: asDaemonError(thrown), action: "Cancelling the reply" });
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
  const noteCopy = !live
    ? target.kind === "global"
      ? GLOBAL_NOTE_COPY
      : SIMULATED_COPY
    : target.kind === "project"
      ? NOTE_COPY
      : LIVE_COPY;
  const sendId = `${ids}-send-reason`;
  const modelLoaded = useSurface(
    () => surfaces.models.catalog(),
    `workspace-models#${modelNonce}`,
    surfaces.models.provenance,
  );
  const modelCatalog = useRetained(modelLoaded, "workspace-models");
  const retryModels = useCallback(() => setModelNonce((value) => value + 1), []);
  const runtimeUnavailable = live && turn?.state === "failed" && turn.errorCode === "runtime_unavailable";

  return (
    <section className="ws-conversation" aria-labelledby={headingId}>
      <header className="ws-head">
        <h2 id={headingId}>Talking to: {option.label}</h2>
        {!live && <Badge tone="placeholder">PLACEHOLDER DATA (D10)</Badge>}
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

      {runtimeUnavailable && turn && (
        <p role="status" className="ws-note ws-runtime">
          Orchestrator runtime unavailable ({turn.runtime.name} {turn.runtime.version}): the daemon recorded your
          message but cannot produce a reply right now.
        </p>
      )}
      {running && (
        <p role="status" className="ws-note ws-runtime">
          Waiting for the orchestrator&apos;s reply…{" "}
          {surfaces.conversation.cancel && (
            <Button variant="quiet" onClick={() => void cancel()}>
              Cancel reply
            </Button>
          )}
        </p>
      )}

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
          <ModelPicker
            catalog={modelCatalog}
            provenance={surfaces.models.provenance}
            validate={surfaces.models.validate}
            onRetry={retryModels}
          />
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
            <ErrorNotice error={error} action={feedback?.action ?? "Sending the message"} />
            {error.indeterminate && !feedback?.action && (
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
  // A daemon turn outcome is a system line, never a reply from the orchestrator.
  if (entry.kind === "status") {
    return (
      <p role="status" className="ws-state ws-turn-status">
        {entry.text}
      </p>
    );
  }
  return (
    <Bubble
      from={human ? "human" : "coordinator"}
      author={human ? "You" : label}
      time={
        <>
          {formatAt(entry.at)}
          {entry.streaming && (
            <>
              {" "}
              <Badge tone="info">Streaming</Badge>
            </>
          )}
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
