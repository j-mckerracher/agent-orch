/** Later-scope preview panels (scheduling/routing and usage provenance). Live mode shows only what D11/D12 report. */
import type { Measured } from "../../../sources/surfaces/types";
import { useServices, useSurface } from "../../services";
import { Disclosure } from "../../theme";
import { DisabledAction, PlaceholderMark } from "./common";

function measured<T>(value: Measured<T>, render: (v: T) => string): string {
  return value.status === "ok" ? render(value.value) : `${value.status}: ${value.reason}`;
}

export function LaterScope() {
  const { surfaces } = useServices();
  const activity = useSurface(() => surfaces.activity.activity(), "agents-activity", surfaces.activity.provenance);
  const usage = useSurface(() => surfaces.usage.usage(), "agents-usage", surfaces.usage.provenance);

  return (
    <>
      <Disclosure summary="Scheduling, routing and resource policy · Later-scope preview" data-testid="agents-scheduling">
        {surfaces.activity.provenance.kind === "fixture" && (
          <p>
            <PlaceholderMark id="D11" />
          </p>
        )}
        <h3>Why tasks wait</h3>
        {activity.status === "loading" && <p role="status">Loading scheduling data…</p>}
        {(activity.status === "unavailable" || activity.status === "unsupported" || activity.status === "unknown") && (
          <p role="status">
            Scheduling data is {activity.status}: {activity.reason}
          </p>
        )}
        {activity.status === "ok" && (
          <ul className="agents-plain">
            <li>Global concurrency: {measured(activity.data.slots, (s) => `${s.used}/${s.limit} slots`)}</li>
            {activity.data.queued.length === 0 && <li>No queued task is waiting.</li>}
            {activity.data.queued.map((entry) => (
              <li key={entry.taskId}>
                <code>{entry.taskId}</code>:{" "}
                {entry.waiting ? `${entry.waiting.reason} (${entry.waiting.detail})` : "not waiting"}
              </li>
            ))}
          </ul>
        )}
        <p className="agents-sub">
          Dependencies, provider rate limits, permissions and budgets can still hold admission; the daemon records no
          routing preferences.
        </p>
        <DisabledAction
          label="Review routing proposal"
          reason="Not available: the daemon records no routing proposals."
        />
        <DisabledAction
          label="Choose manual assignment"
          reason="Not available: the daemon has no method to assign an agent to an existing task."
        />
      </Disclosure>

      <Disclosure summary="Usage provenance and budget accounting · Later-scope preview" data-testid="agents-usage">
        {surfaces.usage.provenance.kind === "fixture" && (
          <p>
            <PlaceholderMark id="D12" />
          </p>
        )}
        {usage.status === "loading" && <p role="status">Loading usage data…</p>}
        {(usage.status === "unavailable" || usage.status === "unsupported" || usage.status === "unknown") && (
          <p role="status">
            Usage is {usage.status}: {usage.reason}
          </p>
        )}
        {usage.status === "ok" && (
          <div className="agents-scroll">
            <table aria-label="Usage provenance">
              <thead>
                <tr>
                  <th scope="col">Measure</th>
                  <th scope="col">Value</th>
                </tr>
              </thead>
              <tbody>
                {(
                  [
                    ["Reported spend", usage.data.reported],
                    ["Estimated spend", usage.data.estimated],
                    ["Budget", usage.data.budget],
                  ] as const
                ).map(([label, value]) => (
                  <tr key={label}>
                    <th scope="row">{label}</th>
                    <td>{measured(value, (n) => `${usage.data.currency} ${n.toFixed(2)}`)}</td>
                  </tr>
                ))}
                {usage.data.coverage.map((c) => (
                  <tr key={c.provider}>
                    <th scope="row">Coverage: {c.provider}</th>
                    <td>{c.reporting}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        <p className="agents-sub">Unknown is not zero. Each figure names its source; none is summed into an invented total.</p>
      </Disclosure>
    </>
  );
}
