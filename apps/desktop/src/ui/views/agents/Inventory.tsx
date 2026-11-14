/** Configured adapter inventory: flags are shown exactly as reported, unknown stays unknown. */
import { Disclosure } from "../../theme";
import { DisabledAction, FlagPill, LoadRegion, PlaceholderMark } from "./common";
import type { InventoryRow } from "./inventoryModel";
import type { Load } from "./useAgentsData";

const USAGE_TEXT = "Unknown (the daemon does not report usage)";

export function Inventory({
  inventory,
  live,
  onRetry,
}: {
  inventory: Load<InventoryRow[]>;
  live: boolean;
  onRetry: () => void;
}) {
  const count = inventory.status === "ready" ? `${inventory.data.length} adapters` : undefined;
  return (
    <Disclosure
      defaultOpen
      summary="Configured inventory and discovered capabilities"
      count={count}
      data-testid="agents-inventory"
    >
      {!live && (
        <p>
          <PlaceholderMark id="D1" />
        </p>
      )}
      <LoadRegion
        what="Adapter inventory"
        load={inventory}
        emptyTitle="No adapters reported"
        emptyDescription="The daemon returned an empty agents.capabilities list."
        onRetry={onRetry}
      >
        {(rows) => (
          <div className="agents-scroll" role="region" aria-label="Adapter inventory table" tabIndex={0}>
            <table aria-label="Adapter inventory">
              <thead>
                <tr>
                  <th scope="col">Configured adapter</th>
                  <th scope="col">Assignment availability</th>
                  <th scope="col">Resume</th>
                  <th scope="col">Interactive input</th>
                  <th scope="col">Usage reporting</th>
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
                        <span className="agents-sub">
                          installed: {row.installed}; launchable: {row.launchable}
                        </span>
                      )}
                      {row.note && <span className="agents-sub">{row.note}</span>}
                    </td>
                    <td>
                      <FlagPill flag={row.resume} />
                    </td>
                    <td>
                      <FlagPill flag={row.interactive} />
                    </td>
                    <td>{USAGE_TEXT}</td>
                    <td>
                      {row.capabilities === null ? (
                        "Not reported"
                      ) : row.capabilities.length === 0 ? (
                        "None reported"
                      ) : (
                        <ul className="agents-flags" aria-label={`${row.name} capabilities`}>
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
      <DisabledAction
        label="Configure providers"
        reason="Provider configuration is not available: the daemon has no method to change adapters."
      />
    </Disclosure>
  );
}
