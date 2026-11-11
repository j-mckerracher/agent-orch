import { createRoot } from "react-dom/client";
import App from "./App";
import { bootServices } from "./native/boot";
import { ReviewRoute } from "./review/ReviewRoute";
import { appGlobalViews, appShellSlots } from "./ui/registry";
import { syncRouterToHash } from "./ui/router";
import { defaultStorage } from "./ui/theme/theme";
import "./ui/app.css";

const { services } = bootServices(window, {
  renderReview: (props) => <ReviewRoute {...props} />,
  // Reading the property can throw in a blocked profile; drafts then stay in memory instead of blanking the app.
  storage: defaultStorage() ?? undefined,
});
syncRouterToHash(services.router);

// Closing or reloading the page ends only this window's daemon connection; the daemon keeps running.
window.addEventListener("pagehide", () => services.connection.stop());

createRoot(document.getElementById("root")!).render(<App services={services} shellSlots={appShellSlots} globalViews={appGlobalViews} />);
