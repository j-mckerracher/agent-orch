// State and transport for S6. All data comes from the injected ReviewClient
// (REAL contract: approvals.list, tasks.list/get, workspaces.diff,
// approvals.resolve, delivery.request) or the injected D7 evidence source. A
// failed live request surfaces as an error; nothing falls back to fixtures.
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { asDaemonError, type DaemonErrorKind } from "../daemon/errors";
import type { Approval, DaemonEvent, DeliveryResult, ResolveDecision, TaskRecord, WorkspaceDiff } from "../daemon/types";
import type { ReviewClient } from "./client";
import {
  acceptBlockers,
  deliveryPlan,
  digestHex,
  rejectBlockers,
  type ActionContext,
  type DiffState,
} from "./binding";
import { isSafeRelativePath, parseUnifiedDiff, type DiffFile } from "./diff";
import type { ReviewEvidenceResult, ReviewEvidenceSource } from "./sources";

export interface ReviewScope {
  projectId?: string;
  taskId?: string;
}

export type MutationView<T> =
  | { phase: "pending" }
  | { phase: "succeeded"; receipt: T }
  | { phase: "failed"; kind: DaemonErrorKind; message: string; indeterminate: boolean; epoch: number };

export interface ScopedDiff {
  path: string;
  state: "loading" | "ok" | "error";
  text?: string;
  note?: string;
  error?: string;
}

interface Mutations {
  decision?: MutationView<Approval>;
  delivery?: MutationView<DeliveryResult>;
}

export interface UseReviewOptions {
  client: ReviewClient;
  evidenceSource: ReviewEvidenceSource;
  scope: ReviewScope;
  newKey?: () => string;
}

const selfEventKey = (approvalId: string, kind: string) => `${kind}|${approvalId}`;

function releaseSelfEvent(expected: Map<string, number>, key: string) {
  const count = expected.get(key) ?? 0;
  if (count > 1) expected.set(key, count - 1);
  else expected.delete(key);
}

/** Matches M5-02's durable event `approval.<approved|rejected>` with payload `{approval_id, event}`. */
function consumeSelfEvent(expected: Map<string, number>, event: DaemonEvent): boolean {
  if (event.kind !== "approval.approved" && event.kind !== "approval.rejected") return false;
  const payload = event.payload;
  if (typeof payload !== "object" || payload === null) return false;
  const approvalId = (payload as { approval_id?: unknown }).approval_id;
  if (typeof approvalId !== "string") return false;
  const key = selfEventKey(approvalId, event.kind);
  if (!expected.has(key)) return false;
  releaseSelfEvent(expected, key);
  return true;
}

const defaultNewKey = () => crypto.randomUUID();

export function useReview({ client, evidenceSource, scope, newKey = defaultNewKey }: UseReviewOptions) {
  const subscribeConnection = useCallback(
    (onChange: () => void) => client.subscribe((notice) => notice.type === "connection" && onChange()),
    [client],
  );
  const connection = useSyncExternalStore(subscribeConnection, () => client.connection());
  const connected = connection.status === "connected";

  const [listState, setListState] = useState<"loading" | "ready" | "error">("loading");
  const [listError, setListError] = useState<{ kind: DaemonErrorKind; message: string } | null>(null);
  const [pending, setPending] = useState<Approval[]>([]);
  const [hiddenCount, setHiddenCount] = useState(0);
  const [resolved, setResolvedState] = useState<Record<string, Approval>>({});
  // Decisions resolved in this session. approvals.list returns only pending
  // approvals, so these stay selectable to continue to delivery. The ref is the
  // synchronous source for refresh(); the state drives rendering.
  const resolvedRef = useRef<Record<string, Approval>>({});
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [task, setTask] = useState<TaskRecord | null>(null);
  const [diff, setDiff] = useState<DiffState>({ status: "loading" });
  const [evidence, setEvidence] = useState<ReviewEvidenceResult | null>(null);
  const [contextLoading, setContextLoading] = useState(false);
  const [stale, setStale] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [refreshEpoch, setRefreshEpoch] = useState(0);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [scoped, setScoped] = useState<ScopedDiff | null>(null);
  const [mutations, setMutations] = useState<Record<string, Mutations>>({});

  const keys = useRef(new Map<string, string>());
  const expectedSelfEvents = useRef(new Map<string, number>());
  const inFlight = useRef(false);
  const loadSeq = useRef(0);
  const loading = useRef(false);
  const dirtyDuringLoad = useRef(false);
  const selectedTaskId = useRef<string | null>(null);
  const snapshot = useRef<{ id: string; commit: string; digest: string } | null>(null);

  const items = useMemo(() => {
    const ids = new Set(pending.map((a) => a.id));
    return [...pending, ...Object.values(resolved).filter((a) => !ids.has(a.id))];
  }, [pending, resolved]);
  const approval = items.find((a) => a.id === selectedId) ?? null;
  selectedTaskId.current = approval?.action.task_id ?? null;

  const stableKey = useCallback(
    (fingerprint: string) => {
      let key = keys.current.get(fingerprint);
      if (!key) {
        key = newKey();
        keys.current.set(fingerprint, key);
      }
      return key;
    },
    [newKey],
  );

  const loadContext = useCallback(
    async (target: Approval, seq: number) => {
      const taskId = target.action.task_id;
      const request = {
        taskId,
        approvalId: target.id,
        candidateCommit: target.action.candidate_commit,
        digestHex: digestHex(target.action.parameter_digest),
      };
      const [taskResult, diffResult, evidenceResult] = await Promise.all([
        client.call<TaskRecord>("tasks.get", { task_id: taskId }).then(
          (value) => ({ ok: true as const, value }),
          () => ({ ok: false as const }),
        ),
        client.call<WorkspaceDiff>("workspaces.diff", { task_id: taskId }).then(
          (value) => ({ ok: true as const, value }),
          (error: unknown) => ({ ok: false as const, error: asDaemonError(error) }),
        ),
        evidenceSource.load(request).catch(
          (error: unknown): ReviewEvidenceResult => ({
            state: "error",
            reason: asDaemonError(error).message,
            provenance: evidenceSource.provenance,
          }),
        ),
      ]);
      if (seq !== loadSeq.current) return;
      setTask(taskResult.ok ? taskResult.value : null);
      if (diffResult.ok) {
        const { workspace_id: workspaceId, diff: text, task_id: diffTask } = diffResult.value;
        if (diffTask !== taskId) setDiff({ status: "unavailable", reason: "The daemon returned a diff for a different task." });
        else if (text.trim() === "") setDiff({ status: "empty", workspaceId });
        else setDiff({ status: "ok", workspaceId, text });
      } else if (diffResult.error.kind === "result-too-large") {
        setDiff({ status: "too_large" });
      } else {
        setDiff({ status: "unavailable", reason: `${diffResult.error.kind}: ${diffResult.error.message}` });
      }
      setEvidence(evidenceResult);
    },
    [client, evidenceSource],
  );

  const refresh = useCallback(
    async (preferId?: string | null) => {
      const seq = ++loadSeq.current;
      loading.current = true;
      dirtyDuringLoad.current = false;
      setContextLoading(true);
      try {
        let all: Approval[];
        let hidden = 0;
        try {
          all = await client.call<Approval[]>("approvals.list", {});
          if (scope.taskId === undefined && scope.projectId !== undefined) {
            const tasks = await client.call<TaskRecord[]>("tasks.list", {});
            const inProject = new Set(
              tasks.filter((t) => t.contract.project_id === scope.projectId).map((t) => t.contract.id),
            );
            all = all.filter((a) => inProject.has(a.action.task_id));
          }
        } catch (error) {
          if (seq !== loadSeq.current) return;
          const e = asDaemonError(error);
          setListError({ kind: e.kind, message: e.message });
          setListState("error");
          return;
        }
        if (seq !== loadSeq.current) return;
        const scoped = scope.taskId ? all.filter((a) => a.action.task_id === scope.taskId) : all;
        const reviewable = scoped.filter((a) => a.action.kind.type === "delivery");
        hidden = scoped.length - reviewable.length;
        setPending(reviewable);
        setHiddenCount(hidden);
        setListError(null);
        setListState("ready");

        const wanted = preferId ?? snapshot.current?.id ?? null;
        const visible = [
          ...reviewable,
          ...Object.values(resolvedRef.current).filter((r) => !reviewable.some((a) => a.id === r.id)),
        ];
        const next = visible.find((a) => a.id === wanted) ?? (preferId ? null : visible[0] ?? null);
        const previous = snapshot.current;
        if (previous && next && next.id === previous.id) {
          const changed =
            next.action.candidate_commit !== previous.commit ||
            digestHex(next.action.parameter_digest) !== previous.digest;
          setNotice(
            changed
              ? "The candidate or action changed since you last viewed it. Earlier evidence and acceptance no longer apply; review the refreshed context."
              : null,
          );
        } else if (previous && !next) {
          setNotice("The selected decision is no longer pending or no longer exists.");
        } else if (previous && next && next.id !== previous.id && !preferId) {
          setNotice("The previously selected decision is gone; showing another pending decision.");
        } else {
          setNotice(null);
        }
        setSelectedId(next?.id ?? null);
        setSelectedPath(null);
        setScoped(null);
        if (next) {
          snapshot.current = {
            id: next.id,
            commit: next.action.candidate_commit,
            digest: digestHex(next.action.parameter_digest),
          };
          await loadContext(next, seq);
        } else {
          snapshot.current = null;
          setTask(null);
          setDiff({ status: "loading" });
          setEvidence(null);
        }
        if (seq !== loadSeq.current) return;
        setStale(dirtyDuringLoad.current);
        setRefreshEpoch((e) => e + 1);
      } finally {
        if (seq === loadSeq.current) {
          loading.current = false;
          setContextLoading(false);
        }
      }
    },
    [client, scope.projectId, scope.taskId, loadContext],
  );

  // Initial load, and a fresh baseline whenever the client or scope changes.
  const refreshRef = useRef(refresh);
  refreshRef.current = refresh;
  useEffect(() => {
    snapshot.current = null;
    void refreshRef.current(null);
  }, [client, evidenceSource, scope.projectId, scope.taskId]);

  // Any event for the reviewed task, or a reconnect, makes the preview stale,
  // except the single `approval.<decision>` event (payload.approval_id) that
  // the daemon appends for a decision this client just submitted: that change is
  // already reflected by the receipt and the refresh that follows it.
  useEffect(() => {
    return client.subscribe((n) => {
      if (n.type === "event" && consumeSelfEvent(expectedSelfEvents.current, n.event)) return;
      if (n.type === "event" && n.event.task_id === selectedTaskId.current) {
        if (loading.current) dirtyDuringLoad.current = true;
        else setStale(true);
      } else if (n.type === "events_error") {
        setStale(true);
      }
    });
  }, [client]);
  const previousStatus = useRef(connection.status);
  useEffect(() => {
    if (previousStatus.current !== "connected" && connection.status === "connected" && snapshot.current) {
      setStale(true);
    }
    previousStatus.current = connection.status;
  }, [connection.status]);

  const select = useCallback(
    (id: string) => {
      const target = items.find((a) => a.id === id);
      if (!target) return;
      const seq = ++loadSeq.current;
      setSelectedId(id);
      setSelectedPath(null);
      setScoped(null);
      setNotice(null);
      setStale(false);
      setContextLoading(true);
      loading.current = true;
      dirtyDuringLoad.current = false;
      snapshot.current = {
        id: target.id,
        commit: target.action.candidate_commit,
        digest: digestHex(target.action.parameter_digest),
      };
      setDiff({ status: "loading" });
      setEvidence(null);
      void loadContext(target, seq).finally(() => {
        if (seq === loadSeq.current) {
          loading.current = false;
          setStale(dirtyDuringLoad.current);
          setContextLoading(false);
        }
      });
    },
    [items, loadContext],
  );

  const files: DiffFile[] = useMemo(() => (diff.status === "ok" ? parseUnifiedDiff(diff.text) : []), [diff]);

  const loadPath = useCallback(
    async (path: string) => {
      if (!approval || (diff.status !== "ok" && diff.status !== "too_large" && diff.status !== "empty")) return;
      if (!isSafeRelativePath(path)) {
        setSelectedPath(path);
        setScoped({ path, state: "error", error: "Enter a safe repository-relative path (no absolute, `..`, leading `-`, or backslash)." });
        return;
      }
      setSelectedPath(path);
      setScoped({ path, state: "loading" });
      const expectedWorkspace = diff.status === "too_large" ? null : diff.workspaceId;
      const seq = loadSeq.current;
      try {
        const result = await client.call<WorkspaceDiff>("workspaces.diff", {
          task_id: approval.action.task_id,
          ...(expectedWorkspace ? { workspace_id: expectedWorkspace } : {}),
          path,
        });
        if (seq !== loadSeq.current) return;
        if (result.task_id !== approval.action.task_id || (expectedWorkspace && result.workspace_id !== expectedWorkspace)) {
          setScoped({ path, state: "error", error: "The daemon returned a diff for a different task or workspace." });
          setStale(true);
          return;
        }
        setScoped({ path, state: "ok", text: result.diff });
      } catch (error) {
        if (seq !== loadSeq.current) return;
        const e = asDaemonError(error);
        const local = files.find((f) => f.path === path);
        if (e.kind === "invalid-params" && local?.status === "deleted") {
          setScoped({
            path,
            state: "ok",
            text: local.text,
            note: "The file no longer exists in the worktree, so the daemon cannot scope it; this section is sliced from the full diff already loaded.",
          });
        } else {
          setScoped({ path, state: "error", error: e.kind === "result-too-large" ? "This file's diff is too large to display." : `${e.kind}: ${e.message}` });
        }
      }
    },
    [approval, client, diff, files],
  );

  const ctx: ActionContext = {
    connected,
    loading: contextLoading,
    stale,
    refreshRequired: false,
    approval,
    task,
    diff,
    evidence,
  };
  const mutation = approval ? (mutations[approval.id] ?? {}) : {};
  const failedAt = (view?: MutationView<unknown>) => view?.phase === "failed" && view.epoch === refreshEpoch;
  const decisionCtx = { ...ctx, refreshRequired: failedAt(mutation.decision) };
  const deliveryCtx = { ...ctx, refreshRequired: failedAt(mutation.delivery) };
  const ctxRef = useRef({ decisionCtx, deliveryCtx });
  ctxRef.current = { decisionCtx, deliveryCtx };
  const pendingRequest = mutation.decision?.phase === "pending" || mutation.delivery?.phase === "pending";

  const accept = acceptBlockers(decisionCtx);
  const reject = rejectBlockers(decisionCtx);
  const delivery = deliveryPlan(deliveryCtx);

  const track = useCallback(
    async <T,>(approvalId: string, slot: keyof Mutations, run: () => Promise<T>): Promise<T | undefined> => {
      setMutations((m) => ({ ...m, [approvalId]: { ...m[approvalId], [slot]: { phase: "pending" } } }));
      try {
        const receipt = await run();
        setMutations((m) => ({ ...m, [approvalId]: { ...m[approvalId], [slot]: { phase: "succeeded", receipt } } }));
        return receipt;
      } catch (error) {
        const e = asDaemonError(error);
        setMutations((m) => ({
          ...m,
          [approvalId]: {
            ...m[approvalId],
            [slot]: { phase: "failed", kind: e.kind, message: e.message, indeterminate: e.indeterminate, epoch: refreshEpochRef.current },
          },
        }));
        return undefined;
      }
    },
    [],
  );
  const refreshEpochRef = useRef(refreshEpoch);
  refreshEpochRef.current = refreshEpoch;

  const decide = useCallback(
    async (decision: ResolveDecision) => {
      const current = ctxRef.current.decisionCtx;
      const target = current.approval;
      const blockers = decision === "approve" ? acceptBlockers(current) : rejectBlockers(current);
      if (!target || blockers.length > 0 || inFlight.current) return;
      const fingerprint = `resolve|${target.id}|${decision}|${target.action.candidate_commit}|${digestHex(target.action.parameter_digest)}`;
      const key = stableKey(fingerprint);
      const eventKey = selfEventKey(target.id, decision === "approve" ? "approval.approved" : "approval.rejected");
      inFlight.current = true;
      expectedSelfEvents.current.set(eventKey, (expectedSelfEvents.current.get(eventKey) ?? 0) + 1);
      let receipt: Approval | undefined;
      try {
        receipt = await track(target.id, "decision", () =>
          client.call<Approval>("approvals.resolve", { approval_id: target.id, decision, idempotency_key: key }),
        );
      } finally {
        inFlight.current = false;
      }
      // A failed (or indeterminate) request may still emit an event; treat it as external.
      if (!receipt) releaseSelfEvent(expectedSelfEvents.current, eventKey);
      if (receipt) {
        resolvedRef.current = { ...resolvedRef.current, [target.id]: receipt };
        setResolvedState(resolvedRef.current);
        await refreshRef.current(target.id);
      }
    },
    [client, stableKey, track],
  );

  const requestDelivery = useCallback(async () => {
    const plan = deliveryPlan(ctxRef.current.deliveryCtx);
    if (!plan.request || inFlight.current) return;
    const request = plan.request;
    const key = stableKey(`delivery|${JSON.stringify(request)}|${ctxRef.current.deliveryCtx.approval?.action.candidate_commit}`);
    inFlight.current = true;
    let receipt: DeliveryResult | undefined;
    try {
      receipt = await track(request.approval_id, "delivery", () =>
        client.call<DeliveryResult>("delivery.request", { ...request, idempotency_key: key }),
      );
    } finally {
      inFlight.current = false;
    }
    if (receipt) await refreshRef.current(request.approval_id);
  }, [client, stableKey, track]);

  return {
    connection,
    listState,
    listError,
    items,
    hiddenCount,
    approval,
    task,
    diff,
    files,
    evidence,
    loading: contextLoading,
    stale,
    notice,
    selectedPath,
    scoped,
    mutation,
    pendingRequest,
    acceptBlockers: accept,
    rejectBlockers: reject,
    deliveryBlockers: delivery.blockers,
    canDeliver: delivery.request !== null,
    deliveryRequest: delivery.request,
    select,
    refresh: () => refresh(snapshot.current?.id ?? null),
    loadPath,
    decide,
    requestDelivery,
  };
}
