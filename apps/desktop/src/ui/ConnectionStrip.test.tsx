import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { DaemonError } from "../daemon/errors";
import { createFixtureSources } from "../sources/fixtures";
import { FakeDaemon } from "../test/fakeDaemon";
import { ConnectionStrip } from "./ConnectionStrip";
import { createServices } from "./createServices";
import { createLayoutSettings, LAYOUT_STORAGE_KEY, type LayoutSettings, type LayoutStorage } from "./layout/layoutSettings";
import { ServicesProvider, type Services } from "./services";

const COLLAPSE = "Collapse connection details";
const EXPAND = "Expand connection details";
const TOGGLE = /^(Collapse|Expand) connection details$/;

function memoryStorage(initial: Record<string, string> = {}): LayoutStorage & { data: Record<string, string> } {
  const data = { ...initial };
  return { data, getItem: (key) => data[key] ?? null, setItem: (key, value) => void (data[key] = value) };
}

const throwingStorage: LayoutStorage = {
  getItem: () => {
    throw new DOMException("denied", "SecurityError");
  },
  setItem: () => {
    throw new DOMException("quota", "QuotaExceededError");
  },
};

const started: Services[] = [];
afterEach(() => {
  for (const services of started.splice(0)) services.connection.stop();
});

interface SetupOptions {
  fake?: FakeDaemon;
  bridge?: boolean;
  layout?: LayoutSettings;
  connection?: { maxAutoAttempts?: number; backoffMs?: number[] };
}

/** Renders the strip alone, over real services, and starts the connection when a bridge is bound. */
function setup({ fake = new FakeDaemon(), bridge = true, layout = createLayoutSettings(memoryStorage()), connection }: SetupOptions = {}) {
  const services = createServices({
    connector: bridge ? fake : undefined,
    sources: createFixtureSources(),
    connection: { requestTimeoutMs: 250, backoffMs: [5], ...connection },
  });
  started.push(services);
  render(
    <ServicesProvider services={services}>
      <ConnectionStrip layout={layout} />
    </ServicesProvider>,
  );
  if (bridge) services.connection.start();
  return { services, fake, layout, user: userEvent.setup() };
}

const toggle = () => screen.getByRole("button", { name: TOGGLE });
const connected = () => waitFor(() => expect(screen.getByTestId("connection-status")).toHaveTextContent("Connected"));

/** Asserts the strip is locked open: expanded, the toggle disabled, and the visible reason linked to it. */
function expectLocked(reason: string) {
  const control = toggle();
  expect(control).toHaveAttribute("aria-expanded", "true");
  expect(control).toHaveAccessibleName(COLLAPSE);
  expect(control).toBeDisabled();
  const reasonId = control.getAttribute("aria-describedby");
  expect(reasonId).toBeTruthy();
  const reasonNode = document.getElementById(reasonId!);
  expect(reasonNode).toHaveTextContent(reason);
  expect(reasonNode).toBeVisible();
  expect(screen.getByTestId("recovery-unknown")).toBeVisible();
  expect(screen.getByTestId("journal-continuity")).toBeVisible();
}

describe("ConnectionStrip collapse (DPES-1105)", () => {
  it("is expanded by default and shows the explanatory notes", async () => {
    setup();
    await connected();
    expect(toggle()).toHaveAttribute("aria-expanded", "true");
    expect(toggle()).toHaveAccessibleName(COLLAPSE);
    expect(screen.getByTestId("connected-note")).toBeVisible();
    expect(screen.getByTestId("recovery-unknown")).toBeVisible();
    expect(screen.getByTestId("journal-continuity")).toBeVisible();
  });

  it("collapses to a one-line summary and folds only the explanatory notes", async () => {
    const { user, layout } = setup();
    await connected();
    await user.click(toggle());
    expect(toggle()).toHaveAccessibleName(EXPAND);
    expect(toggle()).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByTestId("connection-status")).toBeVisible();
    expect(screen.getByTestId("last-sequence")).toBeVisible();
    expect(screen.getByRole("button", { name: "Refresh / reconnect" })).toBeVisible();
    expect(screen.getByTestId("connected-note")).not.toBeVisible();
    expect(screen.getByTestId("recovery-unknown")).not.toBeVisible();
    expect(screen.getByTestId("journal-continuity")).not.toBeVisible();
    expect(layout.getSnapshot().connectionStripExpanded).toBe(false);
  });

  it("keeps the compact summary in the toggle's row: status, last durable sequence and reconnect", async () => {
    const { user } = setup();
    await connected();
    await user.click(toggle());
    const row = toggle().parentElement!;
    expect(row).toHaveTextContent("Connected");
    expect(row).toHaveTextContent(/Last durable sequence: \w+/);
    expect(row).toHaveTextContent("Refresh / reconnect");
  });

  it("ties the toggle to the region it controls", async () => {
    setup();
    await connected();
    const region = document.getElementById(toggle().getAttribute("aria-controls") ?? "");
    expect(region).toBe(screen.getByTestId("connection-details"));
    expect(region).toBeVisible();
  });

  it("works from the keyboard with Enter and Space", async () => {
    const { user } = setup();
    await connected();
    toggle().focus();
    await user.keyboard("{Enter}");
    expect(toggle()).toHaveAttribute("aria-expanded", "false");
    await user.keyboard(" ");
    expect(toggle()).toHaveAttribute("aria-expanded", "true");
  });

  it("keeps the fixture demo banner visible while the strip is collapsed", async () => {
    const { user } = setup({ bridge: false });
    expect(toggle()).toHaveAttribute("aria-expanded", "true");
    await user.click(toggle());
    expect(toggle()).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByTestId("fixture-banner")).toBeVisible();
    expect(screen.getByTestId("fixture-banner")).toHaveTextContent("FIXTURE DEMO MODE");
    expect(screen.getByTestId("recovery-unknown")).not.toBeVisible();
  });

  it("locks open while the connection is reconnecting, and restores the choice when it returns", async () => {
    const fake = new FakeDaemon();
    const { user } = setup({ fake, connection: { maxAutoAttempts: 100, backoffMs: [60_000] } });
    await connected();
    await user.click(toggle());
    fake.openError = new DaemonError("disconnected", "down");
    fake.drop();
    await waitFor(() => expect(screen.getByTestId("connection-status")).toHaveTextContent("Reconnecting..."));
    expectLocked("Kept open while the connection needs attention.");

    fake.openError = null;
    await user.click(screen.getByRole("button", { name: "Refresh / reconnect" }));
    await connected();
    expect(toggle()).toHaveAttribute("aria-expanded", "false");
    expect(toggle()).not.toBeDisabled();
  });

  it("locks open when the daemon is unavailable (no bridge answer)", async () => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError("daemon-unavailable", "the daemon is not running");
    setup({ fake, connection: { maxAutoAttempts: 0 } });
    await waitFor(() => expect(screen.getByTestId("connection-status")).toHaveTextContent("Daemon unavailable"));
    expectLocked("Kept open while the connection needs attention.");
    expect(screen.getByTestId("offline-note")).toBeVisible();
  });

  it("locks open on an authentication failure", async () => {
    const fake = new FakeDaemon();
    const { user } = setup({ fake });
    await connected();
    await user.click(toggle());
    fake.openError = new DaemonError("auth", "bad token", -32001);
    fake.drop();
    await waitFor(() => expect(screen.getByTestId("connection-status")).toHaveTextContent("Authentication failed"));
    expectLocked("Kept open while the connection needs attention.");
  });

  it("locks open while history is incomplete even though the connection is up, and keeps the alert out of the folding region", async () => {
    const fake = new FakeDaemon();
    const { user } = setup({ fake });
    await connected();
    await user.click(toggle());
    fake.pushStreamError(-32015, "slow");
    const alert = await screen.findByTestId("history-incomplete");
    await connected();
    expectLocked("Kept open until history is refreshed.");
    expect(alert).toHaveAttribute("role", "alert");
    expect(alert).toBeVisible();
    expect(document.getElementById("connection-details")!.contains(alert)).toBe(false);

    fake.openError = null;
    await user.click(screen.getByRole("button", { name: "Refresh / reconnect" }));
    await waitFor(() => expect(screen.queryByTestId("history-incomplete")).not.toBeInTheDocument());
    expect(toggle()).toHaveAttribute("aria-expanded", "false");
  });

  it("keeps the status pill announced while collapsed: it stays visible and live when the state changes", async () => {
    const fake = new FakeDaemon();
    const { user } = setup({ fake, connection: { maxAutoAttempts: 100, backoffMs: [60_000] } });
    await connected();
    await user.click(toggle());
    const pill = screen.getByTestId("connection-status");
    expect(pill).toHaveAttribute("role", "status");
    expect(pill.closest("[hidden]")).toBeNull();
    expect(pill).toBeVisible();

    fake.openError = new DaemonError("disconnected", "down");
    fake.drop();
    await waitFor(() => expect(screen.getByTestId("connection-status")).toHaveTextContent("Reconnecting..."));
    expect(screen.getByTestId("connection-status")).toBe(pill);
    expect(pill).toBeVisible();
  });

  it("keeps a collapsed choice across a remount when storage works", async () => {
    const storage = memoryStorage();
    const first = setup({ layout: createLayoutSettings(storage) });
    await connected();
    await first.user.click(toggle());
    expect(JSON.parse(storage.data[LAYOUT_STORAGE_KEY]!)).toMatchObject({ connectionStripExpanded: false });

    cleanup();
    setup({ layout: createLayoutSettings(storage) });
    await waitFor(() => expect(toggle()).toHaveAttribute("aria-expanded", "false"));
  });

  it("applies the collapse for the page and reports not persisted when storage throws", async () => {
    const layout = createLayoutSettings(throwingStorage);
    const { user } = setup({ layout, bridge: false });
    await user.click(toggle());
    expect(toggle()).toHaveAttribute("aria-expanded", "false");
    expect(layout.getSnapshot()).toMatchObject({ connectionStripExpanded: false, persisted: false });
  });
});
