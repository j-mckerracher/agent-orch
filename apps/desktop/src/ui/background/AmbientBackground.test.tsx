import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AmbientBackground, AmbientBackgroundControl } from "./AmbientBackground";
import { withAmbientBackground } from "./register";
import { AMBIENT_STORAGE_KEY, createAmbientSettings, type AmbientStorage } from "./settings";

beforeEach(() => {
  window.localStorage.clear();
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
});

afterEach(() => {
  vi.restoreAllMocks();
  window.localStorage.clear();
});

const throwing = (): AmbientStorage => ({
  getItem: () => {
    throw new Error("blocked");
  },
  setItem: () => {
    throw new Error("blocked");
  },
  removeItem: () => {
    throw new Error("blocked");
  },
});

describe("settings", () => {
  it("is off by default and ignores any stored value except 'on'", () => {
    expect(createAmbientSettings().getSnapshot().enabled).toBe(false);
    for (const value of ["true", "1", "ON", "off", ""]) {
      window.localStorage.setItem(AMBIENT_STORAGE_KEY, value);
      expect(createAmbientSettings().getSnapshot().enabled).toBe(false);
    }
  });

  it("persists the choice and restores it", () => {
    const settings = createAmbientSettings();
    settings.setEnabled(true);
    expect(window.localStorage.getItem(AMBIENT_STORAGE_KEY)).toBe("on");
    expect(createAmbientSettings().getSnapshot()).toEqual({ enabled: true, persisted: true });
    settings.setEnabled(false);
    expect(window.localStorage.getItem(AMBIENT_STORAGE_KEY)).toBeNull();
  });

  it("applies the choice for the session when storage throws, and says it was not saved", () => {
    const settings = createAmbientSettings(throwing());
    expect(settings.getSnapshot().enabled).toBe(false);
    settings.setEnabled(true);
    expect(settings.getSnapshot()).toEqual({ enabled: true, persisted: false });
  });

  it("works with no storage at all", () => {
    const settings = createAmbientSettings(null);
    settings.setEnabled(true);
    expect(settings.getSnapshot()).toEqual({ enabled: true, persisted: false });
  });

  it("notifies subscribers until they unsubscribe", () => {
    const settings = createAmbientSettings(null);
    const listener = vi.fn();
    const unsubscribe = settings.subscribe(listener);
    settings.setEnabled(true);
    unsubscribe();
    settings.setEnabled(false);
    expect(listener).toHaveBeenCalledTimes(1);
  });
});

describe("layer and control", () => {
  it("renders no layer and loads no renderer while off", () => {
    const loadOcean = vi.fn();
    render(<AmbientBackground loadOcean={loadOcean} hasGpu={() => true} />);
    expect(screen.queryByTestId("ambient-background")).toBeNull();
    expect(document.documentElement).not.toHaveAttribute("data-ambient-background");
    expect(loadOcean).not.toHaveBeenCalled();
  });

  it("is opt-in: the switch turns the hidden, inert layer on and off and the choice persists", async () => {
    const user = userEvent.setup();
    const settings = createAmbientSettings();
    render(
      <>
        <AmbientBackgroundControl settings={settings} />
        <AmbientBackground settings={settings} hasGpu={() => false} />
      </>,
    );
    const toggle = screen.getByRole("button", { name: /background/i });
    expect(toggle).toHaveAttribute("aria-pressed", "false");

    await user.click(toggle);
    const layer = screen.getByTestId("ambient-background");
    expect(layer).toHaveAttribute("aria-hidden", "true");
    expect(layer.parentElement).toBe(document.body);
    expect(layer.dataset.engine).toBe("canvas2d");
    expect(document.documentElement).toHaveAttribute("data-ambient-background", "on");
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    expect(window.localStorage.getItem(AMBIENT_STORAGE_KEY)).toBe("on");

    await user.click(toggle);
    expect(screen.queryByTestId("ambient-background")).toBeNull();
    expect(document.documentElement).not.toHaveAttribute("data-ambient-background");
    expect(window.localStorage.getItem(AMBIENT_STORAGE_KEY)).toBeNull();
  });

  it("starts on when the stored choice is on, and unmounting removes the layer", () => {
    window.localStorage.setItem(AMBIENT_STORAGE_KEY, "on");
    const { unmount } = render(<AmbientBackground settings={createAmbientSettings()} hasGpu={() => false} />);
    expect(screen.getByTestId("ambient-background")).toBeInTheDocument();
    unmount();
    expect(screen.queryByTestId("ambient-background")).toBeNull();
    expect(document.documentElement).not.toHaveAttribute("data-ambient-background");
  });

  it("states that the animation is decorative and includes both licence texts", () => {
    render(<AmbientBackgroundControl settings={createAmbientSettings(null)} />);
    expect(screen.getByText(/decorative only/i)).toBeInTheDocument();
    expect(screen.getByLabelText("Agent-Native licence")).toHaveTextContent("Copyright (c) 2026 Builder.io, Inc.");
    expect(screen.getByLabelText("vgpu licence")).toHaveTextContent("Copyright (c) 2025 Vercel, Inc.");
  });

  it("says so when the choice could not be saved", async () => {
    const user = userEvent.setup();
    render(<AmbientBackgroundControl settings={createAmbientSettings(throwing())} />);
    await user.click(screen.getByRole("button", { name: /background/i }));
    expect(screen.getByRole("button", { name: /background/i })).toHaveTextContent("On (this session)");
  });

  it("mentions reduced motion when the preference matches", () => {
    const original = window.matchMedia;
    window.matchMedia = vi.fn().mockReturnValue({
      matches: true,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }) as unknown as typeof window.matchMedia;
    try {
      act(() => {
        render(<AmbientBackgroundControl settings={createAmbientSettings(null)} />);
      });
      expect(screen.getByText(/reduced motion is on/i)).toBeInTheDocument();
    } finally {
      window.matchMedia = original;
    }
  });

  it("wires the control and layer next to an existing theme toggle", () => {
    render(<>{withAmbientBackground(<button type="button">Dark mode</button>)}</>);
    expect(screen.getByRole("button", { name: "Dark mode" })).toBeInTheDocument();
    expect(screen.getByTestId("ambient-control")).toBeInTheDocument();
  });
});
