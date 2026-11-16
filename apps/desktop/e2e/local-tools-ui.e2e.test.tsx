// @vitest-environment jsdom
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createServices } from "../src/ui/createServices";
import { ServicesProvider } from "../src/ui/services";
import { OPT_IN_LABEL } from "../src/ui/shell/localToolsCopy";
import { UsagePill } from "../src/ui/shell/UsagePill";
import { NodeConnector, startDaemon, type RunningDaemon } from "./harness";

/** Same missing-reader setup as local-tools.e2e.test.ts: no reader exists on any machine. */
let home: string;
let daemon: RunningDaemon;
let services: ReturnType<typeof createServices>;

function checkbox(): HTMLInputElement {
  return screen.getByRole("checkbox", { name: OPT_IN_LABEL }) as HTMLInputElement;
}

beforeAll(async () => {
  home = mkdtempSync(join(tmpdir(), "mcao-home-"));
  daemon = await startDaemon({ HOME: home, AGENT_ORCH_CODEBURN_BIN: join(home, "no-such-codeburn") });
  services = createServices({ connector: new NodeConnector(daemon.dataDir), connection: { backoffMs: [50], requestTimeoutMs: 5000 } });
  services.connection.start();
  await waitFor(() => expect(services.connection.getSnapshot().status).toBe("connected"), { timeout: 10_000 });
}, 30_000);

afterAll(async () => {
  services?.connection.stop();
  await daemon?.stop();
  if (home) rmSync(home, { recursive: true, force: true });
});

describe("other AI tools checkbox in the live usage pill", () => {
  it("turns on and off against the daemon, naming the missing reader instead of checking forever", async () => {
    render(
      <ServicesProvider services={services}>
        <UsagePill />
      </ServicesProvider>,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole("group", { name: "Daily usage" }).querySelector("summary") as HTMLElement);

    await waitFor(() => expect(checkbox().disabled).toBe(false));
    expect(checkbox().checked).toBe(false);

    await user.click(checkbox());
    expect(checkbox().checked).toBe(true);
    // The reader answers within the checking interval, so "Checking" gives way to the missing-reader line.
    await waitFor(() => expect(screen.queryByText("Checking local tools now.")).toBeNull(), { timeout: 8_000 });
    expect(screen.getByTestId("top-usage-tools-reader").textContent).toMatch(/not (found|include)/);
    expect(screen.queryByTestId("top-usage-tools-source")).toBeNull();

    await user.click(checkbox());
    await waitFor(() => expect(checkbox().checked).toBe(false));
    expect(screen.queryByTestId("top-usage-tools-reader")).toBeNull();
  }, 60_000);
});
