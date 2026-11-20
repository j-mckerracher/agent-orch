/** Settings > Agents: which adapters are installed and what each reports. Flags are shown as reported; unknown stays unknown. */
import { useId } from "react";
import { useServices } from "../../services";
import { Pill, type Tone } from "../../theme";
import { PlaceholderBadge } from "../../widgets";
import { LoadRegion } from "../agents/common";
import type { Flag } from "./inventoryModel";
import { useAgentsInventory } from "./useAgentsInventory";

const FLAG_TEXT: Record<Flag, { tone: Tone; text: string }> = {
  yes: { tone: "ok", text: "Yes" },
  no: { tone: "neutral", text: "No" },
  unknown: { tone: "warn", text: "Unknown" },
};

function FlagPill({ flag }: { flag: Flag }) {
  const { tone, text } = FLAG_TEXT[flag];
  return (
    <Pill tone={tone} className="settings-flag">
      {text}
    </Pill>
  );
}

export function AgentsInventorySection() {
  const headingId = useId();
  const { sources } = useServices();
  const { live, inventory, reload } = useAgentsInventory();
  return (
    <section aria-labelledby={headingId} className="settings-section settings-agents" data-testid="agents-inventory">
      <h2 id={headingId}>Agents</h2>
      <p className="settings-intro">The adapters this machine can run.</p>
      {!live && (
        <p>
          <PlaceholderBadge provenance={sources.agents.provenance} />
        </p>
      )}
      <LoadRegion
        what="Adapter inventory"
        load={inventory}
        emptyTitle="No adapters reported"
        emptyDescription="The daemon returned an empty agents.capabilities list."
        onRetry={reload}
      >
        {(rows) => (
          <div className="settings-table-scroll" role="region" aria-label="Adapter inventory table" tabIndex={0}>
            <table aria-label="Adapter inventory">
              <thead>
                <tr>
                  <th scope="col">Adapter</th>
                  <th scope="col">Availability</th>
                  <th scope="col">Resume</th>
                  <th scope="col">Interactive input</th>
                  <th scope="col">Capabilities as reported</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => (
                  <tr key={row.id}>
                    <th scope="row">
                      <code>{row.name}</code>
                    </th>
                    <td>
                      {row.assignment}
                      {live && (
                        <small className="settings-hint">
                          installed: {row.installed}; launchable: {row.launchable}
                        </small>
                      )}
                      {row.note && <small className="settings-hint">{row.note}</small>}
                    </td>
                    <td>
                      <FlagPill flag={row.resume} />
                    </td>
                    <td>
                      <FlagPill flag={row.interactive} />
                    </td>
                    <td>
                      {row.capabilities === null ? (
                        "Not reported"
                      ) : row.capabilities.length === 0 ? (
                        "None reported"
                      ) : (
                        <ul className="settings-flags" aria-label={`${row.name} capabilities`}>
                          {row.capabilities.map((c) => (
                            <li key={c.label}>
                              {c.label}: {c.flag === "yes" ? "Yes" : c.flag === "no" ? "No" : "Unknown"}
                            </li>
                          ))}
                        </ul>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </LoadRegion>
    </section>
  );
}
