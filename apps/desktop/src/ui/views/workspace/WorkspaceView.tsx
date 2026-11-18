import { Fragment, useCallback, useEffect, useId, useMemo, useRef, useState, type FormEvent, type KeyboardEvent, type ReactNode } from "react";
import { asDaemonError, type DaemonError } from "../../../daemon/errors";
import { isFixtureId } from "../../../daemon/api";
import { PROJECT_CONVERSATIONS_REQUIRED } from "../../../sources/surfaces/conversationLive";
import type { ProjectRow } from "../../../daemon/types";
import type {
  ActionBlock,
  ActionProposal,
  ConversationEntry,
  ConversationTranscript,
  ModelChoice,
  PermissionLevel,
  Workstream,
} from "../../../sources/surfaces/types";
import type { Route } from "../../router";
import { useGate, useServices, useStore, useSurface, type SurfaceState } from "../../services";
import { NavLink } from "../../shell/NavLink";
import { formatAge } from "../../shell/ResourceStrip";
import { Badge, Bubble, Button, CopyButton, EmptyState, Markdown } from "../../theme";
import { ErrorNotice, Notice } from "../../widgets";
import { useWorkstreamsVersion } from "../workstreams/model";
import { ActionCard } from "./ActionCard";
import { AddCoordinatorDialog, ConversationActions, historyActionReason } from "./ConversationActions";
import { ConversationTargetPicker } from "./ConversationTargetPicker";
import { MonitoringChip } from "./MonitoringChip";
import { ModelPicker } from "./ModelPicker";
import { NEW_UPDATE_TEXT, WAKE_RUNNING_TEXT, notifyingCount, placeWake, quietTurnIds, unseenFor } from "./monitoringModel";
import { PermissionLevelNotes } from "./PermissionLevelPicker";
import { ReturnBrief } from "./ReturnBrief";
import { RoutedInStrip } from "./RoutedInStrip";
import { newerRecord } from "./actionCardModel";
import { permissionForSend, readSavedLevel, saveLevel } from "./permissionLevel";
import { GLOBAL_TARGET, parseTargetKey, projectLabel, targetId, targetKey, type TargetOption } from "./targets";
import { formatAt } from "./time";
import { useActionDecisions } from "./useActionDecisions";
import { useMonitoring, type MonitoringModel } from "./useMonitoring";
import { useRetained } from "./useRetained";
import { WakeLabel, WakeLines, WakeRecords, type WakeRuntime } from "./WakeNotice";
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
  const { projects: projectList, tasks: taskList } = useStore(data);
  const gate = useGate();
  const monitoring = useMonitoring();
  const live = surfaces.conversation.provenance.kind === "daemon";
  // Live targets are workstreams and projects with an explicitly added coordinator;
  // fixtures derive project targets from the placeholder workstreams.
  // A workstream created on the Workstreams view becomes a target here as soon as it lands (DPES-1105).
  const workstreamsVersion = useWorkstreamsVersion();
  const workstreams = useSurface(
    () => surfaces.workstreams.workstreams(),
    `workspace-targets#${workstreamsVersion}`,
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
  // Workstream titles by daemon id, so an approval card can name the workstream its action changes (DPES-1105).
  const workstreamNames = useMemo(() => {
    const names = new Map<string, string>();
    if (workstreams.status === "ok") for (const ws of workstreams.data) names.set(ws.workstreamId ?? ws.id, ws.title);
    return names;
  }, [workstreams]);
  // A proposal names a task by id only; its page needs the task's project, which the tasks list holds (DPES-1105).
  const taskRoute = useCallback(
    (taskId: string): Route | null => {
      const task = (taskList.data ?? []).find((item) => item.contract.id === taskId);
      return task ? { name: "task", projectId: task.contract.project_id, taskId } : null;
    },
    [taskList.data],
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
        workstreamNames={workstreamNames}
        taskRoute={taskRoute}
        live={live}
        monitoring={monitoring}
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
 * to open the registered project's coordinator. `null` means "wait for the list".
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

const ACTIONS_LEVEL_NOTE =
  "Action proposals work only at Restricted and Auto. At Full the orchestrator can answer but cannot propose changes.";

interface ConversationProps {
  option: TargetOption;
  options: readonly TargetOption[];
  workstreamNames: ReadonlyMap<string, string>;
  taskRoute: (taskId: string) => Route | null;
  live: boolean;
  /** Monitoring mode and counts, read by the Workspace view (DPES-1105 Phase 3). */
  monitoring: MonitoringModel;
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
  workstreamNames,
  taskRoute,
  live,
  monitoring,
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
  const [choiceBlocked, setChoiceBlocked] = useState<string | null>(null);
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

  // Approval cards: live ones need the capability and the approver role; fixture ones always show (DPES-1105).
  const actionsShown = live ? gate.connection.capabilities.includes("orchestrator_actions") : true;
  const canDecide = !live || (gate.can("orchestrator_actions") && gate.connection.approver);
  const decisions = useActionDecisions({
    scope: option.key,
    resolve: (proposalId, decision, digest) => surfaces.conversation.resolve(proposalId, decision, digest),
    announce: (text) => setFeedback({ key: option.key, notice: text, error: null }),
    reload: () => setNonce((value) => value + 1),
  });
  const listed = actionsShown && transcript.status === "ok" ? (transcript.data.proposals ?? []) : [];
  const shownProposals = listed.map((p) => newerRecord(p, decisions.confirmed.get(p.id)));
  // An approved action runs in the daemon: keep reading until it reports the outcome.
  const awaitingDaemon = shownProposals.some((p) => p.state === "approved");

  const lines = transcript.status === "ok" ? transcript.data.entries.length + simulated.length : simulated.length;
  useEffect(() => {
    const log = logRef.current;
    if (log) log.scrollTop = log.scrollHeight;
  }, [lines, transcript.status]);

  // The daemon has no push stream: read again while the latest turn is still running.
  const turn = transcript.status === "ok" ? (transcript.data.turn ?? null) : null;
  const turnStatusInTranscript =
    turn !== null &&
    transcript.status === "ok" &&
    transcript.data.entries.some((entry) => entry.turnId === turn.id && entry.kind === "status");
  const running = live && turn?.state === "running";
  // A wake turn is the orchestrator's own: sending is allowed and the daemon cancels it first (DPES-1105 Phase 3).
  const wakeRunning = running && turn?.trigger === "wake";
  const userRunning = running && !wakeRunning;
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
    if (!running && !awaitingDaemon) return;
    const timer = setTimeout(() => setNonce((value) => value + 1), POLL_MS);
    return () => clearTimeout(timer);
  }, [running, awaitingDaemon, nonce]);

  // Mark this conversation's updates seen once it is on screen and rendered (debounced 1 s), then read the counts again.
  const unseenHere = target.kind !== "project" && monitoring.supported ? unseenFor(monitoring.state, target) : 0;
  const transcriptShown = transcript.status === "ok";
  const ackedMark = useRef<string | null>(null);
  const { ack } = monitoring;
  useEffect(() => {
    if (unseenHere === 0) ackedMark.current = null;
    if (unseenHere === 0 || !transcriptShown || target.kind === "project") return;
    const mark = `${option.key}:${unseenHere}`;
    if (ackedMark.current === mark) return;
    const timer = setTimeout(() => {
      ackedMark.current = mark;
      ack(target).catch(() => undefined);
    }, 1_000);
    return () => clearTimeout(timer);
    // `target` is derived from `option.key`; re-running on the key is the contract.
  }, [option.key, unseenHere, transcriptShown, ack]);

  // The daemon pushes nothing: a rise in this conversation's unseen count means a record arrived, so read it now.
  const lastUnseen = useRef(unseenHere);
  useEffect(() => {
    if (unseenHere > lastUnseen.current) setNonce((value) => value + 1);
    lastUnseen.current = unseenHere;
  }, [unseenHere]);

  // Announce a new notifying update, never on first load and never for quiet ones.
  const notifying = transcript.status === "ok" ? notifyingCount(transcript.data.wake ?? []) : null;
  const seenNotifying = useRef<{ key: string; count: number } | null>(null);
  const [announced, setAnnounced] = useState(0);
  useEffect(() => {
    if (notifying === null) return;
    const before = seenNotifying.current;
    if (before?.key === option.key && notifying > before.count) setAnnounced((value) => value + 1);
    seenNotifying.current = { key: option.key, count: notifying };
  }, [notifying, option.key]);

  // An older daemon has no permission levels and runs chats read-only, so the level reads Restricted (DPES-1105).
  const olderDaemon = live && gate.connected && !gate.can("permission_levels");
  const shownLevel: PermissionLevel = olderDaemon ? "restricted" : permission;
  // Action proposals run at Restricted and Auto: at Full the daemon gives the agent no block format (DPES-1105).
  const actionsLevelNote = live && target.kind !== "project" && actionsShown && shownLevel === "full";
  const permissionOnWire = live ? permissionForSend(permission, !olderDaemon) : undefined;
  const choiceOnWire = choice ?? undefined;
  const choosePermission = (next: PermissionLevel) => {
    saveLevel(next);
    setPermission(next);
  };
  const permissionControl = { level: shownLevel, daemonAdvertises: !olderDaemon, onChange: choosePermission };

  let blocked: string | null = null;
  if (live) {
    const id = targetId(target);
    if (id !== null && isFixtureId(id)) blocked = "This is a placeholder target; it has nothing to send to.";
    else if (!gate.can("coordinator")) blocked = "Sending needs a live daemon connection that supports conversations.";
    else if (target.kind === "project" && !gate.can("project_conversations")) blocked = PROJECT_CONVERSATIONS_REQUIRED;
    else if (choiceOnWire !== undefined && !gate.can("conversation_selection"))
      blocked =
        'This daemon cannot use a chosen model or effort. Open model settings and choose "Use the daemon default", or update the daemon.';
    else if (choiceBlocked !== null) blocked = `Your agent choice cannot be used. ${choiceBlocked}`;
    else if (userRunning) blocked = "The orchestrator is still working on your last message. Wait for it to finish or cancel it.";
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
      const receipt = await surfaces.conversation.send(target, sent, choiceOnWire, permissionOnWire);
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
  const runtimeUnavailable =
    live && turn?.state === "failed" && turn.errorCode === "runtime_unavailable" && !turnStatusInTranscript;

  const parentOf = (p: ActionProposal): ActionProposal | null =>
    shownProposals.find((other) => other.id === p.dependsOn) ?? null;
  const cardOf = (p: ActionProposal) => {
    const parent = parentOf(p);
    return (
      <ActionCard
        key={p.id}
        proposal={p}
        parent={parent}
        names={workstreamNames}
        scope={target.kind === "workstream" ? target.workstreamId : null}
        taskRoute={taskRoute}
        live={live}
        canDecide={canDecide}
        connected={!live || gate.connected}
        approver={!live || gate.connection.approver}
        deciding={decisions.inFlight.has(p.id)}
        notice={decisions.notices.get(p.id) ?? null}
        error={decisions.errors.get(p.id) ?? null}
        onDecide={(decision) => void decisions.decide(p, decision, parent, workstreamNames)}
      />
    );
  };
  const loadedTurns = new Set<string>(
    transcript.status === "ok" ? transcript.data.entries.flatMap((entry) => (entry.turnId ? [entry.turnId] : [])) : [],
  );
  // A proposal whose reply is outside the loaded window waits above the transcript (DPES-1105). One decided here
  // stays while the user is looking at it, so its result and focus do not disappear with the pending state.
  const waiting = shownProposals.filter(
    (p) =>
      !loadedTurns.has(p.turnId) &&
      (p.state === "pending" || decisions.confirmed.has(p.id) || decisions.inFlight.has(p.id)),
  );
  const leading: ReactNode =
    waiting.length > 0 ? (
      <section className="ws-action-group" aria-label="Waiting for your decision">
        <h3>Waiting for your decision</h3>
        {waiting.map(cardOf)}
      </section>
    ) : null;
  const blockOf = (turnId: string): ActionBlock | null => {
    if (transcript.status !== "ok") return null;
    const { actionBlocks, turn: latest } = transcript.data;
    return actionBlocks?.[turnId] ?? (latest?.id === turnId ? (latest.actionBlock ?? null) : null);
  };
  // The cards of one turn follow its last entry. A reply the daemon could not read says so, once, above them.
  const afterTurn = (turnId: string): ReactNode => {
    const cards = shownProposals
      .filter((p) => p.turnId === turnId)
      .sort((a, b) => a.index - b.index)
      .map(cardOf);
    return (
      <>
        {blockOf(turnId) === "rejected" && (
          <Notice tone="warn">
            The orchestrator&apos;s action block in this reply could not be read. No action was proposed. Nothing was changed.
          </Notice>
        )}
        {cards}
      </>
    );
  };

  return (
    <section className="ws-conversation" aria-labelledby={headingId}>
      <header className="ws-head">
        <h2 id={headingId}>Talking to: {option.label}</h2>
        {!live && <Badge tone="placeholder">PLACEHOLDER DATA (D10)</Badge>}
        <p className="ws-scope">{option.scope}</p>
        {target.kind !== "project" && (
          <MonitoringChip
            model={monitoring}
            target={target}
            canChange={!live || (gate.can("monitoring") && gate.connection.approver)}
          />
        )}
        {target.kind !== "workstream" && (
          <ConversationActions
            option={option}
            live={live}
            connected={gate.connected}
            supported={gate.can("conversation_history")}
            running={userRunning}
            busy={busy}
            onClear={() => void clearHistory()}
            onRemove={
              target.kind === "project" && onRemoveCoordinator
                ? () => onRemoveCoordinator(target.projectId)
                : undefined
            }
          />
        )}
        {target.kind !== "project" && <ReturnBrief />}
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
          afterTurn={afterTurn}
          leading={leading}
          taskRoute={taskRoute}
        />
      </div>

      {runtimeUnavailable && turn && (
        <p role="status" className="ws-note ws-runtime">
          Orchestrator runtime unavailable ({turn.runtime.name} {turn.runtime.version}): the daemon recorded your
          message but cannot produce a reply right now.
        </p>
      )}
      {wakeRunning && (
        <p role="status" className="ws-note ws-runtime">
          {WAKE_RUNNING_TEXT}
        </p>
      )}
      {userRunning && (
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

      <RoutedInStrip
        key={option.key}
        snapshot={transcript.status === "ok" ? transcript.data : null}
        workstream={target.kind === "workstream"}
      />

      <form className="ws-composer" onSubmit={onSubmit} aria-label={`Message ${option.label}`}>
        <textarea
          id={`${ids}-draft`}
          aria-label={`Message ${option.label}`}
          rows={1}
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
            globalUnseen={monitoring.supported ? unseenFor(monitoring.state, GLOBAL_TARGET) : 0}
          />
          <ModelPicker
            catalog={modelCatalog}
            provenance={surfaces.models.provenance}
            validate={surfaces.models.validate}
            copilotSignIn={surfaces.models}
            onRetry={retryModels}
            onChoiceChange={setChoice}
            onChoiceBlocked={setChoiceBlocked}
            selectionSupported={!live || !gate.connected || gate.can("conversation_selection")}
            permission={permissionControl}
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
            olderDaemon={olderDaemon}
            lastTurn={
              live && turn
                ? {
                    daemonLevel: turn.permissionLevel ?? null,
                    sentLevel: sentLevels.current.get(turn.id) ?? null,
                    trigger: turn.trigger,
                  }
                : null
            }
          />
        )}
        {actionsLevelNote && <p className="ws-small">{ACTIONS_LEVEL_NOTE}</p>}
        {blocked && (
          <p id={sendId} className="ws-small ws-blocked">
            Sending is disabled. {blocked}
          </p>
        )}
      </form>
      {/* A changing trailing space makes a repeat of the same sentence read again. */}
      <p role="status" aria-live="polite" className="ws-visually-hidden">
        {announced > 0 ? `${NEW_UPDATE_TEXT}${announced % 2 === 0 ? "\u00a0" : ""}` : ""}
      </p>
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
  afterTurn,
  leading,
  taskRoute,
}: {
  transcript: SurfaceState<ConversationTranscript>;
  label: string;
  simulated: readonly SimulatedEntry[];
  onRetry: () => void;
  /** Rendered after the last entry of a turn (DPES-1105). */
  afterTurn?: (turnId: string) => ReactNode;
  /** Rendered before the entries, under the truncation note (DPES-1105). */
  leading?: ReactNode;
  /** Links a task event's notice to its task page (DPES-1106). */
  taskRoute: (taskId: string) => Route | null;
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
      actions={<CopyButton text={entry.text} />}
    >
      <Markdown text={entry.text} />
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
  const { truncated, repliesExpected, proposalsMore } = transcript.data;
  // Only the latest turn carries its runtime, so only its entries can name the agent.
  const latestTurn = transcript.data.turn ?? null;
  const wake = transcript.data.wake ?? [];
  // A quiet record shows one collapsed line, so its turn's own entries are not shown (DPES-1105 Phase 3).
  const quiet = quietTurnIds(wake);
  const entries = transcript.data.entries.filter((entry) => entry.turnId === undefined || !quiet.has(entry.turnId));
  const wakeTurns = new Set(entries.flatMap((entry) => (entry.origin === "wake" && entry.turnId ? [entry.turnId] : [])));
  const placement = placeWake(entries, wake);
  if (entries.length === 0 && sim.length === 0 && !leading && wake.length === 0) {
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
      {proposalsMore && <p className="ws-small">Older proposals are not shown.</p>}
      {leading}
      <WakeRecords records={placement.before} taskRoute={taskRoute} />
      {entries.map((entry, index) => (
        <Fragment key={entry.id}>
          <EntryBubble
            entry={entry}
            label={label}
            wakeTurn={entry.turnId !== undefined && wakeTurns.has(entry.turnId)}
            runtime={latestTurn && entry.turnId === latestTurn.id ? latestTurn.runtime : undefined}
          />
          {afterTurn && entry.turnId !== undefined && entries[index + 1]?.turnId !== entry.turnId && afterTurn(entry.turnId)}
          <WakeRecords records={placement.after.get(entry.id) ?? []} taskRoute={taskRoute} />
        </Fragment>
      ))}
      {sim}
    </>
  );
}

function EntryBubble({
  entry,
  label,
  wakeTurn,
  runtime,
}: {
  entry: ConversationEntry;
  label: string;
  wakeTurn: boolean;
  /** The runtime of this entry's turn, when the transcript has it (the latest turn only). */
  runtime: WakeRuntime | undefined;
}) {
  // The opening entry of an automatic turn is event lines inside an update, not something the user wrote.
  if (entry.origin === "wake") return <WakeLines text={entry.text} runtime={runtime} />;
  const human = entry.role === "user";
  // A daemon turn outcome is a system line, never a reply from the orchestrator. It is daemon text, so it stays plain.
  if (entry.kind === "status") {
    return (
      <div className="ws-state ws-turn-status">
        <p role="status" className="ws-turn-status-text">
          {entry.text}
        </p>
        <CopyButton text={entry.text} label="Copy status" />
      </div>
    );
  }
  const bubble = (
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
      actions={<CopyButton text={entry.text} />}
    >
      <Markdown text={entry.text} />
    </Bubble>
  );
  // A reply to an automatic turn says it is an update, so it is never read as an answer to the user's last message.
  return wakeTurn && !human ? (
    <div className="ws-wake-reply">
      <WakeLabel runtime={runtime} />
      {bubble}
    </div>
  ) : (
    bubble
  );
}
