import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DesktopUpdates } from "./DesktopUpdates";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const THIRTY_MINUTES = 30 * 60 * 1000;
const INSTALL = "Install and restart";

type Window = typeof globalThis & { __TAURI_INTERNALS__?: object };

function answer(check: () => unknown) {
  invoke.mockImplementation(async (command: string) => {
    if (command === "desktop_update_check") return check();
  });
}

async function settle() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(0);
  });
}

async function mount() {
  render(<DesktopUpdates />);
  await settle();
}

const checkCalls = () => invoke.mock.calls.filter(([command]) => command === "desktop_update_check").length;

beforeEach(() => {
  vi.useFakeTimers();
  (globalThis as Window).__TAURI_INTERNALS__ = {};
  invoke.mockReset();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  delete (globalThis as Window).__TAURI_INTERNALS__;
});

describe("DesktopUpdates", () => {
  it("renders nothing when the build has updates disabled", async () => {
    answer(() => ({ kind: "disabled" }));
    await mount();
    expect(checkCalls()).toBe(1);
    expect(screen.queryByTestId("desktop-updates")).toBeNull();
  });

  it("renders nothing and calls no command outside the desktop app", async () => {
    delete (globalThis as Window).__TAURI_INTERNALS__;
    await mount();
    expect(invoke).not.toHaveBeenCalled();
    expect(screen.queryByTestId("desktop-updates")).toBeNull();
  });

  it("shows only the manual check when the app is current, then confirms it is up to date", async () => {
    answer(() => ({ kind: "current", version: "0.1.28" }));
    await mount();
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
    expect(screen.queryByRole("button", { name: INSTALL })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    await settle();
    expect(screen.getByRole("status")).toHaveTextContent("MCAO 0.1.28 is up to date.");
  });

  it("offers the available version with Install and restart and Later", async () => {
    answer(() => ({ kind: "available", version: "0.1.29" }));
    await mount();
    expect(screen.getByRole("status")).toHaveTextContent("MCAO 0.1.29 is available.");
    expect(screen.getByRole("button", { name: INSTALL })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Later" })).toBeEnabled();
  });

  it("hides a dismissed version until a manual check or a higher version offers it", async () => {
    let offered = "0.1.29";
    answer(() => ({ kind: "available", version: offered }));
    await mount();

    fireEvent.click(screen.getByRole("button", { name: "Later" }));
    expect(screen.queryByRole("button", { name: INSTALL })).toBeNull();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(THIRTY_MINUTES);
    });
    expect(checkCalls()).toBe(2);
    expect(screen.queryByRole("button", { name: INSTALL })).toBeNull();

    offered = "0.1.30";
    await act(async () => {
      await vi.advanceTimersByTimeAsync(THIRTY_MINUTES);
    });
    expect(screen.getByRole("status")).toHaveTextContent("MCAO 0.1.30 is available.");

    fireEvent.click(screen.getByRole("button", { name: "Later" }));
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    await settle();
    expect(screen.getByRole("status")).toHaveTextContent("MCAO 0.1.30 is available.");
  });

  it("installs the offered version once and locks the buttons while preparing", async () => {
    let finish: () => void = () => undefined;
    invoke.mockImplementation((command: string) =>
      command === "desktop_update_check"
        ? Promise.resolve({ kind: "available", version: "0.1.29" })
        : new Promise<void>((resolve) => (finish = resolve)),
    );
    await mount();

    fireEvent.click(screen.getByRole("button", { name: INSTALL }));
    await settle();
    expect(invoke).toHaveBeenLastCalledWith("desktop_update_install", { version: "0.1.29" });
    expect(screen.getByRole("status")).toHaveTextContent("Preparing update…");
    expect(screen.getByRole("button", { name: INSTALL })).toHaveAttribute("aria-busy", "true");
    for (const name of ["Later", "Check for updates"]) {
      expect(screen.getByRole("button", { name })).toBeDisabled();
    }
    finish();
  });

  it("Check for updates shows pending and 'Checking for updates…' until the check answers", async () => {
    let finish: (value: unknown) => void = () => undefined;
    invoke.mockImplementation((command: string) =>
      command === "desktop_update_check" && checkCalls() > 1
        ? new Promise((resolve) => (finish = resolve))
        : Promise.resolve({ kind: "current", version: "0.1.28" }),
    );
    await mount();

    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(screen.getByRole("button", { name: "Check for updates" })).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("status")).toHaveTextContent("Checking for updates…");

    await act(async () => finish({ kind: "current", version: "0.1.28" }));
    await settle();
    expect(screen.getByRole("status")).toHaveTextContent("MCAO 0.1.28 is up to date.");
    expect(screen.getByRole("button", { name: "Check for updates" })).not.toHaveAttribute("aria-busy");
  });

  it("keeps the offer and reports a busy daemon without retrying the install", async () => {
    invoke.mockImplementation(async (command: string) => {
      if (command === "desktop_update_check") return { kind: "available", version: "0.1.29" };
      throw { kind: "daemon_busy" };
    });
    await mount();

    fireEvent.click(screen.getByRole("button", { name: INSTALL }));
    await settle();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Work is still running. Finish it, then try the update again.",
    );
    expect(screen.getByRole("status")).toHaveTextContent("MCAO 0.1.29 is available.");
    expect(screen.getByRole("button", { name: INSTALL })).toBeEnabled();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(THIRTY_MINUTES);
    });
    expect(invoke.mock.calls.filter(([command]) => command === "desktop_update_install")).toHaveLength(1);
  });

  it.each([
    ["gh_missing", "Install GitHub CLI"],
    [
      "github_unavailable",
      "Can't check GitHub Releases. Verify your connection and run gh auth login --hostname github.com if needed.",
    ],
    ["download_failed", "The update could not be downloaded. Try again."],
    ["something_new", "The update failed. Try again."],
  ])("shows %s from a manual check as a non-blocking alert", async (kind, text) => {
    invoke.mockImplementation(async (command: string) => {
      if (command === "desktop_update_check" && checkCalls() > 1) throw { kind };
      return { kind: "current", version: "0.1.28" };
    });
    await mount();

    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    await settle();
    expect(screen.getByRole("alert")).toHaveTextContent(text);
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeEnabled();
  });

  it("stays quiet when a background check fails", async () => {
    invoke.mockRejectedValue({ kind: "gh_missing" });
    await mount();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByTestId("desktop-updates")).toBeNull();
  });

  it("skips a check while another is running", async () => {
    let resolve: (value: unknown) => void = () => undefined;
    invoke.mockImplementation(() => new Promise((done) => (resolve = done)));
    render(<DesktopUpdates />);
    expect(checkCalls()).toBe(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(THIRTY_MINUTES);
    });
    expect(checkCalls()).toBe(1);

    await act(async () => {
      resolve({ kind: "current", version: "0.1.28" });
    });
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(checkCalls()).toBe(2);
  });

  it("does not check on the interval while the window is hidden", async () => {
    answer(() => ({ kind: "current", version: "0.1.28" }));
    await mount();
    const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(THIRTY_MINUTES);
    });
    expect(checkCalls()).toBe(1);
    visibility.mockRestore();
  });

  it("clears its timer on unmount", async () => {
    answer(() => ({ kind: "current", version: "0.1.28" }));
    render(<DesktopUpdates />);
    await settle();
    expect(vi.getTimerCount()).toBe(1);

    cleanup();
    expect(vi.getTimerCount()).toBe(0);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(THIRTY_MINUTES);
    });
    expect(checkCalls()).toBe(1);
  });
});
