import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { installFakeWorkstreams } from "../../../test/fakeWorkstreams";
import { createServices } from "../../createServices";
import { ServicesProvider } from "../../services";
import { NotesSection } from "./NotesSection";
import { FIXTURE_TEXT, HELPER_TEXT, SENSITIVE_TEXT } from "./notesModel";

const LIMITS = { max_notes: 10, max_bytes: 200 };
const wire = (scope: string, bodies: string[]) => ({
  scope,
  notes: bodies.map((body, position) => ({ position, body })),
  limits: LIMITS,
});

const stops: Array<() => void> = [];
afterEach(() => {
  cleanup();
  while (stops.length) stops.pop()!();
});

interface Options {
  approver?: boolean | null;
  stored?: Record<string, string[]>;
  configure?: (fake: FakeDaemon) => void;
}

/** A daemon that keeps each scope's notes, so a save is read back as the daemon holds it. */
async function section({ approver = true, stored = { global: ["Prefer small pull requests.", "Freeze starts Friday."] }, configure }: Options = {}) {
  const fake = new FakeDaemon();
  fake.approver = approver;
  fake.stub("notes.list", (params: Record<string, unknown>) => wire(String(params.scope), stored[String(params.scope)] ?? []));
  fake.stub("notes.set", (params: Record<string, unknown>) => {
    stored[String(params.scope)] = params.notes as string[];
    return wire(String(params.scope), stored[String(params.scope)]!);
  });
  configure?.(fake);
  const services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 }, data: { coalesceMs: 10 } });
  stops.push(() => {
    services.connection.stop();
    services.data.dispose();
  });
  services.connection.start();
  await until(() => services.connection.getSnapshot().status === "connected", "connected");
  render(
    <ServicesProvider services={services}>
      <NotesSection />
    </ServicesProvider>,
  );
  return { fake, stored, user: userEvent.setup() };
}

const addButton = () => screen.findByRole("button", { name: "Add note" });
const newNote = () => screen.getByRole("textbox", { name: "New note" });
const result = () => screen.getByRole("status", { name: "Notes result" });

describe("notes section", () => {
  it("notes_section_hides_quietly_without_the_capability", async () => {
    await section({ configure: (daemon) => (daemon.unknownCapabilities = ["standing_approvals"]) });
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(screen.queryByRole("heading", { name: "Notes for the orchestrator" })).not.toBeInTheDocument();
    expect(screen.queryByText(/could not be read/)).not.toBeInTheDocument();
  });

  it("notes_section_lists_the_global_notes_with_the_helper_text", async () => {
    const { fake } = await section();
    await addButton();
    expect(screen.getByText(HELPER_TEXT)).toBeInTheDocument();
    expect(HELPER_TEXT).toBe(
      "These are your own reminders. The orchestrator reads them as data, not as instructions. Do not put passwords, tokens or file paths here.",
    );
    const list = screen.getByRole("list", { name: "Notes" });
    expect(within(list).getAllByRole("listitem").map((item) => item.textContent)).toEqual([
      expect.stringContaining("Prefer small pull requests."),
      expect.stringContaining("Freeze starts Friday."),
    ]);
    expect(fake.callsTo("notes.list")[0]!.params).toEqual({ scope: "global" });
  });

  it("notes_section_add_sends_the_whole_list_in_order_and_announces_it", async () => {
    const { fake, stored, user } = await section();
    await addButton();
    await user.type(newNote(), "Ask before touching billing.");
    await user.click(screen.getByRole("button", { name: "Add note" }));
    await waitFor(() => expect(fake.callsTo("notes.set")).toHaveLength(1));
    expect(fake.callsTo("notes.set")[0]!.params).toMatchObject({
      scope: "global",
      notes: ["Prefer small pull requests.", "Freeze starts Friday.", "Ask before touching billing."],
      idempotency_key: expect.any(String),
    });
    expect(stored.global).toHaveLength(3);
    await waitFor(() => expect(result()).toHaveTextContent("Note added."));
    expect(result()).toHaveAttribute("aria-live", "polite");
    await waitFor(() => expect(within(screen.getByRole("list", { name: "Notes" })).getAllByRole("listitem")).toHaveLength(3));
    expect(newNote()).toHaveValue("");
  });

  it("notes_section_delete_sends_the_list_without_that_note", async () => {
    const { fake, user } = await section();
    await addButton();
    await user.click(screen.getByRole("button", { name: "Delete note 1" }));
    await waitFor(() => expect(fake.callsTo("notes.set")).toHaveLength(1));
    expect(fake.callsTo("notes.set")[0]!.params).toMatchObject({ scope: "global", notes: ["Freeze starts Friday."] });
    await waitFor(() => expect(result()).toHaveTextContent("Note deleted."));
    await waitFor(() => expect(within(screen.getByRole("list", { name: "Notes" })).getAllByRole("listitem")).toHaveLength(1));
  });

  it("notes_section_edit_replaces_one_line_and_keeps_the_others", async () => {
    const { fake, user } = await section();
    await addButton();
    await user.click(screen.getByRole("button", { name: "Edit note 2" }));
    const box = screen.getByRole("textbox", { name: "Edit note 2" });
    await user.clear(box);
    await user.type(box, "Freeze starts Monday.");
    await user.click(screen.getByRole("button", { name: "Save note 2" }));
    await waitFor(() => expect(fake.callsTo("notes.set")).toHaveLength(1));
    expect(fake.callsTo("notes.set")[0]!.params).toMatchObject({ notes: ["Prefer small pull requests.", "Freeze starts Monday."] });
    await waitFor(() => expect(result()).toHaveTextContent("Note changed."));
  });

  it("notes_section_a_note_over_the_byte_limit_is_refused_before_sending", async () => {
    const { fake } = await section();
    await addButton();
    // 101 characters but 202 bytes.
    fireEvent.change(newNote(), { target: { value: "é".repeat(101) } });
    expect(newNote()).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent(/202 bytes/);
    expect(screen.getByRole("button", { name: "Add note" })).toBeDisabled();
    expect(screen.getByText("202 of 200 bytes. At most 10 notes.")).toBeInTheDocument();
    expect(fake.callsTo("notes.set")).toHaveLength(0);
  });

  it("notes_section_add_is_off_when_the_scope_holds_the_most_notes", async () => {
    const full = Array.from({ length: 10 }, (_, index) => `note ${index}`);
    await section({ stored: { global: full } });
    const button = await addButton();
    expect(button).toBeDisabled();
    expect(button).toHaveAccessibleDescription(/at most 10 notes/);
  });

  it("notes_section_shows_the_daemons_refusal_in_words_and_keeps_the_list", async () => {
    const { fake, user } = await section({ configure: (daemon) => daemon.failNext("notes.set", { code: -32602, message: "invalid params" }) });
    await addButton();
    await user.type(newNote(), "something the daemon dislikes");
    await user.click(screen.getByRole("button", { name: "Add note" }));
    await waitFor(() => expect(result()).toHaveTextContent(SENSITIVE_TEXT));
    expect(fake.callsTo("notes.set")).toHaveLength(1);
    expect(within(screen.getByRole("list", { name: "Notes" })).getAllByRole("listitem")).toHaveLength(2);
    expect(newNote()).toHaveValue("something the daemon dislikes");
  });

  it("notes_section_is_locked_for_a_non_approver_with_the_notice", async () => {
    const { fake } = await section({ approver: false });
    const button = await addButton();
    expect(button).toBeDisabled();
    expect(button).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
    expect(newNote()).toBeDisabled();
    expect(screen.getByRole("button", { name: "Edit note 1" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Delete note 1" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Delete note 1" })).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
    // The list still reads.
    expect(screen.getByText("Prefer small pull requests.")).toBeInTheDocument();
    expect(fake.callsTo("notes.set")).toHaveLength(0);
  });

  it("notes_section_scope_switch_reads_that_workstreams_notes", async () => {
    const { fake, user } = await section({
      stored: { global: ["A global line."], "workstream:ws-1": ["A line for the first workstream."] },
      configure: (daemon) =>
        installFakeWorkstreams(daemon, { workstreams: [{ id: "ws-1", title: "First stream", summary: "", state: "active", revision: 1 }] }),
    });
    await addButton();
    expect(screen.getByText("A global line.")).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: "One workstream" }));
    expect(await screen.findByText("Choose a workstream to see its notes.")).toBeInTheDocument();
    expect(screen.queryByText("A global line.")).not.toBeInTheDocument();
    await waitFor(() => expect(screen.getAllByRole("option", { name: "First stream" })).toHaveLength(1));
    await user.selectOptions(screen.getByRole("combobox", { name: "Workstream" }), "ws-1");
    expect(await screen.findByText("A line for the first workstream.")).toBeInTheDocument();
    expect(fake.callsTo("notes.list").map((call) => call.params)).toContainEqual({ scope: "workstream:ws-1" });
    await user.type(newNote(), "Second line.");
    await user.click(screen.getByRole("button", { name: "Add note" }));
    await waitFor(() => expect(fake.callsTo("notes.set")).toHaveLength(1));
    expect(fake.callsTo("notes.set")[0]!.params).toMatchObject({
      scope: "workstream:ws-1",
      notes: ["A line for the first workstream.", "Second line."],
    });
    await user.click(screen.getByRole("radio", { name: "Global" }));
    expect(await screen.findByText("A global line.")).toBeInTheDocument();
  });

  it("notes_section_fixture_says_nothing_was_saved_and_calls_no_daemon_method", async () => {
    const services = createServices();
    const api = vi.spyOn(services.connection, "api");
    const user = userEvent.setup();
    render(
      <ServicesProvider services={services}>
        <NotesSection />
      </ServicesProvider>,
    );
    await addButton();
    expect(within(screen.getByRole("list", { name: "Notes" })).getAllByRole("listitem")).toHaveLength(2);
    await user.type(newNote(), "A fixture line.");
    await user.click(screen.getByRole("button", { name: "Add note" }));
    await waitFor(() => expect(result()).toHaveTextContent(FIXTURE_TEXT));
    expect(FIXTURE_TEXT).toBe("Fixture data: nothing was saved.");
    // Nothing was saved, so the list is as it was and the typed line stays.
    expect(within(screen.getByRole("list", { name: "Notes" })).getAllByRole("listitem")).toHaveLength(2);
    expect(newNote()).toHaveValue("A fixture line.");
    expect(api).not.toHaveBeenCalled();
  });
});
