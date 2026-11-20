import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { CopilotSignInRecord } from "../../../daemon/types";
import type { ModelCatalog, ModelChoice, ModelProvider, SurfaceResult } from "../../../sources/surfaces/types";
import type { SurfaceState } from "../../services";
import { CHOICE_PREFERENCE_KEY } from "./modelChoice";
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

/** Copilot with no model or effort list to choose from, as when its model list could not be read. Its listed shape is in modelCatalogLive.test.ts. */
function copilot(overrides: Partial<ModelProvider> = {}): ModelProvider {
  return {
    id: "github-copilot",
    name: "GitHub Copilot",
    installed: true,
    launchable: true,
    modelSelection: "unsupported",
    models: { status: "unsupported", reason: "The Copilot model list could not be read." },
    efforts: { status: "unsupported", reason: "Reasoning effort is not offered for GitHub Copilot." },
    ...overrides,
  };
}

function signIn(state: CopilotSignInRecord["state"], reason: string | null = null): CopilotSignInRecord {
  return { state, reason, checked_at_ms: 1, premium_requests: null, ai_credits: null, warning: null };
}

/** The copilotSignIn source the picker reads: the free status read answers `record`, and a check is never run here. */
function copilotSource(record: CopilotSignInRecord) {
  return {
    checkCopilotSignIn: vi.fn(async () => ({ status: "ok", data: record, provenance, ...observed }) as SurfaceResult<CopilotSignInRecord>),
    copilotSignInStatus: vi.fn(async () => ({ status: "ok", data: record, provenance, ...observed }) as SurfaceResult<CopilotSignInRecord>),
  };
}

describe("ModelPicker in a live chat", () => {
  it("lists a launchable Copilot as choosable, never as the default, and a launchable Codex stays the default", async () => {
    const user = userEvent.setup();
    const pi = provider("pi", "Pi");
    render(
      <ModelPicker
        catalog={catalog([copilot(), { ...pi, launchable: false, detail: "the daemon cannot launch pi" }, provider("codex", "Codex")])}
        provenance={provenance}
      />,
    );
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    const select = screen.getByRole("combobox", { name: "Provider" });
    expect(within(select).getByRole("option", { name: "GitHub Copilot" })).toBeEnabled();
    expect(within(select).getByRole("option", { name: "Pi (not launchable)" })).toBeDisabled();
    expect(within(select).getByRole("option", { name: "Codex" })).toBeEnabled();
    expect(select).toHaveValue("codex");
    expect(screen.getByText("Pi: the daemon cannot launch pi")).toBeInTheDocument();
  });

  it("sends the Copilot choice with the message, and says once that a message uses the user's Copilot seat", async () => {
    const user = userEvent.setup();
    const onChoiceChange = vi.fn<(choice: ModelChoice | null) => void>();
    render(
      <ModelPicker
        catalog={catalog([copilot()])}
        provenance={provenance}
        copilotSignIn={copilotSource(signIn("ready"))}
        onChoiceChange={onChoiceChange}
      />,
    );
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    expect(screen.queryByText(/premium requests or AI credits/)).not.toBeInTheDocument();
    const select = screen.getByRole("combobox", { name: "Provider" });
    expect(select).toHaveValue("");
    await user.selectOptions(select, "github-copilot");
    await waitFor(() => expect(onChoiceChange).toHaveBeenLastCalledWith({ agent: "github-copilot" }));
    expect(screen.getByText("A Copilot message uses premium requests or AI credits on your Copilot seat.")).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Model" })).toBeDisabled();
  });

  it("lists Copilot as not signed in, with the daemon's fix, and does not let it be chosen", async () => {
    const user = userEvent.setup();
    render(
      <ModelPicker
        catalog={catalog([provider("codex", "Codex"), copilot()])}
        provenance={provenance}
        copilotSignIn={copilotSource(signIn("not_signed_in", "Not signed in to GitHub Copilot. Run `copilot login`, then retry."))}
      />,
    );
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    const select = await screen.findByRole("combobox", { name: "Provider" });
    await waitFor(() => expect(within(select).getByRole("option", { name: "GitHub Copilot (not signed in)" })).toBeDisabled());
    expect(screen.getByText(/GitHub Copilot: Not signed in to GitHub Copilot/)).toBeInTheDocument();
    expect(select).toHaveValue("codex");
  });

  it("lists Copilot on an account with no seat, and shows the daemon's reason", async () => {
    const user = userEvent.setup();
    render(
      <ModelPicker
        catalog={catalog([provider("codex", "Codex"), copilot()])}
        provenance={provenance}
        copilotSignIn={copilotSource(signIn("no_seat", "This GitHub account has no GitHub Copilot subscription."))}
      />,
    );
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    const select = await screen.findByRole("combobox", { name: "Provider" });
    await waitFor(() => expect(within(select).getByRole("option", { name: "GitHub Copilot (no Copilot seat)" })).toBeDisabled());
    expect(screen.getByText(/This GitHub account has no GitHub Copilot subscription/)).toBeInTheDocument();
  });

  it("keeps Copilot choosable while sign-in is not confirmed, and says so", async () => {
    const user = userEvent.setup();
    render(
      <ModelPicker
        catalog={catalog([provider("codex", "Codex"), copilot()])}
        provenance={provenance}
        copilotSignIn={copilotSource(signIn("not_checked", "Not confirmed yet: sign-in is checked on your first Copilot use."))}
      />,
    );
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    const select = await screen.findByRole("combobox", { name: "Provider" });
    await waitFor(() => expect(screen.getByText(/GitHub Copilot: Not confirmed yet/)).toBeInTheDocument());
    expect(within(select).getByRole("option", { name: "GitHub Copilot" })).toBeEnabled();
  });

  it("shows Copilot's own reason when its CLI is missing, instead of a blanket not-available message", async () => {
    const user = userEvent.setup();
    render(
      <ModelPicker
        catalog={catalog([provider("codex", "Codex"), copilot({ launchable: false, detail: "the copilot executable was not found" })])}
        provenance={provenance}
      />,
    );
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    const select = screen.getByRole("combobox", { name: "Provider" });
    expect(within(select).getByRole("option", { name: "GitHub Copilot (not launchable)" })).toBeDisabled();
    expect(screen.getByText("GitHub Copilot: the copilot executable was not found")).toBeInTheDocument();
    expect(screen.queryByText(/not available for chat/)).not.toBeInTheDocument();
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

describe("ModelPicker saved choice (DPES-1105)", () => {
  const savedValue = (): unknown => JSON.parse(window.localStorage.getItem(CHOICE_PREFERENCE_KEY) ?? "null");
  const save = (value: ModelChoice) => window.localStorage.setItem(CHOICE_PREFERENCE_KEY, JSON.stringify(value));
  const fixture = { kind: "fixture", id: "D14", fixture: "model-catalog" } as const;

  it("restores the saved choice on mount, before any default, and sends it as the user's own choice", () => {
    save({ agent: "claude-code", model: "m1", effort: "high" });
    const onChoiceChange = vi.fn<(choice: ModelChoice | null) => void>();
    render(
      <ModelPicker
        catalog={catalog([provider("codex", "Codex"), provider("claude-code", "Claude Code")])}
        provenance={provenance}
        onChoiceChange={onChoiceChange}
      />,
    );
    expect(screen.getByRole("button", { name: "Model settings: Claude Code · Model one · high" })).toBeInTheDocument();
    expect(onChoiceChange).toHaveBeenLastCalledWith({ agent: "claude-code", model: "m1", effort: "high" });
  });

  it("saves each change, and saves nothing until the user changes a part", async () => {
    const user = userEvent.setup();
    render(<ModelPicker catalog={catalog([provider("codex", "Codex"), provider("claude-code", "Claude Code")])} provenance={provenance} />);
    expect(window.localStorage.getItem(CHOICE_PREFERENCE_KEY)).toBeNull();
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    await user.selectOptions(screen.getByRole("combobox", { name: "Provider" }), "claude-code");
    expect(savedValue()).toEqual({ agent: "claude-code" });
    await user.selectOptions(screen.getByRole("combobox", { name: "Model" }), "m1");
    expect(savedValue()).toEqual({ agent: "claude-code", model: "m1" });
    await user.selectOptions(screen.getByRole("combobox", { name: "Effort" }), "high");
    expect(savedValue()).toEqual({ agent: "claude-code", model: "m1", effort: "high" });
  });

  it("keeps a saved provider the daemon cannot launch, shows its reason, and sends no choice", async () => {
    const user = userEvent.setup();
    save({ agent: "claude-code", model: "m1" });
    const onChoiceChange = vi.fn<(choice: ModelChoice | null) => void>();
    const unlaunchable = { ...provider("claude-code", "Claude Code"), launchable: false, detail: "the claude executable was not found" };
    render(
      <ModelPicker catalog={catalog([provider("codex", "Codex"), unlaunchable])} provenance={provenance} onChoiceChange={onChoiceChange} />,
    );
    expect(screen.getByRole("button", { name: "Model settings: Claude Code · unavailable" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    expect(screen.getByRole("combobox", { name: "Provider" })).toHaveValue("claude-code");
    expect(screen.getByText(/Your choice is unavailable\. The claude executable was not found\./)).toBeInTheDocument();
    expect(onChoiceChange).toHaveBeenLastCalledWith(null);
    expect(savedValue()).toEqual({ agent: "claude-code", model: "m1" });
  });

  it("keeps a saved model the daemon's catalog no longer lists, and sends no choice", () => {
    save({ agent: "codex", model: "gone" });
    const onChoiceChange = vi.fn<(choice: ModelChoice | null) => void>();
    render(<ModelPicker catalog={catalog([provider("codex", "Codex")])} provenance={provenance} onChoiceChange={onChoiceChange} />);
    expect(screen.getByRole("button", { name: "Model settings: Codex · unavailable" })).toBeInTheDocument();
    expect(screen.getByText(/Your choice is unavailable\. The daemon's catalog does not list model gone\./)).toBeInTheDocument();
    expect(onChoiceChange).toHaveBeenLastCalledWith(null);
  });

  it("sends a saved provider-only choice even when that provider's model list is never read", () => {
    save({ agent: "codex" });
    const onChoiceChange = vi.fn<(choice: ModelChoice | null) => void>();
    const unread: ModelProvider = { ...provider("codex", "Codex"), models: { status: "unknown", reason: "not read yet" } };
    render(<ModelPicker catalog={catalog([unread])} provenance={provenance} onChoiceChange={onChoiceChange} />);
    expect(onChoiceChange).toHaveBeenLastCalledWith({ agent: "codex" });
  });

  it("keeps a saved provider the catalog no longer lists, and does not show the default in its place", async () => {
    const user = userEvent.setup();
    save({ agent: "pi" });
    const onChoiceChange = vi.fn<(choice: ModelChoice | null) => void>();
    render(<ModelPicker catalog={catalog([provider("codex", "Codex")])} provenance={provenance} onChoiceChange={onChoiceChange} />);
    expect(screen.getByRole("button", { name: "Model settings: pi · unavailable" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    expect(screen.getByRole("combobox", { name: "Provider" })).toHaveValue("pi");
    expect(screen.getByRole("option", { name: "pi (saved)" })).toBeDisabled();
    expect(onChoiceChange).toHaveBeenLastCalledWith(null);
  });

  it("shows a saved choice as not confirmed while the catalog loads, then sends it once the catalog confirms it", () => {
    save({ agent: "codex", model: "m1" });
    const onChoiceChange = vi.fn<(choice: ModelChoice | null) => void>();
    const loading: SurfaceState<ModelCatalog> = { status: "loading" };
    const { rerender } = render(<ModelPicker catalog={loading} provenance={provenance} onChoiceChange={onChoiceChange} />);
    expect(screen.getByRole("button", { name: "Model settings: codex · not confirmed" })).toBeInTheDocument();
    expect(onChoiceChange).toHaveBeenLastCalledWith(null);
    rerender(<ModelPicker catalog={catalog([provider("codex", "Codex")])} provenance={provenance} onChoiceChange={onChoiceChange} />);
    expect(screen.getByRole("button", { name: "Model settings: Codex · Model one · default effort" })).toBeInTheDocument();
    expect(onChoiceChange).toHaveBeenLastCalledWith({ agent: "codex", model: "m1" });
  });

  it("keeps a saved Copilot choice when the account is signed out, shows the daemon's reason, and sends no choice", async () => {
    save({ agent: "github-copilot" });
    const onChoiceChange = vi.fn<(choice: ModelChoice | null) => void>();
    const source = copilotSource(signIn("not_signed_in", "Run copilot login, then try again."));
    render(
      <ModelPicker
        catalog={catalog([provider("codex", "Codex"), copilot()])}
        provenance={provenance}
        copilotSignIn={source}
        onChoiceChange={onChoiceChange}
      />,
    );
    expect(await screen.findByRole("button", { name: "Model settings: GitHub Copilot · unavailable" })).toBeInTheDocument();
    expect(screen.getByText(/Your choice is unavailable\. Run copilot login, then try again\./)).toBeInTheDocument();
    // The sign-in answer arrives on its own tick; the picker then reports no choice.
    await waitFor(() => expect(onChoiceChange).toHaveBeenLastCalledWith(null));
    expect(source.copilotSignInStatus).toHaveBeenCalled();
  });

  it("on a daemon that cannot take a model choice, reports the saved choice so a send is blocked, and keeps it", async () => {
    const user = userEvent.setup();
    save({ agent: "codex", model: "m1" });
    const onChoiceChange = vi.fn<(choice: ModelChoice | null) => void>();
    render(
      <ModelPicker
        catalog={catalog([provider("codex", "Codex")])}
        provenance={provenance}
        selectionSupported={false}
        onChoiceChange={onChoiceChange}
      />,
    );
    expect(onChoiceChange).toHaveBeenLastCalledWith({ agent: "codex", model: "m1" });
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    expect(screen.getByText(/The connected daemon cannot take a model choice, so a message/)).toBeInTheDocument();
    expect(screen.queryByText(/Your choice is/)).toBeNull();
    expect(savedValue()).toEqual({ agent: "codex", model: "m1" });
  });

  it("still blocks a saved choice on a daemon that cannot take one when the catalog is also unavailable", () => {
    save({ agent: "codex", model: "m1" });
    const onChoiceChange = vi.fn<(choice: ModelChoice | null) => void>();
    const down: SurfaceResult<ModelCatalog> = {
      status: "unavailable",
      reason: "the catalog is not answering",
      provenance,
      ...observed,
    };
    render(
      <ModelPicker catalog={down} provenance={provenance} selectionSupported={false} onChoiceChange={onChoiceChange} />,
    );
    expect(onChoiceChange).toHaveBeenLastCalledWith({ agent: "codex", model: "m1" });
    expect(savedValue()).toEqual({ agent: "codex", model: "m1" });
  });

  it("Use the daemon default removes the saved choice, reports no choice, and shows the daemon default", async () => {
    const user = userEvent.setup();
    save({ agent: "codex", model: "m1" });
    const onChoiceChange = vi.fn<(choice: ModelChoice | null) => void>();
    render(
      <ModelPicker
        catalog={catalog([provider("codex", "Codex")])}
        provenance={provenance}
        selectionSupported={false}
        onChoiceChange={onChoiceChange}
      />,
    );
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    await user.click(screen.getByRole("button", { name: "Use the daemon default" }));
    expect(window.localStorage.getItem(CHOICE_PREFERENCE_KEY)).toBeNull();
    expect(onChoiceChange).toHaveBeenLastCalledWith(null);
    expect(screen.getByRole("button", { name: "Model settings: Codex · default model · default effort" })).toBeInTheDocument();
    expect(screen.queryByText(/The connected daemon cannot take a model choice/)).toBeNull();
  });

  it("offers Use the daemon default only while a choice is made", async () => {
    const user = userEvent.setup();
    render(<ModelPicker catalog={catalog([provider("codex", "Codex")])} provenance={provenance} />);
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    expect(screen.queryByRole("button", { name: "Use the daemon default" })).toBeNull();
  });

  it("neither restores nor saves the choice in fixture mode, so the live store is never changed", async () => {
    const user = userEvent.setup();
    save({ agent: "claude-code" });
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    try {
      const fixtureCatalog: SurfaceResult<{ providers: ModelProvider[] }> = {
        status: "ok",
        data: { providers: [provider("codex", "Codex"), provider("claude-code", "Claude Code")] },
        provenance: fixture,
        ...observed,
      };
      const onChoiceChange = vi.fn<(choice: ModelChoice | null) => void>();
      render(<ModelPicker catalog={fixtureCatalog} provenance={fixture} onChoiceChange={onChoiceChange} />);
      expect(screen.getByRole("button", { name: /^Model settings: Codex/ })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /^Model settings/ }));
      await user.selectOptions(screen.getByRole("combobox", { name: "Provider" }), "claude-code");
      expect(setItem).not.toHaveBeenCalledWith(CHOICE_PREFERENCE_KEY, expect.anything());
      expect(savedValue()).toEqual({ agent: "claude-code" });
      expect(onChoiceChange.mock.calls.every(([choice]) => choice === null)).toBe(true);
    } finally {
      setItem.mockRestore();
    }
  });

  it("starts at the default, and keeps the picker working, when browser storage is blocked", async () => {
    const user = userEvent.setup();
    const blocked = () => {
      throw new DOMException("storage is blocked", "SecurityError");
    };
    const getItem = vi.spyOn(Storage.prototype, "getItem").mockImplementation(blocked);
    const setItem = vi.spyOn(Storage.prototype, "setItem").mockImplementation(blocked);
    try {
      const onChoiceChange = vi.fn<(choice: ModelChoice | null) => void>();
      render(
        <ModelPicker
          catalog={catalog([provider("codex", "Codex"), provider("claude-code", "Claude Code")])}
          provenance={provenance}
          onChoiceChange={onChoiceChange}
        />,
      );
      expect(screen.getByRole("button", { name: "Model settings: Codex · default model · default effort" })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /^Model settings/ }));
      await user.selectOptions(screen.getByRole("combobox", { name: "Provider" }), "claude-code");
      expect(onChoiceChange).toHaveBeenLastCalledWith({ agent: "claude-code" });
    } finally {
      getItem.mockRestore();
      setItem.mockRestore();
    }
  });
});

describe("ModelPicker permission level in a chat", () => {
  it("shows the level at the end of the chip and offers the four levels in the panel", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const onRetry = vi.fn();
    const copilotWithReadFailure = {
      ...copilot(),
      chatCompatibility: { auto: { retryable: true, reason: "Could not confirm CLI compatibility. Retry the check." } },
    };
    const picker = (level: "auto" | "restricted") => (
      <ModelPicker
        catalog={catalog([provider("codex", "Codex"), copilotWithReadFailure])}
        provenance={provenance}
        permission={{ level, daemonAdvertises: true, onChange }}
        onRetry={onRetry}
      />
    );
    const { rerender } = render(picker("auto"));
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
    expect(screen.getByRole("option", { name: "GitHub Copilot (compatibility not confirmed)" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Retry compatibility check" }));
    expect(onRetry).toHaveBeenCalledOnce();
    await user.selectOptions(level, "Restricted");
    expect(onChange).toHaveBeenCalledWith("restricted");
    rerender(picker("restricted"));
    expect(screen.getByRole("option", { name: "GitHub Copilot" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Retry compatibility check" })).not.toBeInTheDocument();
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
