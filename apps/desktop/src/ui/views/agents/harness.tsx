/** Test harness: renders the real `App` with the agents view registered as the `agents` global view. */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect } from "vitest";
import App from "../../../App";
import { createFixtureSources } from "../../../sources/fixtures";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import type { FakeDaemon } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { agentsGlobalViews } from "./register";

export interface HarnessOptions {
  /** Daemon-backed adapters (live mode). Default is fixture sources, as in the other component tests. */
  live?: boolean;
  /** Fixture mode with no host bridge: nothing connects. */
  bridge?: boolean;
}

export async function mountAgents(fake: FakeDaemon, options: HarnessOptions = {}) {
  const { live = false, bridge = true } = options;
  const services = createServices({
    connector: bridge ? fake : undefined,
    initialRoute: { name: "global", view: "agents" },
    connection: { backoffMs: [5], requestTimeoutMs: 250 },
    data: { coalesceMs: 10 },
    sources: live ? undefined : createFixtureSources(),
    surfaces: live ? undefined : createFixtureSurfaces(),
  });
  const user = userEvent.setup();
  const view = render(<App services={services} globalViews={agentsGlobalViews} />);
  if (bridge) await waitFor(() => expect(screen.getByTestId("connection-status")).toHaveTextContent("Connected"));
  return { services, user, view };
}
