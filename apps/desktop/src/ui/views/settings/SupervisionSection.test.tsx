import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import { ALL_CAPABILITIES, type SettingsWire } from "../../../daemon/types";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import { observation, okResult } from "../../../sources/surfaces/result";
import type { SettingsSnapshot, SettingsSource } from "../../../sources/surfaces/types";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { ServicesProvider, type Services } from "../../services";
import { SettingsView } from "./SettingsView";

const BASE = "Check interval with one task running (minutes)";
const MIN = "Fastest check interval (seconds)";
const HOURLY = "Replies per hour, per conversation";
const DAILY = "Replies per day, per conversation";
const BUDGET = "Daily token budget for automatic replies";
const SAVE_CHECKS = "Save check settings";
const SAVE_REPLIES = "Save reply limits";

const WITH_GROUPS: SettingsWire = {
  load: { cpu_percent: 80, memory_percent: 85, disk_percent: 90 },
  gate: { state: "open", hold: null, meters: [] },
  feed: "sampler",
  supervision: { base_interval_secs: 600, min_interval_secs: 60, attempt_wall_minutes: 120 },
  wake_limits: { hourly_cap: 6, daily_cap: 40, daily_token_budget: null },
};

const stops: Array<() => void> = [];
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  while (stops.length) stops.pop()!();
});

function page(services: Services) {
  return render(
    <ServicesProvider services={services}>
      <SettingsView />
    </ServicesProvider>,
  );
}

/** A connected daemon whose `settings.set` applies the groups it is sent and answers with the result. */
async function connected(wire: SettingsWire = WITH_GROUPS, approver: boolean | null = true) {
  const fake = new FakeDaemon();
  fake.approver = approver;
  let current = wire;
  fake.stub("settings.get", () => current);
  fake.stub("settings.set", (params: Record<string, unknown>) => {
    current = {
      ...current,
      ...(current.supervision && { supervision: { ...current.supervision, ...(params.supervision as object | undefined) } }),
      ...(current.wake_limits && { wake_limits: { ...current.wake_limits, ...(params.wake_limits as object | undefined) } }),
    };
    return current;
  });
  const services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 }, data: { coalesceMs: 10 } });
  stops.push(() => {
    services.connection.stop();
    services.data.dispose();
  });
  services.connection.start();
  await until(() => services.connection.getSnapshot().status === "connected", "connected");
  expect(services.connection.getSnapshot().capabilities).toEqual(expect.arrayContaining([...ALL_CAPABILITIES]));
  return { fake, services };
}

const input = (name: string) => screen.findByRole(name === HOURLY || name === DAILY || name === BUDGET ? "textbox" : "spinbutton", { name });
const typeInto = async (user: ReturnType<typeof userEvent.setup>, name: string, text: string) => {
  const field = await input(name);
  await user.clear(field);
  if (text !== "") await user.type(field, text);
  return field;
};

describe("Supervision and automatic reply sections", () => {
  it("supervision_sections_render_labelled_inputs_with_linked_hints", async () => {
    const { services } = await connected();
    page(services);
    const checks = await screen.findByRole("region", { name: "Checks on running work" });
    const replies = screen.getByRole("region", { name: "Automatic replies" });
    expect(within(checks).getAllByRole("spinbutton")).toHaveLength(3);
    expect(within(replies).getAllByRole("textbox")).toHaveLength(3);
    expect(await input(BASE)).toHaveAccessibleDescription(/Whole number from 1 to 240/);
    expect(await input(MIN)).toHaveAccessibleDescription(/With 1 task: every 10 min/);
    expect(await input(HOURLY)).toHaveAccessibleDescription("Leave empty for no limit.");
  });

  it("supervision_sections_mark_an_invalid_field_with_linked_error_text_and_explain_why_save_is_blocked", async () => {
    const user = userEvent.setup();
    const { services } = await connected();
    page(services);
    const field = await typeInto(user, BASE, "241");
    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(field).toHaveAccessibleDescription(/Enter a whole number from 1 to 240\./);
    const save = screen.getByRole("button", { name: SAVE_CHECKS });
    expect(save).toBeEnabled();
    expect(save).toHaveAccessibleDescription("Fix the fields marked as invalid before saving.");
  });

  it("supervision_sections_explain_the_approver_notice_when_the_connection_is_not_an_approver", async () => {
    const { services } = await connected(WITH_GROUPS, false);
    page(services);
    await input(BASE);
    const save = await screen.findByRole("button", { name: SAVE_CHECKS });
    expect(save).toBeEnabled();
    expect(save).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
    expect(screen.getByRole("button", { name: SAVE_REPLIES })).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
  });

  it("supervision_sections_save_sends_one_settings_set_with_every_valid_field_of_the_saved_group", async () => {
    const user = userEvent.setup();
    const { fake, services } = await connected();
    page(services);
    await typeInto(user, BASE, "20");
    await user.click(screen.getByRole("button", { name: SAVE_CHECKS }));
    expect(await screen.findByText("Saved. New checks use these settings from the next check.")).toBeInTheDocument();
    const calls = fake.callsTo("settings.set");
    expect(calls).toHaveLength(1);
    const { idempotency_key, ...groups } = calls[0]!.params as Record<string, unknown>;
    expect(idempotency_key).toEqual(expect.any(String));
    expect(groups).toEqual({ supervision: { base_interval_secs: 1200, min_interval_secs: 60, attempt_wall_minutes: 120 } });
    expect(await input(BASE)).toHaveValue(20);
  });

  it("supervision_sections_each_section_has_its_own_result_region", async () => {
    const user = userEvent.setup();
    const { services } = await connected();
    page(services);
    await typeInto(user, BASE, "20");
    await user.click(screen.getByRole("button", { name: SAVE_CHECKS }));
    const checks = await screen.findByRole("status", { name: `${SAVE_CHECKS} result` });
    const replies = screen.getByRole("status", { name: `${SAVE_REPLIES} result` });
    await waitFor(() => expect(checks).toHaveTextContent("Saved."));
    expect(replies).toBeEmptyDOMElement();
  });

  it("supervision_sections_clearing_a_cap_shows_the_no_limit_note_and_saves_null", async () => {
    const user = userEvent.setup();
    const { fake, services } = await connected();
    page(services);
    await input(HOURLY);
    expect(screen.getByText("No limit means a busy fleet can spend tokens on automatic replies without a cap.")).toBeInTheDocument();
    await typeInto(user, HOURLY, "");
    await user.click(screen.getByRole("button", { name: SAVE_REPLIES }));
    await screen.findByText("Saved. New limits apply from the next automatic reply.");
    expect(fake.callsTo("settings.set")[0]!.params).toMatchObject({ wake_limits: { hourly_cap: null } });
    expect(fake.callsTo("settings.set")[0]!.params).not.toHaveProperty("supervision");
    await typeInto(user, DAILY, "no limit");
    await typeInto(user, BUDGET, "500000");
    expect(screen.getByRole("button", { name: SAVE_REPLIES })).toBeEnabled();
  });

  it("supervision_sections_an_older_daemon_without_the_groups_shows_the_unavailable_text_and_no_inputs", async () => {
    const { services } = await connected({ ...WITH_GROUPS, supervision: undefined, wake_limits: undefined });
    page(services);
    expect(await screen.findByText("This daemon cannot change these settings.")).toBeInTheDocument();
    expect(screen.queryByRole("spinbutton", { name: BASE })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: SAVE_CHECKS })).not.toBeInTheDocument();
  });

  it("supervision_sections_fixture_mode_says_nothing_was_saved_and_calls_no_daemon_method", async () => {
    const user = userEvent.setup();
    page(createServices());
    await typeInto(user, BASE, "20");
    await user.click(screen.getByRole("button", { name: SAVE_CHECKS }));
    expect(await screen.findByText("Fixture data: nothing was saved.")).toBeInTheDocument();
  });

  it("supervision_sections_a_refresh_does_not_overwrite_an_unsaved_edit", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "setTimeout", "clearTimeout", "Date"] });
    const fixture = createFixtureSurfaces();
    const first = await fixture.settings.get();
    if (first.status !== "ok") throw new Error("fixture settings unavailable");
    const get = vi.fn(async () => okResult(first.provenance, { ...first.data, supervision: { baseIntervalSecs: 1200, minIntervalSecs: 60, attemptWallMinutes: 120 } } as SettingsSnapshot, observation(0)));
    const source: SettingsSource = { ...fixture.settings, get };
    page(createServices({ surfaces: { ...fixture, settings: source } }));
    await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
    const field = screen.getByRole("spinbutton", { name: BASE }) as HTMLInputElement;
    expect(field.value).toBe("20");
    // userEvent needs real timers, so the edit is a change event.
    fireEvent.change(field, { target: { value: "30" } });
    await act(async () => void (await vi.advanceTimersByTimeAsync(5_000)));
    expect(get.mock.calls.length).toBeGreaterThan(1);
    expect((screen.getByRole("spinbutton", { name: BASE }) as HTMLInputElement).value).toBe("30");
  });

  it("supervision_sections_leave_the_load_sections_own_save_working_beside_them", async () => {
    const user = userEvent.setup();
    const { fake, services } = await connected();
    page(services);
    await typeInto(user, "CPU limit (percent)", "85");
    await user.click(screen.getByRole("button", { name: "Save machine load limits" }));
    await waitFor(() => expect(fake.callsTo("settings.set")).toHaveLength(1));
    const { idempotency_key: _key, ...groups } = fake.callsTo("settings.set")[0]!.params as Record<string, unknown>;
    expect(groups).toEqual({ load: { cpu_percent: 85, memory_percent: 85, disk_percent: 90 } });
  });
});
