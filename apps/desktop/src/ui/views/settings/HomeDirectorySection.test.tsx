import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import type { SettingsWire } from "../../../daemon/types";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { ServicesProvider, type Services } from "../../services";
import { HOME_DIRECTORY_REFUSED_TEXT } from "./HomeDirectorySection";
import { SettingsView } from "./SettingsView";

const LABEL = "Home directory";
const SAVE = "Save home directory";
const RESULT = "Save home directory result";

const BASE: SettingsWire = {
  load: { cpu_percent: 80, memory_percent: 85, disk_percent: 90 },
  gate: { state: "open", hold: null, meters: [] },
  feed: "sampler",
};

const stops: Array<() => void> = [];
afterEach(() => {
  cleanup();
  while (stops.length) stops.pop()!();
});

function page(services: Services) {
  return render(
    <ServicesProvider services={services}>
      <SettingsView />
    </ServicesProvider>,
  );
}

async function connected(wire: SettingsWire, approver: boolean | null = true) {
  const fake = new FakeDaemon();
  fake.approver = approver;
  let current = wire;
  fake.stub("settings.get", () => current);
  // The daemon answers with the folder it resolved: `~/Code` comes back as a full path.
  fake.stub("settings.set", (params: Record<string, unknown>) => {
    if ("home_directory" in params) {
      const typed = params.home_directory as string | null;
      current = { ...current, home_directory: typed === null ? null : typed.replace(/^~/, "/Users/ada") };
    }
    return current;
  });
  const services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 }, data: { coalesceMs: 10 } });
  stops.push(() => {
    services.connection.stop();
    services.data.dispose();
  });
  services.connection.start();
  await until(() => services.connection.getSnapshot().status === "connected", "connected");
  return { fake, services };
}

describe("Home directory section", () => {
  it("home_directory_shows_the_saved_folder_with_the_example_and_what_it_is_for", async () => {
    const { services } = await connected({ ...BASE, home_directory: "/Users/ada/Code" });
    page(services);
    expect(await screen.findByRole("heading", { level: 2, name: "Home directory" })).toBeInTheDocument();
    const input = screen.getByRole("textbox", { name: LABEL });
    expect(input).toHaveValue("/Users/ada/Code");
    expect(input).toHaveAttribute("placeholder", "~/Code");
    expect(screen.getByText(/when it is not clear which project it belongs to/)).toBeInTheDocument();
    expect(screen.getByText(/writes to this folder directly/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: SAVE })).toBeEnabled();
  });

  it("home_directory_is_empty_when_none_is_set", async () => {
    const { services } = await connected({ ...BASE, home_directory: null });
    page(services);
    expect(await screen.findByRole("textbox", { name: LABEL })).toHaveValue("");
  });

  it("home_directory_is_hidden_for_a_daemon_that_does_not_send_the_setting", async () => {
    const { services } = await connected(BASE);
    page(services);
    await screen.findByRole("heading", { level: 2, name: "Machine load limits" });
    await screen.findByLabelText("CPU limit (percent)");
    expect(screen.queryByRole("heading", { name: "Home directory" })).not.toBeInTheDocument();
  });

  it("home_directory_save_sends_only_the_typed_folder_and_shows_the_path_the_daemon_resolved", async () => {
    const user = userEvent.setup();
    const { fake, services } = await connected({ ...BASE, home_directory: null });
    page(services);
    await user.type(await screen.findByRole("textbox", { name: LABEL }), "  ~/Code  ");
    await user.click(screen.getByRole("button", { name: SAVE }));
    await waitFor(() => expect(screen.getByRole("status", { name: RESULT })).toHaveTextContent("now runs in /Users/ada/Code"));
    const calls = fake.callsTo("settings.set");
    expect(calls).toHaveLength(1);
    expect(calls[0]!.params).toEqual({ idempotency_key: expect.any(String), home_directory: "~/Code" });
    expect(screen.getByRole("textbox", { name: LABEL })).toHaveValue("/Users/ada/Code");
  });

  it("home_directory_save_with_an_empty_field_clears_it", async () => {
    const user = userEvent.setup();
    const { fake, services } = await connected({ ...BASE, home_directory: "/Users/ada/Code" });
    page(services);
    await user.clear(await screen.findByRole("textbox", { name: LABEL }));
    await user.click(screen.getByRole("button", { name: SAVE }));
    await waitFor(() => expect(screen.getByRole("status", { name: RESULT })).toHaveTextContent("Cleared."));
    expect(fake.callsTo("settings.set")[0]!.params).toEqual({ idempotency_key: expect.any(String), home_directory: null });
    expect(screen.getByRole("textbox", { name: LABEL })).toHaveValue("");
  });

  it("home_directory_says_the_folder_was_not_found_when_the_daemon_refuses_it_and_keeps_what_was_typed", async () => {
    const user = userEvent.setup();
    const { fake, services } = await connected({ ...BASE, home_directory: null });
    fake.failNext("settings.set", { code: -32602, message: "invalid params" });
    page(services);
    await user.type(await screen.findByRole("textbox", { name: LABEL }), "/no/such/folder");
    await user.click(screen.getByRole("button", { name: SAVE }));
    await waitFor(() => expect(screen.getByRole("status", { name: RESULT })).toHaveTextContent(HOME_DIRECTORY_REFUSED_TEXT));
    expect(screen.getByRole("textbox", { name: LABEL })).toHaveValue("/no/such/folder");
  });

  it("home_directory_locks_the_field_and_save_with_the_approver_notice_for_a_non_approver", async () => {
    const { services } = await connected({ ...BASE, home_directory: "/Users/ada/Code" }, false);
    page(services);
    expect(await screen.findByRole("textbox", { name: LABEL })).toBeDisabled();
    expect(screen.getByRole("button", { name: SAVE })).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
  });

  it("home_directory_fixture_save_says_nothing_was_saved", async () => {
    const user = userEvent.setup();
    page(createServices({ surfaces: createFixtureSurfaces() }));
    await user.type(await screen.findByRole("textbox", { name: LABEL }), "~/Code");
    await user.click(screen.getByRole("button", { name: SAVE }));
    expect(await screen.findByText("Fixture data: nothing was saved.")).toBeInTheDocument();
  });
});
