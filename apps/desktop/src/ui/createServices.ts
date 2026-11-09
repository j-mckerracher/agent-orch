import { AppData, type AppDataOptions } from "../daemon/appData";
import { ConnectionManager, type ConnectionOptions } from "../daemon/connection";
import { DaemonError } from "../daemon/errors";
import type { DaemonConnector } from "../daemon/session";
import { createFixtureSources } from "../sources/fixtures";
import type { Sources } from "../sources/types";
import { DraftsStore } from "./drafts";
import { RouterStore, type Route } from "./router";
import type { Services } from "./services";

export interface CreateServicesOptions {
  /** Host-supplied authenticated connector; absent until the M5-05 native bridge exists. */
  connector?: DaemonConnector;
  /** Daemon-backed adapters replace fixtures here, with no component change. */
  sources?: Sources;
  initialRoute?: Route;
  connection?: Omit<ConnectionOptions, "connector">;
  data?: AppDataOptions;
  renderReview?: Services["renderReview"];
}

const unboundConnector: DaemonConnector = {
  open: () => Promise.reject(new DaemonError("disconnected", "native bridge not bound")),
};

export function createServices(options: CreateServicesOptions = {}): Services {
  const connection = new ConnectionManager({ ...options.connection, connector: options.connector ?? unboundConnector });
  return {
    connection,
    data: new AppData(connection, options.data),
    sources: options.sources ?? createFixtureSources(),
    router: new RouterStore(options.initialRoute),
    drafts: new DraftsStore(),
    bridgeBound: options.connector !== undefined,
    renderReview: options.renderReview,
  };
}
