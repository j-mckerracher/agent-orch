import { createRoot } from "react-dom/client";
import App from "./App";
import { bootServices } from "./native/boot";
import { ReviewRoute } from "./review/ReviewRoute";
import { syncRouterToHash } from "./ui/router";
import "./ui/app.css";

const { services } = bootServices(window, {
  renderReview: (props) => <ReviewRoute {...props} />,
  storage: window.localStorage,
});
syncRouterToHash(services.router);

// Closing or reloading the page ends only this window's daemon connection; the daemon keeps running.
window.addEventListener("pagehide", () => services.connection.stop());

createRoot(document.getElementById("root")!).render(<App services={services} />);
