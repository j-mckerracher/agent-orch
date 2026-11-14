import { useId } from "react";
import { useShellTier } from "../../shell/useShellTier";
import { Disclosure } from "../../theme";
import { layoutGraph, NODE_HEIGHT, NODE_WIDTH, textDependencies } from "./graph";
import { taskLabelOf, type WsTask } from "./model";

const LABEL_CHARS = 30;
const short = (text: string) => (text.length > LABEL_CHARS ? `${text.slice(0, LABEL_CHARS - 1)}…` : text);

/**
 * Task-level dependency graph (OX-W4). The SVG is a drawing of tasks that take part in a
 * dependency; the text list below it is the complete equivalent and is all that narrow
 * layouts get. Edges point from a predecessor to the task that waits on it.
 */
export function DependencyGraph({ tasks }: { tasks: readonly WsTask[] }) {
  const tier = useShellTier();
  const titleId = useId();
  const descId = useId();
  const narrow = tier === "narrow";
  const layout = layoutGraph(tasks);
  const rows = textDependencies(tasks);
  const withEdges = rows.filter((row) => row.predecessors.length > 0 || row.external.length > 0);
  const summary =
    layout.connected === 0 && withEdges.length === 0
      ? "No task in this workstream records a dependency."
      : `${withEdges.length} of ${tasks.length} tasks wait on a predecessor.`;

  return (
    <div className="ws-graph" data-testid="dependency-graph">
      <p className="ws-graph-summary">{summary}</p>
      {!narrow && layout.nodes.length > 0 && (
        <div className="ws-graph-scroll" role="region" aria-label="Dependency graph drawing" tabIndex={0}>
          <svg
            className="ws-graph-svg"
            role="img"
            aria-labelledby={`${titleId} ${descId}`}
            viewBox={`0 0 ${layout.width} ${layout.height}`}
            width={layout.width}
            height={layout.height}
          >
            <title id={titleId}>Task dependency graph</title>
            <desc id={descId}>
              {layout.nodes.length} tasks and {layout.edges.length} dependencies, predecessors above the tasks that wait on
              them. The dependency list below gives the same information as text.
            </desc>
            <defs>
              <marker id={`${titleId}-arrow`} viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto">
                <path d="M0 0L8 4L0 8z" className="ws-graph-arrow" />
              </marker>
            </defs>
            {layout.edges.map((edge) => (
              <path
                key={`${edge.from}>${edge.to}`}
                d={edge.path}
                className="ws-graph-edge"
                fill="none"
                markerEnd={`url(#${titleId}-arrow)`}
              />
            ))}
            {layout.nodes.map((node) => (
              <g key={node.task.id} transform={`translate(${node.x} ${node.y})`} data-testid="graph-node" data-state={node.task.state}>
                <title>
                  {taskLabelOf(node.task)} ({node.task.state})
                </title>
                <rect width={NODE_WIDTH} height={NODE_HEIGHT} rx={6} className="ws-graph-node" />
                <text x={10} y={22} className="ws-graph-node-name">
                  {short(taskLabelOf(node.task))}
                </text>
                <text x={10} y={42} className="ws-graph-node-state">
                  {node.task.state}
                </text>
              </g>
            ))}
          </svg>
        </div>
      )}
      {layout.omitted > 0 && (
        <p className="ws-graph-note">
          {layout.omitted} more connected tasks are not drawn; the list below has all of them.
        </p>
      )}
      {layout.cyclic && <p className="ws-graph-note">A dependency cycle was found; the drawing layers are approximate.</p>}
      <Disclosure
        key={narrow ? "narrow" : "wide"}
        summary="Dependencies as text"
        count={withEdges.length}
        defaultOpen={narrow}
        className="ws-graph-text"
      >
        {withEdges.length === 0 ? (
          <p>No dependencies recorded.</p>
        ) : (
          <ul className="ws-deps">
            {withEdges.map(({ task, predecessors, external }) => (
              <li key={task.id}>
                <b>{taskLabelOf(task)}</b> ({task.state}) waits on:{" "}
                {[
                  ...predecessors.map((predecessor) => `${taskLabelOf(predecessor)} (${predecessor.state})`),
                  ...external.map((id) => `${id} (not in this workstream, state unknown)`),
                ].join("; ")}
              </li>
            ))}
          </ul>
        )}
      </Disclosure>
    </div>
  );
}
