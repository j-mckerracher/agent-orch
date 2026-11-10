/**
 * Chooses where the shipped app gets its data. The choice is made once at
 * start-up and never changes: a failed live request never falls back to
 * sample data.
 *
 * - inside the Tauri webview: the live daemon through the host bridge;
 * - outside it with an explicit `?fixture=1`: labelled fixture demo mode;
 * - outside it otherwise: the live path with no bridge, which reports the
 *   daemon as unavailable rather than showing sample data.
 */
import type { DaemonConnector } from "../daemon/session";
import { createServices, type CreateServicesOptions } from "../ui/createServices";
import type { DraftStorage } from "../ui/drafts";
import { parseRoute } from "../ui/router";
import type { Services } from "../ui/services";
import { createTauriConnector, insideTauri, noBridgeConnector } from "./tauriConnector";

export type AppMode = "native" | "fixture" | "no-bridge";

export interface BootTarget {
  location: { search: string; hash: string };
}

export function selectMode(target: BootTarget & object): AppMode {
  if (insideTauri(target)) return "native";
  return new URLSearchParams(target.location.search).get("fixture") === "1" ? "fixture" : "no-bridge";
}

export interface BootOptions {
  renderReview?: Services["renderReview"];
  storage?: DraftStorage;
  /** Test seam; defaults to the Tauri host bridge. */
  nativeConnector?: DaemonConnector;
  connection?: CreateServicesOptions["connection"];
}

export function bootServices(target: BootTarget & object, options: BootOptions = {}): { services: Services; mode: AppMode } {
  const mode = selectMode(target);
  const connector =
    mode === "native" ? (options.nativeConnector ?? createTauriConnector()) : mode === "no-bridge" ? noBridgeConnector : undefined;
  const services = createServices({
    connector,
    initialRoute: parseRoute(target.location.hash),
    renderReview: options.renderReview,
    draftStorage: mode === "fixture" ? undefined : options.storage,
    connection: options.connection,
  });
  return { services, mode };
}
