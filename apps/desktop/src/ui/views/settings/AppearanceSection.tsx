/** Settings > Appearance: the theme (light or dark) and the decorative background animation. Both persist on this device. */
import { useId } from "react";
import { ambientBackgroundRegistration } from "../../background";
import { ThemeToggle } from "../../theme";

export function AppearanceSection() {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="settings-section settings-appearance" data-testid="settings-appearance">
      <h2 id={headingId}>Appearance</h2>
      <div className="ambient-row">
        <ThemeToggle />
        {ambientBackgroundRegistration.control}
      </div>
    </section>
  );
}
