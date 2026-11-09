import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect } from "vitest";
import App from "../App";
import { createFixtureSources } from "../sources/fixtures";
import { createServices, type CreateServicesOptions } from "../ui/createServices";
import type { Route } from "../ui/router";
import type { Services } from "../ui/services";
import type { FakeDaemon } from "./fakeDaemon";

export const REPO = "/work/repo";

export interface MountOptions extends Omit<CreateServicesOptions, "connector" | "initialRoute"> {
  route?: Route;
  /** When false the host supplies no connector, as in the shipped app before M5-05. */
  bridge?: boolean;
  waitConnected?: boolean;
  /** Use the real daemon-backed adapters instead of the fixture sources a connected host would otherwise default tests to. */
  daemonSources?: boolean;
}

export async function mount(fake: FakeDaemon, options: MountOptions = {}) {
  const { route, bridge = true, waitConnected = true, daemonSources = false, ...rest } = options;
  const services: Services = createServices({
    connector: bridge ? fake : undefined,
    initialRoute: route,
    connection: { backoffMs: [5], requestTimeoutMs: 250, ...rest.connection },
    data: { coalesceMs: 10, ...rest.data },
    sources: rest.sources ?? (daemonSources ? undefined : createFixtureSources()),
    renderReview: rest.renderReview,
  });
  const user = userEvent.setup();
  const view = render(<App services={services} />);
  if (bridge && waitConnected) await connected();
  return { services, user, view };
}

export const connectionStatus = () => screen.getByTestId("connection-status");

export const connected = () => waitFor(() => expect(connectionStatus()).toHaveTextContent("Connected"));
