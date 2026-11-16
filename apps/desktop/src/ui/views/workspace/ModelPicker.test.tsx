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

describe("ModelPicker permission level in a chat", () => {
  it("shows the level at the end of the chip and offers the four levels in the panel", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <ModelPicker
        catalog={catalog([provider("codex", "Codex")])}
        provenance={provenance}
        permission={{ level: "auto", daemonAdvertises: true, onChange }}
      />,
    );
    const chip = screen.getByRole("button", { name: "Model settings: Codex · default model · default effort · Auto" });
    expect(chip).toHaveTextContent("Codex · default · Auto");
    await user.click(chip);
    const level = screen.getByRole("combobox", { name: "Permission level" });
    expect(level).toHaveValue("auto");
    expect(within(level).getAllByRole("option").map((option) => option.textContent)).toEqual([
      "Restricted",
      "Extended (not available for chat yet)",
      "Auto",
      "Full",
    ]);
    expect(within(level).getByRole("option", { name: "Extended (not available for chat yet)" })).toBeDisabled();
    await user.selectOptions(level, "Restricted");
    expect(onChange).toHaveBeenCalledWith("restricted");
  });

  it("asks before Full, and on Cancel returns focus to the chip with the panel closed", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <ModelPicker
        catalog={catalog([provider("codex", "Codex")])}
        provenance={provenance}
        permission={{ level: "auto", daemonAdvertises: true, onChange }}
      />,
    );
    const chip = screen.getByRole("button", { name: /^Model settings/ });
    await user.click(chip);
    await user.selectOptions(screen.getByRole("combobox", { name: "Permission level" }), "Full");
    await user.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(onChange).not.toHaveBeenCalled();
    expect(chip).toHaveFocus();
    expect(chip).toHaveAttribute("aria-expanded", "false");
  });

  it("reads the model and effort in the chip as chosen or default, with the full words in the accessible name", async () => {
    const user = userEvent.setup();
    render(
      <ModelPicker
        catalog={catalog([provider("codex", "Codex")])}
        provenance={provenance}
        permission={{ level: "auto", daemonAdvertises: true, onChange: vi.fn() }}
      />,
    );
    const chip = () => screen.getByRole("button", { name: /^Model settings/ });
    // Both default: one word covers both.
    expect(chip()).toHaveTextContent("Codex · default · Auto");
    expect(chip()).toHaveAccessibleName("Model settings: Codex · default model · default effort · Auto");
    await user.click(chip());
    // The default model with a chosen effort: "default" names the model, and the effort is the one chosen.
    await user.selectOptions(screen.getByRole("combobox", { name: "Effort" }), "high");
    await waitFor(() => expect(chip()).toHaveTextContent("Codex · default · high · Auto"));
    expect(chip()).toHaveAccessibleName("Model settings: Codex · default model · high · Auto");
    // A chosen model keeps its label. Choosing a model clears the effort, so the default effort shows its words.
    await user.selectOptions(screen.getByRole("combobox", { name: "Model" }), "m1");
    await waitFor(() => expect(chip()).toHaveTextContent("Codex · Model one · default effort · Auto"));
    expect(chip()).toHaveAccessibleName("Model settings: Codex · Model one · default effort · Auto");
    // Both chosen.
    await user.selectOptions(screen.getByRole("combobox", { name: "Effort" }), "high");
    await waitFor(() => expect(chip()).toHaveTextContent("Codex · Model one · high · Auto"));
    expect(chip()).toHaveAccessibleName("Model settings: Codex · Model one · high · Auto");
  });

  it.each([
    ["restricted", "Restricted"],
    ["extended", "Extended"],
    ["auto", "Auto"],
    ["full", "Full"],
  ] as const)("shows the %s level as its label at the end of the chip", (level, label) => {
    render(
      <ModelPicker
        catalog={catalog([provider("codex", "Codex")])}
        provenance={provenance}
        permission={{ level, daemonAdvertises: true, onChange: vi.fn() }}
      />,
    );
    const chip = screen.getByRole("button", { name: /^Model settings/ });
    expect(chip).toHaveTextContent(`Codex · default · ${label}`);
    expect(chip).toHaveAccessibleName(`Model settings: Codex · default model · default effort · ${label}`);
  });

  it("shows a fixture agent without its placeholder qualifier, and keeps the qualifier in the accessible name", () => {
    const fixture = { kind: "fixture", id: "D14", fixture: "model-catalog" } as const;
    const fixtureCatalog: SurfaceResult<{ providers: ModelProvider[] }> = {
      status: "ok",
      data: { providers: [provider("codex", "Codex (placeholder)")] },
      provenance: fixture,
      ...observed,
    };
    render(
      <ModelPicker
        catalog={fixtureCatalog}
        provenance={fixture}
        permission={{ level: "auto", daemonAdvertises: true, onChange: vi.fn() }}
      />,
    );
    const chip = screen.getByRole("button", { name: /^Model settings/ });
    expect(within(chip).getByText("Codex · default · high")).toBeInTheDocument();
    expect(within(chip).getByText("PLACEHOLDER (D14)")).toBeInTheDocument();
    expect(chip).toHaveAccessibleName(
      "Model settings: Codex (placeholder) · default model · high · Auto PLACEHOLDER (D14)",
    );
  });

  it("shows no level, and offers no level control, where no message runs", async () => {
    const user = userEvent.setup();
    render(<ModelPicker catalog={catalog([provider("codex", "Codex")])} provenance={provenance} />);
    await user.click(screen.getByRole("button", { name: "Model settings: Codex · default model · default effort" }));
    expect(screen.getByRole("combobox", { name: "Provider" })).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Permission level" })).toBeNull();
  });
});
