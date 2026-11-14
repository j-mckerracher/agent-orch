import { AppData, type AppDataOptions } from "../daemon/appData";
import { ConnectionManager, type ConnectionOptions } from "../daemon/connection";
import { DaemonError } from "../daemon/errors";
import type { DaemonConnector } from "../daemon/session";
import { createDaemonSources } from "../sources/daemon";
import { createFixtureSources } from "../sources/fixtures";
import { createDerivedSurfaces } from "../sources/surfaces/derived";
import { createFixtureSurfaces } from "../sources/surfaces/fixtures";
import type { SurfaceSources } from "../sources/surfaces/types";
import type { Sources } from "../sources/types";
import { DraftsStore, type DraftStorage } from "./drafts";
import { RouterStore, type Route } from "./router";
import type { Services } from "./services";

export interface CreateServicesOptions {
  /** Host-supplied authenticated connector. Without one the app is in explicit fixture demo mode. */
  connector?: DaemonConnector;
  /** Overrides the default: daemon adapters when a connector is supplied, fixtures otherwise. */
  sources?: Sources;
  /** Overrides the D9-D16 default: derived adapters when a connector is supplied, fixtures otherwise. */
  surfaces?: SurfaceSources;
  initialRoute?: Route;
  connection?: Omit<ConnectionOptions, "connector">;
  data?: AppDataOptions;
  renderReview?: Services["renderReview"];
  /** Persists form drafts across window close/reopen. */
  draftStorage?: DraftStorage;
}

const unboundConnector: DaemonConnector = {
  open: () => Promise.reject(new DaemonError("disconnected", "native bridge not bound")),
};

export function createServices(options: CreateServicesOptions = {}): Services {
  const connection = new ConnectionManager({ ...options.connection, connector: options.connector ?? unboundConnector });
  return {
    connection,
    data: new AppData(connection, options.data),
    sources: options.sources ?? (options.connector ? createDaemonSources(connection) : createFixtureSources()),
    surfaces: options.surfaces ?? (options.connector ? createDerivedSurfaces(connection) : createFixtureSurfaces()),
    router: new RouterStore(options.initialRoute),
    drafts: new DraftsStore(options.draftStorage),
    bridgeBound: options.connector !== undefined,
    renderReview: options.renderReview,
  };
}
