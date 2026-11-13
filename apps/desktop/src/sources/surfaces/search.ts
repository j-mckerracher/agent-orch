/**
 * D15 search: a local, in-memory index over what D9 (workstreams and tasks)
 * and D16 (decisions) return. It has no daemon method and no fixture of its
 * own; its provenance mirrors its inputs, so fixture-backed inputs are labelled
 * PLACEHOLDER DATA. Any mixed input also gives the aggregate fixture provenance;
 * individual hits retain their own origin. Daemon inputs list their source methods.
 *
 * Failure semantics: an input that is not `ok` is listed in `skipped` and its
 * records are not searched, so a miss there is not evidence of absence. When
 * every input is not `ok`, the result takes the most actionable status
 * (unavailable, then unknown, then unsupported). The index is rebuilt per call.
 */
import { notOk, observation, okResult, oldest } from "./result";
import type {
  DecisionQueueSource,
  Provenance,
  SearchHit,
  SearchResults,
  SearchSource,
  SurfaceResult,
  SurfaceStatus,
  WorkstreamSource,
} from "./types";

// One global cap, in source order: each workstream then its tasks, followed by decisions.
// Later hits, including decisions, are dropped first when the combined list exceeds 50.
const MAX_HITS = 50;
const STATUS_RANK: Record<Exclude<SurfaceStatus, "ok">, number> = { unavailable: 0, unknown: 1, unsupported: 2 };

function combine(parts: readonly Provenance[]): Provenance {
  const fixture = parts.find((part) => part.kind === "fixture");
  if (fixture && fixture.kind === "fixture") {
    return { kind: "fixture", id: "D15", fixture: `local-index:${fixture.fixture}` };
  }
  const methods = new Set<string>();
  for (const part of parts) if (part.kind === "daemon") for (const method of part.methods ?? []) methods.add(method);
  return { kind: "daemon", id: "D15", methods: [...methods].sort() };
}

function matches(terms: readonly string[], ...fields: string[]): boolean {
  const haystack = fields.join("\n").toLowerCase();
  return terms.every((term) => haystack.includes(term));
}

export function createSearchSource(
  workstreams: WorkstreamSource,
  decisions: DecisionQueueSource,
  now: () => number = Date.now,
): SearchSource {
  return {
    provenance: combine([workstreams.provenance, decisions.provenance]),
    async search(query): Promise<SurfaceResult<SearchResults>> {
      const [ws, dq] = await Promise.all([workstreams.workstreams(), decisions.queue()]);
      const provenance = combine([ws.provenance, dq.provenance]);
      const skipped: SearchResults["skipped"] = [];
      if (ws.status !== "ok") skipped.push({ source: "D9", status: ws.status, reason: ws.reason });
      if (dq.status !== "ok") skipped.push({ source: "D16", status: dq.status, reason: dq.reason });
      if (ws.status !== "ok" && dq.status !== "ok") {
        const worst = skipped.reduce((a, b) => (STATUS_RANK[b.status] < STATUS_RANK[a.status] ? b : a));
        return notOk(
          provenance, worst.status,
          skipped.map((item) => `${item.source}: ${item.reason}`).join("; "),
          observation(now()),
        );
      }

      const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
      const hits: SearchHit[] = [];
      if (terms.length > 0) {
        if (ws.status === "ok") {
          const origin = ws.provenance.kind === "fixture" ? "fixture" : "real";
          for (const workstream of ws.data) {
            if (matches(terms, workstream.id, workstream.title)) {
              hits.push({
                kind: "workstream",
                id: workstream.id,
                title: workstream.title,
                detail: workstream.unreadReason === undefined ? `${workstream.tasks.length} task(s)` : "tasks unknown",
                origin,
              });
            }
            for (const task of workstream.tasks) {
              if (matches(terms, task.id, task.objective, task.agent)) {
                hits.push({ kind: "task", id: task.id, title: task.objective, detail: `${task.state} - ${task.agent}`, origin });
              }
            }
          }
        }
        if (dq.status === "ok") {
          const origin = dq.provenance.kind === "fixture" ? "fixture" : "real";
          for (const item of dq.data.items) {
            if (matches(terms, item.id, item.taskId, item.kind, item.state)) {
              hits.push({ kind: "decision", id: item.id, title: `${item.kind} for ${item.taskId}`, detail: item.state, origin });
            }
          }
        }
      }

      const oks = [ws, dq].flatMap((result) =>
        result.status === "ok" ? [{ observedAt: result.observedAt, ageMs: result.ageMs }] : [],
      );
      const obs = oldest(oks) ?? observation(now());
      return okResult(
        provenance,
        { query, hits: hits.slice(0, MAX_HITS), truncated: hits.length > MAX_HITS, skipped },
        obs,
      );
    },
  };
}
