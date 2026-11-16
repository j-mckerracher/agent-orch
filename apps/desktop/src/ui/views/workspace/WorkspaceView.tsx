import { useCallback, useEffect, useId, useMemo, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { asDaemonError, type DaemonError } from "../../../daemon/errors";
import { isFixtureId } from "../../../daemon/api";
import type { ProjectRow } from "../../../daemon/types";
import type {
  ConversationEntry,
  ConversationTranscript,
  ModelChoice,
  PermissionLevel,
  Workstream,
} from "../../../sources/surfaces/types";
import { useGate, useServices, useStore, useSurface, type SurfaceState } from "../../services";
import { NavLink } from "../../shell/NavLink";
import { formatAge } from "../../shell/ResourceStrip";
import { Badge, Bubble, Button, EmptyState } from "../../theme";
import { ErrorNotice } from "../../widgets";
import { AddCoordinatorDialog, ConversationActions, historyActionReason } from "./ConversationActions";
import { ConversationTargetPicker } from "./ConversationTargetPicker";
import { ModelPicker } from "./ModelPicker";
import { PermissionLevelNotes, PermissionLevelPicker } from "./PermissionLevelPicker";
import { permissionForSend, readSavedLevel, saveLevel } from "./permissionLevel";
import { GLOBAL_TARGET, parseTargetKey, projectLabel, targetId, targetKey, type TargetOption } from "./targets";
import { formatAt } from "./time";
import { useRetained } from "./useRetained";
import "./workspace.css";

const globalOption = (live: boolean): TargetOption => ({
  key: "global",
  target: GLOBAL_TARGET,
  label: "Global orchestrator",
  scope: "All workstreams · Cross-workstream coordination",
  placeholder: !live,
});
const POLL_MS = 1000;
const COMPOSER_PLACEHOLDER = "Ask a question, redirect work, or discuss the plan…";

interface SimulatedEntry {
  id: string;
  text: string;
  at: string;
}

export function WorkspaceView() {
  const { surfaces, drafts, data } = useServices();
  const snapshot = useStore(drafts);
  const projectList = useStore(data).projects;
  const gate = useGate();
  const live = surfaces.conversation.provenance.kind === "daemon";
  // Live targets are the daemon's workstreams plus the registered projects (project coordinators take
  // notes only); fixtures derive project targets from the placeholder workstreams.
  const workstreams = useSurface(
    () => surfaces.workstreams.workstreams(),
    "workspace-targets",
    surfaces.workstreams.provenance,
  );
  const [simulated, setSimulated] = useState<Record<string, SimulatedEntry[]>>({});
  // Coordinators removed in this session's fixture mode (DPES-1105). Live state comes from the daemon.
  const [fixtureRemoved, setFixtureRemoved] = useState<readonly string[]>([]);
  const [addOpen, setAddOpen] = useState(false);
  const [addBusy, setAddBusy] = useState<string | null>(null);
  const [actionNote, setActionNote] = useState<{ notice: string | null; error: DaemonError | null; action: string } | null>(
    null,
  );
  const removedIds = useMemo(
    () =>
      new Set(
        live
          ? (projectList.data ?? []).filter((project) => project.coordinator_active === false).map((project) => project.id)
          : fixtureRemoved,
      ),
    [live, projectList.data, fixtureRemoved],
  );
  const options = useMemo(
    () => buildOptions(workstreams, live, live ? projectList.data : null, removedIds),
    [workstreams, live, projectList.data, removedIds],
  );
  const addable = useMemo(
    () => Array.from(removedIds).map((id) => ({ id, label: projectLabel(id) })),
    [removedIds],
  );
  const addReason = historyActionReason({
    live,
    connected: gate.connected,
    supported: gate.can("conversation_history"),
    running: false,
    busy: addBusy !== null,
  });

  const removeCoordinator = async (projectId: string) => {
    setActionNote(null);
    try {
      const receipt = await surfaces.conversation.removeCoordinator!(projectId);
      if (live) void data.refreshAll();
      else setFixtureRemoved((ids) => [...ids, projectId]);
      // The removed coordinator's unsent draft has no target any more; discard it. Switch to Global.
      drafts.setWorkspaceDraft(targetKey({ kind: "project", projectId }), "");
      drafts.setWorkspaceTarget("global");
      setActionNote({ notice: receipt.notice, error: null, action: "" });
    } catch (thrown) {
      setActionNote({ notice: null, error: asDaemonError(thrown), action: "Removing the coordinator" });
    }
  };

  const addCoordinator = async (projectId: string) => {
    setAddBusy(projectId);
    setActionNote(null);
    try {
      const receipt = await surfaces.conversation.restoreCoordinator!(projectId);
      if (live) void data.refreshAll();
      else setFixtureRemoved((ids) => ids.filter((id) => id !== projectId));
      drafts.setWorkspaceTarget(targetKey({ kind: "project", projectId }));
      setAddOpen(false);
      setActionNote({ notice: receipt.notice, error: null, action: "" });
    } catch (thrown) {
      setActionNote({ notice: null, error: asDaemonError(thrown), action: "Adding the coordinator" });
    } finally {
      setAddBusy(null);
    }
  };

  const coordinatorControls = Boolean(surfaces.conversation.removeCoordinator && surfaces.conversation.restoreCoordinator);
  const stored = snapshot.workspace?.target ?? null;
  const known = stored !== null && options.some((option) => option.key === stored);
  const workstreamsLoading = workstreams.status === "loading";
  // Unread projects are loading only while a daemon answer can still arrive; disconnected, they are unavailable.
  const projectsPending = projectList.data === null && projectList.status !== "error" && (gate.connected || gate.settling);
  const listLoading = live ? workstreamsLoading || projectsPending : workstreamsLoading;
  const workstreamsFailure = workstreams.status !== "ok" && workstreams.status !== "loading" ? workstreams : null;
  const workstreamsReason = workstreamsFailure?.reason ?? null;
  const listError = live ? (projectList.error?.message ?? workstreamsReason) : workstreamsReason;
  const listErrorObservation = projectList.error ? null : workstreamsFailure;
  const resolved = resolveKey(stored, known, options, listLoading, live);

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
      {listError !== null && (
        <p role="status" className="ws-note">
          Project conversations could not be listed: {listError}
          {listErrorObservation && (
            <>
              {" "}· observed <time dateTime={listErrorObservation.observedAt}>{formatAge(listErrorObservation.observedAt, listErrorObservation.ageMs, Date.now())}</time>
            </>
          )}
        </p>
      )}
      {actionNote?.error && <ErrorNotice error={actionNote.error} action={actionNote.action} />}
      {actionNote?.notice && <p role="status">{actionNote.notice}</p>}
      <Conversation
        option={option}
        options={options}
        live={live}
        draft={drafts.workspaceDraft(resolved)}
        simulated={simulated[resolved] ?? []}
        onSelect={(next) => drafts.setWorkspaceTarget(next.key)}
        onDraft={(text) => drafts.setWorkspaceDraft(resolved, text)}
        onSimulated={(entry) => setSimulated((all) => ({ ...all, [resolved]: [...(all[resolved] ?? []), entry] }))}
        onSimulatedCleared={() =>
          setSimulated((all) => {
            const { [resolved]: _cleared, ...rest } = all;
            return rest;
          })
        }
        onRemoveCoordinator={coordinatorControls ? (projectId) => void removeCoordinator(projectId) : undefined}
        addCoordinator={coordinatorControls ? { onAdd: () => setAddOpen(true), reason: addReason } : undefined}
      />
      {addOpen && (
        <AddCoordinatorDialog
          projects={addable}
          busyId={addBusy}
          onAdd={(projectId) => void addCoordinator(projectId)}
          onClose={() => setAddOpen(false)}
        />
      )}
    </div>
  );
}

function buildOptions(
  workstreams: SurfaceState<Workstream[]>,
  live: boolean,
  liveProjects: readonly ProjectRow[] | null,
  removedIds: ReadonlySet<string>,
): TargetOption[] {
  const global = globalOption(live);
  // A live workstream's `id` is the daemon's workstream id; fixture ids never become targets.
  const daemonWorkstreams: TargetOption[] =
    live && workstreams.status === "ok"
      ? workstreams.data.flatMap((ws) => {
          const id = ws.workstreamId ?? ws.id;
          return isFixtureId(id)
            ? []
            : [
                {
                  key: targetKey({ kind: "workstream", workstreamId: id }),
                  target: { kind: "workstream" as const, workstreamId: id },
                  label: `${ws.title} orchestrator`,
                  scope: id,
                  placeholder: false,
                },
              ];
        })
      : [];
  if (live) {
    // A removed coordinator is not a target until it is added back (DPES-1105).
    const projects = (liveProjects ?? []).filter((project) => !removedIds.has(project.id)).map<TargetOption>((project) => ({
      key: targetKey({ kind: "project", projectId: project.id }),
      target: { kind: "project", projectId: project.id },
      label: projectLabel(project.id),
      scope: project.id,
      placeholder: false,
    }));
    return [global, ...daemonWorkstreams, ...projects];
  }
  const projects: TargetOption[] =
    workstreams.status === "ok"
      ? workstreams.data.filter((ws) => !removedIds.has(ws.id)).map((ws) => ({
          key: targetKey({ kind: "project", projectId: ws.id }),
          target: { kind: "project", projectId: ws.id },
          label: `${ws.title} coordinator`,
          scope: isFixtureId(ws.id) ? "Placeholder workstream" : ws.id,
          placeholder: false,
        }))
      : [];
  return [global, ...projects];
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
  /** A clear emptied this conversation: drop what the fixture window had simulated for it. */
  onSimulatedCleared: () => void;
  /** Project coordinators only; absent when the surface cannot remove coordinators. */
  onRemoveCoordinator?: (projectId: string) => void;
  addCoordinator?: { onAdd: () => void; reason: string | null };
}

function Conversation({
  option,
  options,
  live,
  draft,
  simulated,
  onSelect,
  onDraft,
  onSimulated,
  onSimulatedCleared,
  onRemoveCoordinator,
  addCoordinator,
}: ConversationProps) {
  const { surfaces, drafts } = useServices();
  const gate = useGate();
  const ids = useId();
  const [nonce, setNonce] = useState(0);
  const [modelNonce, setModelNonce] = useState(0);
  const [busy, setBusy] = useState(false);
  // The model choice each message is sent with; null means the daemon default (DPES-1105).
  const [choice, setChoice] = useState<ModelChoice | null>(null);
  // The level each message runs at (DPES-1105). Only a saved Restricted survives a reload.
  const [permission, setPermission] = useState<PermissionLevel>(() => readSavedLevel());
  // The level each turn was sent at, by turn id, so a reply can be checked against what the daemon reports.
  const sentLevels = useRef(new Map<string, PermissionLevel>());
  const [feedback, setFeedback] = useState<{
    key: string;
    notice: string | null;
    error: DaemonError | null;
    turnId?: string;
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
    if (turn?.state === "running") return;
    setFeedback((current) => {
      if (
        current?.key !== option.key ||
        current.turnId !== turn?.id ||
        current.notice !== "Recorded by the daemon. Waiting for the orchestrator's reply."
      ) {
        return current;
      }
      // The transcript carries the terminal reply, failure, or cancellation state.
      return { ...current, notice: null };
    });
  }, [option.key, turn?.id, turn?.state]);
  useEffect(() => {
    if (!running) return;
    const timer = setTimeout(() => setNonce((value) => value + 1), POLL_MS);
    return () => clearTimeout(timer);
  }, [running, nonce]);

  // An older daemon has no permission levels and runs chats read-only, so the level reads Restricted (DPES-1105).
  const olderDaemon = live && gate.connected && !gate.can("permission_levels");
  const shownLevel: PermissionLevel = olderDaemon ? "restricted" : permission;
  // A coordinator note runs no agent, so it carries no level (the daemon refuses one there).
  const permissionOnWire = live && target.kind !== "project" ? permissionForSend(permission, !olderDaemon) : undefined;
  const choosePermission = (next: PermissionLevel) => {
    saveLevel(next);
    setPermission(next);
  };

  let blocked: string | null = null;
  if (live) {
    const id = targetId(target);
    if (id !== null && isFixtureId(id)) blocked = "This is a placeholder target; it has nothing to send to.";
    else if (!gate.can("coordinator")) blocked = "Sending needs a live daemon connection that supports conversations.";
    // A coordinator note takes no choice, and an older daemon would drop it: either way the message is not sent (DPES-1105).
    else if (choice !== null && target.kind === "project")
      blocked = "Coordinator notes take no model or effort. Choose the default in model settings to send one.";
    else if (choice !== null && !gate.can("conversation_selection"))
      blocked = "This daemon cannot use a chosen model or effort. Choose the default in model settings, or update the daemon.";
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
      const receipt = await surfaces.conversation.send(target, sent, choice ?? undefined, permissionOnWire);
      if (receipt.simulated) {
        onSimulated({ id: receipt.entryId, text: sent, at: receipt.at });
        setFeedback({ key, notice: "SIMULATED: this message was not sent or recorded anywhere.", error: null });
      } else {
        setFeedback({ key, notice: receipt.notice, error: null, ...(receipt.turn ? { turnId: receipt.turn.id } : {}) });
        if (receipt.turn && permissionOnWire) sentLevels.current.set(receipt.turn.id, permissionOnWire);
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

  // Clears this conversation's history (DPES-1105). Drafts are kept: they are unsent text, not history.
  const clearHistory = async () => {
    if (!surfaces.conversation.clear) return;
    const key = option.key;
    setBusy(true);
    setFeedback(null);
    try {
      const receipt = await surfaces.conversation.clear(target);
      onSimulatedCleared();
      setFeedback({ key, notice: receipt.notice, error: null });
      setNonce((value) => value + 1);
    } catch (thrown) {
      setFeedback({ key, notice: null, error: asDaemonError(thrown), action: "Clearing the history" });
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
        notice: result.applied ? "Canceled the reply." : "The reply had already finished; nothing was canceled.",
        error: null,
      });
      setNonce((value) => value + 1);
    } catch (thrown) {
      setFeedback({ key, notice: null, error: asDaemonError(thrown), action: "Canceling the reply" });
    }
  };

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    if (document.activeElement instanceof HTMLElement && document.activeElement.closest(".ws-model-root")) return;
    void send();
  };
  // Enter sends. Shift+Enter keeps the default newline; Ctrl/Cmd+Enter also sends. Enter that confirms an IME
  // composition is left to the IME. A send that is not allowed does nothing, and the newline is still
  // suppressed, so Enter never adds a line the user did not ask for.
  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key !== "Enter" || event.nativeEvent.isComposing || event.keyCode === 229) return;
    if (event.shiftKey && !event.ctrlKey && !event.metaKey) return;
    event.preventDefault();
    void send();
  };

  const headingId = `${ids}-heading`;
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
        {target.kind !== "workstream" && (
          <ConversationActions
            option={option}
            live={live}
            connected={gate.connected}
            supported={gate.can("conversation_history")}
            running={running}
            busy={busy}
            onClear={() => void clearHistory()}
            onRemove={
              target.kind === "project" && onRemoveCoordinator
                ? () => onRemoveCoordinator(target.projectId)
                : undefined
            }
          />
        )}
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

      {target.kind === "project" && (
        <div className="ws-chips">
          <Button variant="quiet" disabled aria-describedby={`${ids}-chips`}>
            Escalate to Global
          </Button>
          <NavLink to={{ name: "global", view: "workstreams" }} className="ws-chip-link">
            Show dependency plan
          </NavLink>
          <NavLink to={{ name: "global", view: "review" }} className="ws-chip-link">
            Request review
          </NavLink>
          <span id={`${ids}-chips`} className="ws-small">
            Escalate to Global is not available: the daemon has no operation for it.
          </span>
        </div>
      )}

      <form className="ws-composer" onSubmit={onSubmit} aria-label={`Message ${option.label}`}>
        <textarea
          id={`${ids}-draft`}
          aria-label={`Message ${option.label}`}
          rows={2}
          value={draft}
          placeholder={COMPOSER_PLACEHOLDER}
          aria-describedby={blocked ? sendId : undefined}
          onChange={(event) => onDraft(event.target.value)}
          onKeyDown={onKeyDown}
        />
        <div className="ws-composer-row">
          <ConversationTargetPicker
            options={options}
            selectedKey={option.key}
            onSelect={onSelect}
            addCoordinator={addCoordinator}
          />
          {target.kind !== "project" && (
            <PermissionLevelPicker
              level={shownLevel}
              daemonAdvertises={!olderDaemon}
              agent={choice?.agent}
              onChange={choosePermission}
            />
          )}
          <ModelPicker
            catalog={modelCatalog}
            provenance={surfaces.models.provenance}
            validate={surfaces.models.validate}
            copilotSignIn={surfaces.models}
            onRetry={retryModels}
            onChoiceChange={setChoice}
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
        {target.kind !== "project" && (
          <PermissionLevelNotes
            level={shownLevel}
            olderDaemon={olderDaemon}
            agent={choice?.agent}
            lastTurn={
              live && turn
                ? { daemonLevel: turn.permissionLevel ?? null, sentLevel: sentLevels.current.get(turn.id) ?? null }
                : null
            }
            onChange={choosePermission}
          />
        )}
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
          <time dateTime={entry.at}>{formatAt(entry.at)}</time> <Badge tone="placeholder">SIMULATED, not recorded</Badge>
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
          <p>
            The conversation is unavailable: {transcript.reason} · observed{" "}
            <time dateTime={transcript.observedAt}>{formatAge(transcript.observedAt, transcript.ageMs, Date.now())}</time>
          </p>
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
          {transcript.status === "unsupported" ? "Not supported" : "Not known"}: {transcript.reason} · observed{" "}
          <time dateTime={transcript.observedAt}>{formatAge(transcript.observedAt, transcript.ageMs, Date.now())}</time>
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
          <time dateTime={entry.at}>{formatAt(entry.at)}</time>
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
