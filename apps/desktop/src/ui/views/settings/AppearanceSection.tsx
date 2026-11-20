/** Settings > Appearance: the theme (light or dark) and the decorative background animation. Both persist on this device. */
import { ambientBackgroundRegistration } from "../../background";
import { ThemeToggle } from "../../theme";

export function AppearanceSection() {
  return (
    <section aria-label="Appearance" className="settings-section settings-appearance" data-testid="settings-appearance">
      <div className="ambient-row">
        <ThemeToggle />
        {ambientBackgroundRegistration.control}
      </div>
    </section>
  );
}
