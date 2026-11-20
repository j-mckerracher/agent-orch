import { useId } from "react";
import { useServices, useSurface } from "../../services";
import { ResourceStrip } from "../../shell/ResourceStrip";

/**
 * What the workspace activity popover showed about the machine, moved here with the popover: the agent slots in use
 * and this device's CPU, memory, GPU and disk readings. The slots line is left out until the daemon reports it.
 */
export function LimitsMachineLoadSection() {
  const { surfaces } = useServices();
  const headingId = useId();
  const state = useSurface(() => surfaces.activity.activity(), "limits-slots", surfaces.activity.provenance);
  const slots = state.status === "ok" && state.data.slots.status === "ok" ? state.data.slots.value : null;
  return (
    <section aria-labelledby={headingId} className="settings-section" data-testid="limits-machine-load">
      <h2 id={headingId}>Current machine load</h2>
      {slots && (
        <p data-testid="limits-slots">
          {slots.used} of {slots.limit} agent slots in use · {Math.max(0, slots.limit - slots.used)} available
        </p>
      )}
      <ResourceStrip defaultOpen />
    </section>
  );
}
