import { createRoot } from "react-dom/client";
import App from "./App";
import { createServices } from "./ui/createServices";
import { parseRoute, syncRouterToHash } from "./ui/router";
import "./ui/app.css";

// The Tauri host supplies a daemon connector in M5-05; until then the app is honestly offline.
const services = createServices({ initialRoute: parseRoute(window.location.hash) });
syncRouterToHash(services.router);

createRoot(document.getElementById("root")!).render(<App services={services} />);
