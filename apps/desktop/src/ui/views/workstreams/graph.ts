/**
 * Layered layout for the task-level dependency graph (OX-W4). Pure and DOM-free so it can
 * be tested. Only tasks that take part in a dependency within the workstream are drawn;
 * the text list is the complete, accessible equivalent and is the only view on narrow
 * layouts.
 */
import type { WsTask } from "./model";

export const NODE_WIDTH = 236;
export const NODE_HEIGHT = 54;
const H_GAP = 24;
const V_GAP = 44;
const PAD = 8;
/** More nodes than this make the SVG unreadable; the text list still shows every task. */
export const MAX_GRAPH_NODES = 16;

export interface GraphNode {
  task: WsTask;
  layer: number;
  x: number;
  y: number;
}

export interface GraphEdge {
  from: string;
  to: string;
  path: string;
}

export interface GraphLayout {
  nodes: GraphNode[];
  edges: GraphEdge[];
  width: number;
  height: number;
  /** Tasks that depend on, or are depended on by, another task of the workstream. */
  connected: number;
  /** Tasks left out of the drawing because of `MAX_GRAPH_NODES`. */
  omitted: number;
  /** A dependency cycle was found; the layers are a best effort. */
  cyclic: boolean;
}

export interface TextDependency {
  task: WsTask;
  /** Predecessors recorded in this workstream. */
  predecessors: WsTask[];
  /** Predecessor ids that no task of this workstream carries (other project, or unknown). */
  external: string[];
  /** Tasks of this workstream that wait on this one. */
  consumers: WsTask[];
}

export function textDependencies(tasks: readonly WsTask[]): TextDependency[] {
  const byId = new Map(tasks.map((task) => [task.id, task]));
  return tasks.map((task) => ({
    task,
    predecessors: task.dependencies.flatMap((id) => byId.get(id) ?? []),
    external: task.dependencies.filter((id) => !byId.has(id)),
    consumers: tasks.filter((other) => other.dependencies.includes(task.id)),
  }));
}

export function layoutGraph(tasks: readonly WsTask[]): GraphLayout {
  const byId = new Map(tasks.map((task) => [task.id, task]));
  const known = (task: WsTask) => task.dependencies.filter((id) => byId.has(id) && id !== task.id);
  const hasConsumer = new Set(tasks.flatMap(known));
  const connectedTasks = tasks.filter((task) => known(task).length > 0 || hasConsumer.has(task.id));

  const layerOf = new Map<string, number>();
  const visiting = new Set<string>();
  let cyclic = false;
  const layer = (task: WsTask): number => {
    const cached = layerOf.get(task.id);
    if (cached !== undefined) return cached;
    if (visiting.has(task.id)) {
      cyclic = true;
      return 0;
    }
    visiting.add(task.id);
    const value = known(task).reduce((max, id) => Math.max(max, layer(byId.get(id)!) + 1), 0);
    visiting.delete(task.id);
    layerOf.set(task.id, value);
    return value;
  };
  for (const task of connectedTasks) layer(task);

  const drawn = [...connectedTasks]
    .sort((a, b) => (layerOf.get(a.id) ?? 0) - (layerOf.get(b.id) ?? 0))
    .slice(0, MAX_GRAPH_NODES);
  const drawnIds = new Set(drawn.map((task) => task.id));
  const rows = new Map<number, WsTask[]>();
  for (const task of drawn) {
    const row = layerOf.get(task.id) ?? 0;
    rows.set(row, [...(rows.get(row) ?? []), task]);
  }
  const widest = Math.max(1, ...[...rows.values()].map((row) => row.length));
  const width = widest * NODE_WIDTH + (widest - 1) * H_GAP + PAD * 2;
  const layers = [...rows.keys()].sort((a, b) => a - b);
  const nodes: GraphNode[] = [];
  layers.forEach((row, index) => {
    const members = rows.get(row)!;
    const rowWidth = members.length * NODE_WIDTH + (members.length - 1) * H_GAP;
    const left = (width - rowWidth) / 2;
    members.forEach((task, column) => {
      nodes.push({ task, layer: index, x: left + column * (NODE_WIDTH + H_GAP), y: PAD + index * (NODE_HEIGHT + V_GAP) });
    });
  });
  const position = new Map(nodes.map((node) => [node.task.id, node]));
  const edges: GraphEdge[] = [];
  for (const node of nodes) {
    for (const id of known(node.task)) {
      const from = position.get(id);
      if (!from || !drawnIds.has(id)) continue;
      const x1 = from.x + NODE_WIDTH / 2;
      const y1 = from.y + NODE_HEIGHT;
      const x2 = node.x + NODE_WIDTH / 2;
      const y2 = node.y;
      const mid = y1 + V_GAP / 2;
      edges.push({ from: id, to: node.task.id, path: `M${x1} ${y1}V${mid}H${x2}V${y2}` });
    }
  }
  const height = nodes.length === 0 ? 0 : PAD * 2 + layers.length * NODE_HEIGHT + (layers.length - 1) * V_GAP;
  return { nodes, edges, width, height, connected: connectedTasks.length, omitted: connectedTasks.length - drawn.length, cyclic };
}
