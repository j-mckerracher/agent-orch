import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../../../App";
import { createServices } from "../../createServices";
import { THEME_STORAGE_KEY } from "../../theme";
import { settingsGlobalViews } from "./register";

beforeEach(() => {
  // The ambient layer's switch is part of the section; jsdom has no 2D canvas.
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  window.localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  window.localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  window.location.hash = "";
});

function renderSettings() {
  const services = createServices({ initialRoute: { name: "global", view: "settings" } });
  render(<App services={services} globalViews={settingsGlobalViews} />);
}

describe("Settings > Appearance", () => {
  it("is where the theme toggle lives now: reachable in Settings, absent from the shell", async () => {
    renderSettings();
    const section = await screen.findByTestId("settings-appearance");
    expect(within(section).getByRole("heading", { level: 2, name: "Appearance" })).toBeInTheDocument();
    expect(within(section).getByRole("button", { name: /dark mode/i })).toBeInTheDocument();
    expect(within(screen.getByTestId("shell-sidebar")).queryByRole("button", { name: /dark mode/i })).toBeNull();
  });

  it("switches the theme, applies it to the document and persists it as before", async () => {
    const user = userEvent.setup();
    renderSettings();
    const toggle = within(await screen.findByTestId("settings-appearance")).getByRole("button", { name: /dark mode/i });
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("dark");
    await user.click(toggle);
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("light");
  });
});
