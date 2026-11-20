import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AmbientBackground, AmbientBackgroundControl } from "./AmbientBackground";
import { AMBIENT_STORAGE_KEY, createAmbientSettings, type AmbientStorage } from "./settings";

const originalMatchMedia = window.matchMedia;

beforeEach(() => {
  window.localStorage.clear();
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
});

afterEach(() => {
  vi.restoreAllMocks();
  window.localStorage.clear();
  window.matchMedia = originalMatchMedia;
});

const throwing = (): AmbientStorage => ({
  getItem: () => {
    throw new Error("blocked");
  },
  setItem: () => {
    throw new Error("blocked");
  },
});

function setReducedMotion(matches: boolean) {
  window.matchMedia = vi.fn().mockReturnValue({
    matches,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }) as unknown as typeof window.matchMedia;
}

describe("settings", () => {
  it("is on by default and writes nothing until the user chooses", () => {
    expect(createAmbientSettings().getSnapshot()).toEqual({ enabled: true, persisted: true });
    expect(window.localStorage.length).toBe(0);
  });

  it("only a stored 'off' turns it off; any other stored value keeps the default", () => {
    window.localStorage.setItem(AMBIENT_STORAGE_KEY, "off");
    expect(createAmbientSettings().getSnapshot().enabled).toBe(false);
    for (const value of ["on", "true", "1", "OFF", ""]) {
      window.localStorage.setItem(AMBIENT_STORAGE_KEY, value);
      expect(createAmbientSettings().getSnapshot().enabled, value).toBe(true);
    }
  });

  it("persists both choices, so a chosen 'off' survives a reload", () => {
    const settings = createAmbientSettings();
    settings.setEnabled(false);
    expect(window.localStorage.getItem(AMBIENT_STORAGE_KEY)).toBe("off");
    expect(createAmbientSettings().getSnapshot()).toEqual({ enabled: false, persisted: true });
    settings.setEnabled(true);
    expect(window.localStorage.getItem(AMBIENT_STORAGE_KEY)).toBe("on");
    expect(createAmbientSettings().getSnapshot()).toEqual({ enabled: true, persisted: true });
  });

  it("applies the choice for the session when storage throws, and says it was not saved", () => {
    const settings = createAmbientSettings(throwing());
    expect(settings.getSnapshot().enabled).toBe(true);
    settings.setEnabled(false);
    expect(settings.getSnapshot()).toEqual({ enabled: false, persisted: false });
  });

  it("works with no storage at all", () => {
    const settings = createAmbientSettings(null);
    expect(settings.getSnapshot().enabled).toBe(true);
    settings.setEnabled(false);
    expect(settings.getSnapshot()).toEqual({ enabled: false, persisted: false });
  });

  it("notifies subscribers until they unsubscribe", () => {
    const settings = createAmbientSettings(null);
    const listener = vi.fn();
    const unsubscribe = settings.subscribe(listener);
    settings.setEnabled(false);
    unsubscribe();
    settings.setEnabled(true);
    expect(listener).toHaveBeenCalledTimes(1);
  });
});

describe("layer and control", () => {
  it("is on by default: the layer mounts without a click and the switch says On", () => {
    const settings = createAmbientSettings();
    const loadOcean = vi.fn();
    render(
      <>
        <AmbientBackgroundControl settings={settings} />
        <AmbientBackground settings={settings} hasGpu={() => false} loadOcean={loadOcean} />
      </>,
    );
    expect(screen.getByRole("button", { name: /background/i })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("ambient-background").dataset.engine).toBe("canvas2d");
    expect(document.documentElement).toHaveAttribute("data-ambient-background", "on");
    expect(loadOcean).not.toHaveBeenCalled();
  });

  it("renders no layer and loads no renderer when the stored choice is off", () => {
    window.localStorage.setItem(AMBIENT_STORAGE_KEY, "off");
    const loadOcean = vi.fn();
    render(<AmbientBackground settings={createAmbientSettings()} loadOcean={loadOcean} hasGpu={() => true} />);
    expect(screen.queryByTestId("ambient-background")).toBeNull();
    expect(document.documentElement).not.toHaveAttribute("data-ambient-background");
    expect(loadOcean).not.toHaveBeenCalled();
  });

  it("the switch turns the layer off and on, and the choice persists", async () => {
    const user = userEvent.setup();
    const settings = createAmbientSettings();
    render(
      <>
        <AmbientBackgroundControl settings={settings} />
        <AmbientBackground settings={settings} hasGpu={() => false} />
      </>,
    );
    const toggle = screen.getByRole("button", { name: /background/i });
    expect(toggle).toHaveAttribute("aria-pressed", "true");

    await user.click(toggle);
    expect(screen.queryByTestId("ambient-background")).toBeNull();
    expect(document.documentElement).not.toHaveAttribute("data-ambient-background");
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    expect(window.localStorage.getItem(AMBIENT_STORAGE_KEY)).toBe("off");

    await user.click(toggle);
    const layer = screen.getByTestId("ambient-background");
    expect(layer).toHaveAttribute("aria-hidden", "true");
    expect(layer.parentElement).toBe(document.body);
    expect(layer.dataset.engine).toBe("canvas2d");
    expect(document.documentElement).toHaveAttribute("data-ambient-background", "on");
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    expect(window.localStorage.getItem(AMBIENT_STORAGE_KEY)).toBe("on");
  });

  it("starts on when the stored choice is on, and unmounting removes the layer", () => {
    window.localStorage.setItem(AMBIENT_STORAGE_KEY, "on");
    const { unmount } = render(<AmbientBackground settings={createAmbientSettings()} hasGpu={() => false} />);
    expect(screen.getByTestId("ambient-background")).toBeInTheDocument();
    unmount();
    expect(screen.queryByTestId("ambient-background")).toBeNull();
    expect(document.documentElement).not.toHaveAttribute("data-ambient-background");
  });

  it("under reduced motion the default-on layer draws one still field instead of animating", () => {
    setReducedMotion(true);
    render(<AmbientBackground settings={createAmbientSettings()} hasGpu={() => false} />);
    expect(screen.getByTestId("ambient-background").dataset.engine).toBe("static");
    expect(document.documentElement).toHaveAttribute("data-ambient-background", "on");
  });

  it("keeps the Canvas 2D layer, with no error shown, when the WebGPU renderer cannot load", async () => {
    const loadOcean = vi.fn().mockRejectedValue(new Error("renderer chunk failed"));
    render(<AmbientBackground settings={createAmbientSettings()} hasGpu={() => true} loadOcean={loadOcean} />);
    await waitFor(() => expect(loadOcean).toHaveBeenCalledTimes(1));
    await act(async () => {});
    expect(screen.getByTestId("ambient-background").dataset.engine).toBe("canvas2d");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("states that the animation is decorative and includes both license texts", () => {
    render(<AmbientBackgroundControl settings={createAmbientSettings(null)} />);
    expect(screen.getByText(/decorative only/i)).toBeInTheDocument();
    expect(screen.getByLabelText("Agent-Native license")).toHaveTextContent("Copyright (c) 2026 Builder.io, Inc.");
    expect(screen.getByLabelText("vgpu license")).toHaveTextContent("Copyright (c) 2025 Vercel, Inc.");
  });

  it("says so when the choice could not be saved", async () => {
    const user = userEvent.setup();
    render(<AmbientBackgroundControl settings={createAmbientSettings(throwing())} />);
    await user.click(screen.getByRole("button", { name: /background/i }));
    expect(screen.getByRole("button", { name: /background/i })).toHaveTextContent("Off (this session)");
  });

  it("mentions reduced motion when the preference matches", () => {
    setReducedMotion(true);
    act(() => {
      render(<AmbientBackgroundControl settings={createAmbientSettings(null)} />);
    });
    expect(screen.getByText(/reduced motion is on/i)).toBeInTheDocument();
  });
});
