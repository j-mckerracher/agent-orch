import { useId } from "react";
import { useServices, useSource } from "../../services";
import { SourceSection } from "../../widgets";

/** Which agents the daemon detects and whether a launch is possible (moved from the project page; DPES-1044). */
export function AgentDetectionSection() {
  const { sources } = useServices();
  const headingId = useId();
  const agents = useSource(() => sources.agents.listAgents(), "agents", sources.agents.provenance);
  const availability = useSource(() => sources.execution.availability(), "availability", sources.execution.provenance);
  return (
    <section aria-labelledby={headingId} className="settings-section">
      <h2 id={headingId}>Agent detection</h2>
      <SourceSection title="Detected agents" provenance={sources.agents.provenance} result={agents} emptyText="No agents reported.">
        {(list) => (
          <ul className="ps-steps">
            {list.map((agent) => (
              <li key={agent.id}>
                {agent.name}: {agent.availability}
                {agent.features.length > 0 && ` (${agent.features.join(", ")})`}
                {agent.note && <span className="muted"> {agent.note}</span>}
              </li>
            ))}
          </ul>
        )}
      </SourceSection>
      <SourceSection
        title="Execution availability"
        provenance={sources.execution.provenance}
        result={availability}
        emptyText="No availability reported."
      >
        {(info) => (
          <p>
            Launch available: {info.launchAvailable ? "yes" : "no"}. {info.reason}
          </p>
        )}
      </SourceSection>
    </section>
  );
}
