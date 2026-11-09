import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect } from "vitest";
import App from "../App";
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
}

export async function mount(fake: FakeDaemon, options: MountOptions = {}) {
  const { route, bridge = true, waitConnected = true, ...rest } = options;
  const services: Services = createServices({
    connector: bridge ? fake : undefined,
    initialRoute: route,
    connection: { backoffMs: [5], requestTimeoutMs: 250, ...rest.connection },
    data: { coalesceMs: 10, ...rest.data },
    sources: rest.sources,
    renderReview: rest.renderReview,
  });
  const user = userEvent.setup();
  const view = render(<App services={services} />);
  if (bridge && waitConnected) await connected();
  return { services, user, view };
}

export const connectionStatus = () => screen.getByTestId("connection-status");

export const connected = () => waitFor(() => expect(connectionStatus()).toHaveTextContent("Connected"));
