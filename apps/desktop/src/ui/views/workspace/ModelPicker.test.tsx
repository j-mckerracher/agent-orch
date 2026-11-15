import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ModelChoice, ModelProvider, SurfaceResult } from "../../../sources/surfaces/types";
import { ModelPicker } from "./ModelPicker";

const observed = { observedAt: "2026-11-15T10:00:00Z", ageMs: 0 };
const provenance = { kind: "daemon", id: "D14" } as const;

function provider(id: string, name: string): ModelProvider {
  return {
    id,
    name,
    installed: true,
    launchable: true,
    modelSelection: "supported",
    models: { status: "ok", value: [{ id: "m1", label: "Model one" }], ...observed },
    efforts: { status: "ok", value: ["low", "high"], ...observed },
  };
}

function catalog(providers: ModelProvider[]): SurfaceResult<{ providers: ModelProvider[] }> {
  return { status: "ok", data: { providers }, provenance, ...observed };
}

describe("ModelPicker in a live chat", () => {
  it("lists the agents chat cannot run as unavailable for chat, and never defaults to them", async () => {
    const user = userEvent.setup();
    render(
      <ModelPicker
        catalog={catalog([provider("github-copilot", "GitHub Copilot"), provider("pi", "Pi"), provider("codex", "Codex")])}
        provenance={provenance}
      />,
    );
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    const select = screen.getByRole("combobox", { name: "Provider" });
    expect(within(select).getByRole("option", { name: "GitHub Copilot (not available for chat)" })).toBeDisabled();
    expect(within(select).getByRole("option", { name: "Pi (not available for chat)" })).toBeDisabled();
    expect(within(select).getByRole("option", { name: "Codex" })).toBeEnabled();
    expect(select).toHaveValue("codex");
  });

  it("reports nothing until the user changes a part of the choice, then the displayed choice", async () => {
    const user = userEvent.setup();
    const onChoiceChange = vi.fn<(choice: ModelChoice | null) => void>();
    render(
      <ModelPicker
        catalog={catalog([provider("codex", "Codex"), provider("claude-code", "Claude Code")])}
        provenance={provenance}
        onChoiceChange={onChoiceChange}
      />,
    );
    expect(onChoiceChange).toHaveBeenLastCalledWith(null);
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    await user.selectOptions(screen.getByRole("combobox", { name: "Provider" }), "claude-code");
    // The agent alone: the model and effort the picker shows as defaults are not part of the choice.
    await waitFor(() => expect(onChoiceChange).toHaveBeenLastCalledWith({ agent: "claude-code" }));
    await user.selectOptions(screen.getByRole("combobox", { name: "Model" }), "m1");
    await waitFor(() => expect(onChoiceChange).toHaveBeenLastCalledWith({ agent: "claude-code", model: "m1" }));
    const effort = screen.getByRole("combobox", { name: "Effort" });
    await waitFor(() => expect(effort).toBeEnabled());
    await user.selectOptions(effort, "high");
    await waitFor(() =>
      expect(onChoiceChange).toHaveBeenLastCalledWith({ agent: "claude-code", model: "m1", effort: "high" }),
    );
  });
});
