import { render } from "@testing-library/react";
import { ServicesProvider, type Services } from "../ui/services";
import { SettingsView } from "../ui/views/settings/SettingsView";

/** Renders Settings on one page, the way `#/settings?page=<id>` does. */
export function renderSettingsPage(services: Services, page?: string) {
  services.router.navigate({ name: "global", view: "settings", ...(page ? { settingsPage: page } : {}) });
  return render(
    <ServicesProvider services={services}>
      <SettingsView />
    </ServicesProvider>,
  );
}
