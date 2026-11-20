import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { APPROVER_REQUIRED_NOTICE, DaemonError } from "../../../daemon/errors";
import { ALL_CAPABILITIES, type LoadHoldWire, type LoadMeterWire, type SettingsWire } from "../../../daemon/types";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import { observation, okResult } from "../../../sources/surfaces/result";
import type { LoadReading, ModelCatalog, Provenance, SettingsSnapshot, SettingsSource } from "../../../sources/surfaces/types";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { type Services } from "../../services";
import { renderSettingsPage } from "../../../test/settingsPage";

const CPU = "CPU limit (percent)";
const MEMORY = "Memory limit (percent)";
const DISK = "Disk limit (percent)";
const SAVE = "Save changes";
const SAVED_TEXT = "Saved. The new limits apply from the next measurement.";

const mean = (meter: LoadMeterWire["meter"], mean_percent: number): LoadMeterWire => ({ meter, state: "mean", mean_percent });
const OPEN: SettingsWire = {
  load: { cpu_percent: 80, memory_percent: 85, disk_percent: 90 },
  gate: { state: "open", hold: null, meters: [mean("cpu", 42), mean("memory", 61), mean("disk", 40)] },
  feed: "sampler",
};
const HOLD: LoadHoldWire = { meter: "cpu", mean_percent: 91, limit_percent: 80, reopen_percent: 70 };
const CLOSED: SettingsWire = {
  ...OPEN,
  gate: {
    state: "closed",
    hold: HOLD,
    meters: [mean("cpu", 91), mean("memory", 62), { meter: "disk", state: "unknown", reason: "warming_up" }],
  },
};

const stops: Array<() => void> = [];
afterEach(() => {
  cleanup();
  while (stops.length) stops.pop()!();
});

const page = (services: Services, id = "limits") => renderSettingsPage(services, id);

/** A connected daemon session that negotiated `capabilities`; `settings.get` answers `wire` unless the test stubs it again. */
async function connected(
  wire: SettingsWire = OPEN,
  configure: (fake: FakeDaemon) => void = () => undefined,
  capabilities: typeof ALL_CAPABILITIES = ALL_CAPABILITIES,
) {
  const fake = new FakeDaemon();
  fake.capabilitiesOverride = [...capabilities];
  fake.stub("settings.get", wire);
  fake.stub("agents.capabilities", []);
  configure(fake);
  const services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 }, data: { coalesceMs: 10 } });
  stops.push(() => {
    services.connection.stop();
    services.data.dispose();
  });
  services.connection.start();
  await until(() => services.connection.getSnapshot().status === "connected", "connected");
  return { fake, services };
}

const field = (name: string) => screen.findByRole("spinbutton", { name });
const typeInto = async (user: ReturnType<typeof userEvent.setup>, name: string, text: string) => {
  const input = await field(name);
  await user.clear(input);
  await user.type(input, text);
  return input;
};

const DISPATCH_CATALOG: ModelCatalog = {
  providers: ["codex", "claude-code"].map((id) => ({
    id, name: id, installed: true, launchable: true, modelSelection: "supported",
    models: { status: "ok", value: [
      { id: "m1", label: "Model one", efforts: ["low", "high"] },
      { id: "m2", label: "Model two", efforts: ["medium"] },
    ], observedAt: "2026-11-17T00:00:00Z", ageMs: 0 },
    efforts: { status: "unsupported", reason: "Choose a model to see effort levels." },
  })),
};

// Covers the shapes a provider can have: models and efforts (Codex, Claude Code), an ok-but-empty list (Pi), and facets the
// daemon reports as unsupported (Copilot) (DPES-1105).
const NOW = { observedAt: "2026-11-17T00:00:00Z", ageMs: 0 };
const REAL_SHAPED_CATALOG: ModelCatalog = {
  providers: [
    DISPATCH_CATALOG.providers[0]!,
    {
      id: "claude-code", name: "Claude Code", installed: true, launchable: true, modelSelection: "supported",
      models: { status: "ok", value: [{ id: "opus", label: "Opus", efforts: [] }, { id: "sonnet", label: "Sonnet", efforts: [] }], ...NOW },
      efforts: { status: "ok", value: ["low", "high"], ...NOW },
    },
    {
      id: "pi", name: "Pi", installed: true, launchable: true, modelSelection: "unsupported",
      models: { status: "ok", value: [], ...NOW },
      efforts: { status: "ok", value: [], ...NOW },
    },
    {
      id: "github-copilot", name: "GitHub Copilot", installed: true, launchable: true, modelSelection: "unsupported",
      models: { status: "unsupported", reason: "Copilot cannot list models without a billable call." },
      efforts: { status: "unsupported", reason: "This provider exposes no effort setting." },
    },
  ],
};

async function dispatchPage(wire: SettingsWire, catalog: ModelCatalog = DISPATCH_CATALOG) {
  const { fake, services } = await connected(wire);
  let saved = wire;
  fake.stub("settings.get", () => saved);
  fake.stub("settings.set", (params: unknown) => {
    saved = { ...saved, dispatch_defaults: (params as SettingsWire).dispatch_defaults };
    return saved;
  });
  vi.spyOn(services.surfaces.models, "catalog").mockResolvedValue(
    okResult(services.surfaces.models.provenance, catalog, observation(Date.now())),
  );
  fake.stub("agents.validate_selection", { valid: true });
  return { fake, services };
}

describe("Agent dispatch defaults", () => {
  it("saves per-provider pairs, preserves them when switching providers, and restores after remount", async () => {
    const { fake, services } = await dispatchPage({ ...OPEN, dispatch_defaults: { default_agent: null, providers: {} } });
    const user = userEvent.setup();
    const first = page(services, "agents");
    await user.selectOptions(await screen.findByLabelText("codex model"), "m1");
    await user.selectOptions(screen.getByLabelText("codex effort"), "high");
    await user.selectOptions(screen.getByLabelText("claude-code model"), "m2");
    await user.selectOptions(screen.getByLabelText("claude-code effort"), "medium");
    await user.selectOptions(screen.getByLabelText("Default provider"), "claude-code");
    await user.selectOptions(screen.getByLabelText("Default provider"), "codex");
    expect(screen.getByLabelText("claude-code effort")).toHaveValue("medium");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    expect(await screen.findByText("Saved. New tasks use these defaults.")).toBeInTheDocument();
    expect(fake.callsTo("settings.set")[0]?.params).toEqual({
      idempotency_key: expect.any(String),
      dispatch_defaults: {
        default_agent: "codex",
        providers: { codex: { model: "m1", effort: "high" }, "claude-code": { model: "m2", effort: "medium" } },
      },
    });
    first.unmount();
    page(services, "agents");
    expect(await screen.findByLabelText("Default provider")).toHaveValue("codex");
    expect(await screen.findByLabelText("Default effort")).toHaveValue("high");
    expect(screen.getByLabelText("claude-code model")).toHaveValue("m2");
    await user.selectOptions(screen.getByLabelText("Default model"), "m2");
    expect(screen.getByLabelText("Default effort")).toHaveValue("");
    expect(within(screen.getByLabelText("Default effort")).queryByRole("option", { name: "high" })).not.toBeInTheDocument();
  });

  it("shows the default provider's model and effort at the top and keeps other providers' overrides in a collapsed disclosure", async () => {
    const { services } = await dispatchPage({ ...OPEN, dispatch_defaults: { default_agent: "codex", providers: { "claude-code": { model: "m2" } } } });
    page(services, "agents");
    expect(await screen.findByLabelText("Default model")).toBeInTheDocument();
    expect(screen.getByLabelText("Default effort")).toBeInTheDocument();
    expect(screen.queryByLabelText("codex model")).not.toBeInTheDocument();
    const overrides = screen.getByText("Per-provider overrides").closest("details")!;
    expect(overrides).not.toHaveAttribute("open");
    expect(within(overrides).getByText("1 set")).toBeInTheDocument();
    expect(within(overrides).getByLabelText("claude-code model")).toHaveValue("m2");
  });

  it("retains unavailable saved choices, reports validation failures, and lets the user clear them", async () => {
    const { fake, services } = await dispatchPage({
      ...OPEN, dispatch_defaults: { default_agent: "codex", providers: { codex: { model: "gone", effort: "high" } } },
    });
    fake.stub("agents.validate_selection", { valid: false, reason: "unknown_model", detail: "The selected model is no longer available." });
    const user = userEvent.setup();
    page(services, "agents");
    expect(await screen.findByLabelText("Default model")).toHaveValue("gone");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    expect(await screen.findByText("codex: The selected model is no longer available.")).toBeInTheDocument();
    expect(fake.callsTo("settings.set")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Clear defaults" }));
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    expect(await screen.findByText("Saved. New tasks use these defaults.")).toBeInTheDocument();
    expect(fake.callsTo("settings.set")[0]?.params).toMatchObject({ dispatch_defaults: { default_agent: null, providers: {} } });
  });

  it("keeps every override usable after saving the default, saves the picks, and clears one with Provider default", async () => {
    const { fake, services } = await dispatchPage(
      {
        ...OPEN,
        dispatch_defaults: { default_agent: "codex", providers: { pi: { model: "old" }, "github-copilot": { model: "old" } } },
      },
      REAL_SHAPED_CATALOG,
    );
    const user = userEvent.setup();
    page(services, "agents");
    await user.selectOptions(await screen.findByLabelText("Default model"), "m1");
    await user.selectOptions(screen.getByLabelText("Default effort"), "high");
    await user.click(screen.getByRole("button", { name: SAVE }));
    expect(await screen.findByText("Saved. New tasks use these defaults.")).toBeInTheDocument();
    expect(fake.callsTo("settings.set")[0]?.params).toMatchObject({
      dispatch_defaults: {
        default_agent: "codex",
        providers: { codex: { model: "m1", effort: "high" }, pi: { model: "old" }, "github-copilot": { model: "old" } },
      },
    });

    const overrides = screen.getByText("Per-provider overrides").closest("details")!;
    expect(within(overrides).getByText("2 set")).toBeInTheDocument();
    // A saved choice keeps its picker usable; the picker with nothing saved is disabled and says why.
    expect(within(overrides).getByLabelText("GitHub Copilot model")).toBeEnabled();
    expect(within(overrides).getByLabelText("GitHub Copilot effort")).toBeDisabled();
    expect(within(overrides).getByLabelText("GitHub Copilot effort")).toHaveAccessibleDescription(/own default effort/);
    await user.selectOptions(within(overrides).getByLabelText("Claude Code model"), "sonnet");
    await user.selectOptions(within(overrides).getByLabelText("Claude Code effort"), "low");
    expect(within(overrides).getByLabelText("Claude Code model")).toHaveValue("sonnet");
    expect(within(overrides).getByLabelText("Claude Code effort")).toHaveValue("low");
    expect(within(overrides).getByText("3 set")).toBeInTheDocument();
    await user.selectOptions(within(overrides).getByLabelText("Pi model"), "");
    await user.selectOptions(within(overrides).getByLabelText("GitHub Copilot model"), "");
    expect(within(overrides).getByLabelText("Pi model")).toHaveValue("");
    expect(within(overrides).getByLabelText("GitHub Copilot model")).toHaveValue("");
    expect(within(overrides).getByText("1 set")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: SAVE }));
    await until(() => fake.callsTo("settings.set").length === 2, "second save");
    const second = (fake.callsTo("settings.set")[1]?.params as { dispatch_defaults: { providers: object } }).dispatch_defaults;
    expect(second.providers).toEqual({ codex: { model: "m1", effort: "high" }, "claude-code": { model: "sonnet", effort: "low" } });
  });

  it("says in place why a provider's model or effort list cannot be chosen from", async () => {
    const { services } = await dispatchPage({ ...OPEN, dispatch_defaults: { default_agent: "codex", providers: {} } }, REAL_SHAPED_CATALOG);
    page(services, "agents");
    const overrides = (await screen.findByText("Per-provider overrides")).closest("details")!;
    // An empty list (status ok) and a facet the provider does not support are both disabled and described.
    const NO_MODELS = "This provider lists no models to choose from. It runs with its own default model.";
    const NO_EFFORTS = "No effort levels to choose from. It runs with its own default effort.";
    expect(within(overrides).getByLabelText("Pi model")).toBeDisabled();
    expect(within(overrides).getByLabelText("Pi model")).toHaveAccessibleDescription(NO_MODELS);
    expect(within(overrides).getByLabelText("Pi effort")).toBeDisabled();
    expect(within(overrides).getByLabelText("Pi effort")).toHaveAccessibleDescription(NO_EFFORTS);
    expect(within(overrides).getByLabelText("GitHub Copilot model")).toHaveAccessibleDescription(
      "Copilot cannot list models without a billable call.",
    );
    expect(within(overrides).getByLabelText("GitHub Copilot effort")).toHaveAccessibleDescription(
      "This provider exposes no effort setting. It runs with its own default effort.",
    );
    // A provider with a list stays usable and carries no explanation.
    expect(within(overrides).getByLabelText("Claude Code model")).toBeEnabled();
    expect(within(overrides).getByLabelText("Claude Code model")).not.toHaveAccessibleDescription();
  });

  it("does not allow a token-only session to save dispatch defaults", async () => {
    const fake = new FakeDaemon();
    fake.approver = false;
    fake.stub("settings.get", { ...OPEN, dispatch_defaults: { default_agent: "codex", providers: {} } });
    const services = createServices({ connector: fake, connection: { backoffMs: [5] } });
    stops.push(() => { services.connection.stop(); services.data.dispose(); });
    services.connection.start();
    await until(() => services.connection.getSnapshot().status === "connected", "connected");
    page(services, "agents");
    expect(await screen.findByLabelText("Default provider")).toBeDisabled();
    // The reason is in the section beside the disabled controls, as well as at the Save button.
    expect(screen.getAllByText(APPROVER_REQUIRED_NOTICE).length).toBeGreaterThan(1);
    fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
    expect(fake.callsTo("settings.set")).toHaveLength(0);
  });
});

describe("Settings page, live", () => {
  it("registers a local repository from Settings and shows it in the repository list", async () => {
    const { fake, services } = await connected();
    const user = userEvent.setup();
    page(services, "repositories");
    const region = await screen.findByRole("region", { name: "Your repositories" });
    await user.click(within(region).getByRole("button", { name: "Add repository" }));
    await user.type(screen.getByLabelText("Repository path"), "/work/new-repo");
    await user.click(screen.getByRole("button", { name: "Register repository" }));

    expect(await within(region).findByText("Repository registered.")).toBeInTheDocument();
    expect(await within(region).findByRole("list", { name: "Registered repositories" })).toHaveTextContent("/work/new-repo");
    expect(fake.callsTo("projects.register")[0]?.params).toEqual({ path: "/work/new-repo" });
  });

  it("rejects a relative repository path in Settings without calling the daemon", async () => {
    const { fake, services } = await connected();
    const user = userEvent.setup();
    page(services, "repositories");
    const region = await screen.findByRole("region", { name: "Your repositories" });
    await user.click(within(region).getByRole("button", { name: "Add repository" }));
    await user.type(screen.getByLabelText("Repository path"), "relative/repo");
    await user.click(screen.getByRole("button", { name: "Register repository" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Enter an absolute repository path.");
    expect(fake.callsTo("projects.register")).toHaveLength(0);
  });

  it("settings_page_shows_three_labelled_limit_fields_with_the_daemons_values_and_hints", async () => {
    const { services } = await connected();
    page(services);
    expect(await screen.findByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
    const region = screen.getByRole("region", { name: "Machine load limits" });
    expect(within(region).getByText(/Each limit is compared with the average of the last 60 seconds/)).toBeInTheDocument();
    const cpu = await field(CPU);
    const memory = await field(MEMORY);
    const disk = await field(DISK);
    expect([cpu, memory, disk].map((input) => (input as HTMLInputElement).value)).toEqual(["80", "85", "90"]);
    for (const input of [cpu, memory, disk]) {
      expect(input).toHaveAttribute("min", "50");
      expect(input).toHaveAttribute("max", "100");
      expect(input).toHaveAttribute("inputmode", "numeric");
    }
    expect(cpu).toHaveAccessibleDescription("Now: 42 percent (60 second average)");
    expect(disk).toHaveAccessibleDescription("Now: 40 percent (60 second average) Measured on the disk that holds the app's data");
  });

  it("settings_status_strip_shows_an_open_gate_with_the_three_means", async () => {
    const { services } = await connected();
    page(services);
    expect(await screen.findByText("Machine load: OK, new tasks can start")).toBeInTheDocument();
    const means = within(screen.getByRole("list", { name: "Current machine load" })).getAllByRole("listitem");
    expect(means.map((item) => item.textContent)).toEqual(["CPU 42 percent", "memory 61 percent", "disk 40 percent"]);
  });

  it("settings_status_strip_shows_a_closed_gate_with_the_holding_meter_and_an_unread_meter", async () => {
    const { services } = await connected(CLOSED);
    page(services);
    expect(await screen.findByText("Machine load: busy, new tasks are waiting (CPU 91 percent, limit 80)")).toBeInTheDocument();
    const means = within(screen.getByRole("list", { name: "Current machine load" })).getAllByRole("listitem");
    expect(means.map((item) => item.textContent)).toEqual(["CPU 91 percent", "memory 62 percent", "Load unknown: disk"]);
    expect(await field(DISK)).toHaveAccessibleDescription(/Load unknown: disk \(still measuring\)/);
  });

  it("settings_page_says_load_unknown_for_a_meter_the_machine_cannot_read", async () => {
    const wire: SettingsWire = { ...OPEN, gate: { ...OPEN.gate, meters: [{ meter: "cpu", state: "unknown", reason: "unavailable" }, mean("memory", 61), mean("disk", 40)] } };
    const { services } = await connected(wire);
    page(services);
    expect(await screen.findByText("Load unknown: CPU")).toBeInTheDocument();
    expect(await field(CPU)).toHaveAccessibleDescription("Load unknown: CPU (not available on this machine)");
  });

  it("settings_page_says_load_checks_are_off_for_a_daemon_started_with_the_test_override", async () => {
    const { services } = await connected({ ...OPEN, feed: "disabled_by_environment" });
    page(services);
    expect(await screen.findByText("Load checks are off for this daemon.")).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Current machine load" })).not.toBeInTheDocument();
    expect(screen.queryByText(/Machine load: OK/)).not.toBeInTheDocument();
  });

  it("settings_page_shows_the_off_note_beside_a_field_at_100_and_a_check_that_is_off", async () => {
    const wire: SettingsWire = {
      load: { cpu_percent: 80, memory_percent: 85, disk_percent: 100 },
      gate: { state: "open", hold: null, meters: [mean("cpu", 42), mean("memory", 61), { meter: "disk", state: "off" }] },
    };
    const { services } = await connected(wire);
    const user = userEvent.setup();
    page(services);
    const disk = await field(DISK);
    expect(disk).toHaveAccessibleDescription(/Check is off/);
    expect(disk).toHaveAccessibleDescription(/Off: tasks start whatever the load\./);
    expect(screen.getAllByText("Off: tasks start whatever the load.")).toHaveLength(1);
    await typeInto(user, CPU, "100");
    expect(screen.getAllByText("Off: tasks start whatever the load.")).toHaveLength(2);
    await typeInto(user, CPU, "90");
    expect(screen.getAllByText("Off: tasks start whatever the load.")).toHaveLength(1);
  });

  it("settings_page_marks_an_invalid_limit_with_linked_error_text_and_a_save_that_says_why", async () => {
    const { fake, services } = await connected();
    const user = userEvent.setup();
    page(services);
    const cpu = await typeInto(user, CPU, "49");
    expect(cpu).toHaveAttribute("aria-invalid", "true");
    expect(cpu).toHaveAccessibleDescription(/Enter a whole number from 50 to 100\./);
    expect(screen.getByRole("alert")).toHaveTextContent("Enter a whole number from 50 to 100.");
    expect(cpu).toHaveFocus();
    const save = screen.getByRole("button", { name: SAVE });
    expect(save).toBeEnabled();
    expect(save).toHaveAccessibleDescription("Enter whole numbers from 50 to 100 before saving.");
    await user.click(save);
    expect(screen.getByText("Enter whole numbers from 50 to 100 before saving.")).toHaveAttribute("role", "alert");
    expect(fake.callsTo("settings.set")).toHaveLength(0);
    await typeInto(user, CPU, "70");
    expect(cpu).not.toHaveAttribute("aria-invalid");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(save).toBeEnabled();
  });

  it("settings_page_save_without_an_edit_still_writes_the_saved_limits", async () => {
    const { fake, services } = await connected(OPEN, (daemon) => daemon.stub("settings.set", () => OPEN));
    const user = userEvent.setup();
    page(services);
    await field(CPU);
    const save = screen.getByRole("button", { name: SAVE });
    expect(save).toBeEnabled();
    await user.click(save);
    expect(await screen.findByText(SAVED_TEXT)).toBeInTheDocument();
    expect(fake.callsTo("settings.set")).toHaveLength(1);
    expect(fake.callsTo("settings.set")[0]!.params).toMatchObject({
      load: { cpu_percent: 80, memory_percent: 85, disk_percent: 90 },
    });
  });

  it("settings_page_saves_every_valid_limit_and_announces_the_new_values", async () => {
    const { fake, services } = await connected(OPEN, (daemon) =>
      daemon.stub("settings.set", () => ({ ...OPEN, load: { ...OPEN.load, cpu_percent: 85 } })),
    );
    const user = userEvent.setup();
    page(services);
    await typeInto(user, CPU, "85");
    await typeInto(user, MEMORY, "85");
    await user.click(screen.getByRole("button", { name: SAVE }));
    expect(await screen.findByText(SAVED_TEXT)).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "Save result" })).toHaveTextContent(SAVED_TEXT);
    expect(fake.callsTo("settings.set")).toEqual([
      {
        method: "settings.set",
        params: { idempotency_key: expect.any(String), load: { cpu_percent: 85, memory_percent: 85, disk_percent: 90 } },
      },
    ]);
    expect(await field(CPU)).toHaveValue(85);
    expect(screen.getByRole("button", { name: SAVE })).toBeEnabled();
  });

  it.each([
    ["the daemon refuses the values", { code: -32602, message: "bad" }, "The daemon refused these values."],
    ["the daemon says only the app can change limits", { code: -32012, message: "no" }, "Only the app can change these limits."],
  ])("settings page says so in plain words when %s", async (_label, failure, text) => {
    const { fake, services } = await connected();
    fake.failNext("settings.set", failure);
    const user = userEvent.setup();
    page(services);
    const cpu = await typeInto(user, CPU, "70");
    const save = screen.getByRole("button", { name: SAVE });
    await user.click(save);
    expect(await screen.findByText(text)).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "Save result" })).toHaveTextContent(text);
    expect(cpu).toHaveValue(70);
    expect(save).toBeEnabled();
    expect(save).toHaveFocus();
  });

  it("settings_page_says_the_daemon_did_not_answer_and_a_retry_reuses_the_same_request_key", async () => {
    const { fake, services } = await connected(OPEN, (daemon) => {
      daemon.stub("settings.set", () => ({ ...OPEN, load: { ...OPEN.load, cpu_percent: 70 } }));
      daemon.failNext("settings.set", "hang");
    });
    const user = userEvent.setup();
    page(services);
    await typeInto(user, CPU, "70");
    await user.click(screen.getByRole("button", { name: SAVE }));
    expect(await screen.findByText("The daemon did not answer. Nothing was changed.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: SAVE }));
    expect(await screen.findByText(SAVED_TEXT)).toBeInTheDocument();
    const keys = fake.callsTo("settings.set").map((call) => (call.params as { idempotency_key: string }).idempotency_key);
    expect(keys).toHaveLength(2);
    expect(keys[0]).toBe(keys[1]);
  });

  it("settings_page_blocks_save_and_shows_the_notice_when_this_app_did_not_start_the_service", async () => {
    const { fake, services } = await connected(OPEN, (daemon) => {
      daemon.approver = false;
    });
    page(services);
    const cpu = await field(CPU);
    const save = screen.getByRole("button", { name: SAVE });
    expect(save).toBeEnabled();
    expect(screen.getByText(APPROVER_REQUIRED_NOTICE)).toBeVisible();
    expect(save).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
    expect(cpu).toHaveValue(80);
    expect(cpu).toHaveAttribute("readonly");
    expect(fake.callsTo("settings.set")).toHaveLength(0);
  });

  it("settings_page_says_settings_are_not_available_on_a_daemon_without_the_settings_capability", async () => {
    const { fake, services } = await connected(
      OPEN,
      () => undefined,
      ALL_CAPABILITIES.filter((capability) => capability !== "settings"),
    );
    page(services);
    expect(await screen.findByText("Settings are not available on this daemon.")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByRole("spinbutton")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: SAVE })).not.toBeInTheDocument();
    expect(fake.callsTo("settings.get")).toHaveLength(0);
  });

  it("settings_page_shows_an_error_with_retry_when_the_first_read_fails_and_retry_reads_again", async () => {
    const { fake, services } = await connected(OPEN, (daemon) => daemon.failNext("settings.get", { code: -32603, message: "store busy" }));
    const user = userEvent.setup();
    page(services);
    expect(await screen.findByRole("alert")).toHaveTextContent("The machine load limits could not be read.");
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await field(CPU)).toHaveValue(80);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(fake.callsTo("settings.get")).toHaveLength(2);
  });

  it("settings_page_blocks_save_and_notes_the_failed_refresh_when_the_connection_is_lost", async () => {
    const { fake, services } = await connected();
    page(services);
    await field(CPU);
    fake.openError = new DaemonError("disconnected", "down");
    fake.drop();
    await until(() => services.connection.getSnapshot().status !== "connected", "connection lost");
    expect(await screen.findByText("The last refresh failed. The load figures on this page may be out of date.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: SAVE })).toBeEnabled();
    expect(screen.getByText("The app is not connected to the orchestrator service, so nothing can be saved.")).toBeVisible();
  });
});

describe("Settings page, refresh", () => {
  const FIXTURE: Provenance = { kind: "fixture", id: "D17", fixture: "settings" };
  const readings = (cpu: number): Record<"cpu" | "memory" | "disk", LoadReading> => ({
    cpu: { state: "mean", meanPercent: cpu },
    memory: { state: "mean", meanPercent: 61 },
    disk: { state: "mean", meanPercent: 40 },
  });
  const open = (cpu: number): SettingsSnapshot => ({
    limits: { cpu: 80, memory: 85, disk: 90 },
    gate: { state: "open", hold: null, readings: readings(cpu) },
    feed: "sampler",
  });
  const closed = (cpu: number): SettingsSnapshot => ({
    limits: { cpu: 80, memory: 85, disk: 90 },
    gate: { state: "closed", hold: { meter: "cpu", meanPercent: cpu, limitPercent: 80, reopenPercent: 70 }, readings: readings(cpu) },
    feed: "sampler",
  });

  /** A source whose answer the test changes; every read is counted. */
  function scripted(first: SettingsSnapshot) {
    const state = { current: first };
    const get = vi.fn(async () => okResult(FIXTURE, state.current, observation(0)));
    const source: SettingsSource = {
      provenance: FIXTURE,
      get,
      set: () => Promise.reject(new Error("not used")),
      setGroups: () => Promise.reject(new Error("not used")),
    };
    return { state, get, services: createServices({ surfaces: { ...createFixtureSurfaces(), settings: source } }) };
  }

  const advance = (ms: number) => act(async () => void (await vi.advanceTimersByTimeAsync(ms)));
  const setVisibility = (value: "visible" | "hidden") =>
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => value });

  beforeEach(() => vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "setTimeout", "clearTimeout", "Date"] }));
  afterEach(() => {
    vi.useRealTimers();
    Reflect.deleteProperty(document, "visibilityState");
  });

  it("settings_page_reads_every_5_seconds_while_mounted_and_stops_when_it_unmounts", async () => {
    const { get, services } = scripted(open(42));
    const view = page(services);
    await advance(0);
    expect(get).toHaveBeenCalledTimes(1);
    await advance(4_999);
    expect(get).toHaveBeenCalledTimes(1);
    await advance(1);
    expect(get).toHaveBeenCalledTimes(2);
    await advance(5_000);
    expect(get).toHaveBeenCalledTimes(3);
    view.unmount();
    await advance(60_000);
    expect(get).toHaveBeenCalledTimes(3);
  });

  it("settings_page_does_not_read_while_the_document_is_hidden_and_reads_again_when_it_shows", async () => {
    const { get, services } = scripted(open(42));
    page(services);
    await advance(0);
    setVisibility("hidden");
    await advance(20_000);
    expect(get).toHaveBeenCalledTimes(1);
    setVisibility("visible");
    act(() => void document.dispatchEvent(new Event("visibilitychange")));
    await advance(0);
    expect(get).toHaveBeenCalledTimes(2);
  });

  it("settings_page_refresh_updates_the_strip_and_hints_but_never_an_unsaved_edit", async () => {
    const { state, services } = scripted(open(42));
    page(services);
    await advance(0);
    const cpu = screen.getByRole("spinbutton", { name: CPU });
    fireEvent.change(cpu, { target: { value: "85" } });
    state.current = closed(95);
    await advance(5_000);
    expect(screen.getByText("Machine load: busy, new tasks are waiting (CPU 95 percent, limit 80)")).toBeInTheDocument();
    expect(cpu).toHaveAccessibleDescription("Now: 95 percent (60 second average)");
    expect(cpu).toHaveValue(85);
    expect(screen.getByRole("spinbutton", { name: MEMORY })).toHaveValue(85);
  });

  it("settings_page_announces_a_change_of_gate_state_once_and_a_changed_number_never", async () => {
    const { state, services } = scripted(open(42));
    page(services);
    await advance(0);
    const announcer = screen.getByRole("status", { name: "Machine load changes" });
    expect(announcer).toBeEmptyDOMElement();
    state.current = open(55);
    await advance(5_000);
    expect(announcer).toBeEmptyDOMElement();
    state.current = closed(91);
    await advance(5_000);
    expect(announcer).toHaveTextContent("Machine load is busy. New tasks are waiting.");
    state.current = closed(96);
    await advance(5_000);
    expect(announcer).toHaveTextContent("Machine load is busy. New tasks are waiting.");
    state.current = open(60);
    await advance(5_000);
    expect(announcer).toHaveTextContent("Machine load is OK. New tasks can start.");
  });

  it("settings_page_shows_loading_settings_until_the_first_read_answers", async () => {
    const source: SettingsSource = { provenance: FIXTURE, get: () => new Promise(() => undefined), set: () => Promise.reject(new Error("not used")), setGroups: () => Promise.reject(new Error("not used")) };
    page(createServices({ surfaces: { ...createFixtureSurfaces(), settings: source } }));
    await advance(0);
    // The standing approvals section keeps its own result region, so name the loading line.
    expect(screen.getByText("Loading settings…")).toHaveAttribute("role", "status");
    expect(screen.queryByRole("spinbutton")).not.toBeInTheDocument();
  });
});

describe("Settings page, notifications", () => {
  it("settings_page_carries_the_notifications_section_for_decisions_that_need_you", async () => {
    page(createServices(), "notifications");
    expect(await screen.findByRole("heading", { level: 3, name: "Decision notifications" })).toBeInTheDocument();
  });
});

describe("Settings page, fixture mode", () => {
  it("settings_page_in_fixture_mode_is_labelled_saves_nothing_and_calls_no_daemon_method", async () => {
    const services = createServices();
    const api = vi.spyOn(services.connection, "api");
    const user = userEvent.setup();
    page(services);
    expect(await screen.findByText("Fixture data")).toBeInTheDocument();
    const cpu = await typeInto(user, CPU, "70");
    const save = screen.getByRole("button", { name: SAVE });
    expect(save).toBeEnabled();
    await user.click(save);
    expect(await screen.findByText("Fixture data: nothing was saved.")).toBeInTheDocument();
    expect(cpu).toHaveValue(70);
    expect(screen.getByRole("button", { name: SAVE })).toBeEnabled();
    expect(api).not.toHaveBeenCalled();
  });

  it("settings_page_in_fixture_mode_shows_the_closed_gate_example_only_when_asked", async () => {
    const closedServices = createServices({ surfaces: createFixtureSurfaces({ settingsGate: "closed" }) });
    const closedView = page(closedServices);
    expect(await screen.findByText("Machine load: busy, new tasks are waiting (CPU 91 percent, limit 80)")).toBeInTheDocument();
    expect(screen.getByText("Load unknown: disk")).toBeInTheDocument();
    closedView.unmount();
    page(createServices());
    expect(await screen.findByText("Machine load: OK, new tasks can start")).toBeInTheDocument();
  });
});
