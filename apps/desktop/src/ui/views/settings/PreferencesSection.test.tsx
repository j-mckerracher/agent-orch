import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import type { MemoryArchivedWire, MemoryEntryWire } from "../../../daemon/types";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { installFakeWorkstreams } from "../../../test/fakeWorkstreams";
import { createServices } from "../../createServices";
import { ServicesProvider } from "../../services";
import { PreferencesSection } from "./PreferencesSection";
import { FIXTURE_TEXT, REFUSED_TEXT, SECTION_TITLE } from "./preferencesModel";

const DAY = 86_400_000;
const NOW = Date.UTC(2026, 10, 19);

interface Doc {
  entries: MemoryEntryWire[];
  archived: MemoryArchivedWire[];
}

const entry = (key: string, text: string, over: Partial<MemoryEntryWire> = {}): MemoryEntryWire => ({
  key,
  text,
  written_by: "user",
  created_at_ms: NOW - 10 * DAY,
  confirmed_at_ms: NOW - DAY,
  stale: false,
  ...over,
});

const bytes = (doc: Doc) => doc.entries.reduce((sum, item) => sum + new TextEncoder().encode(item.key + item.text).length, 0);
const budgetOf = (kind: string) => (kind === "learning" ? 1024 : 2048);

const stops: Array<() => void> = [];
afterEach(() => {
  cleanup();
  while (stops.length) stops.pop()!();
});

interface Options {
  approver?: boolean | null;
  docs?: Record<string, Doc>;
  configure?: (fake: FakeDaemon) => void;
  memory?: boolean;
}

/** A daemon that keeps each (kind, scope) document, so a change is read back as the daemon holds it. */
async function section({ approver = true, docs = {}, configure, memory = true }: Options = {}) {
  const fake = new FakeDaemon();
  fake.approver = approver;
  const doc = (kind: unknown, scope: unknown): Doc => (docs[`${kind}|${scope}`] ??= { entries: [], archived: [] });
  let nextArchive = 100;
  if (memory) {
    fake.stub("memory.list", (params: Record<string, unknown>) => {
      const found = doc(params.kind, params.scope);
      return {
        kind: params.kind,
        scope: params.scope,
        entries: found.entries,
        bytes: bytes(found),
        budget: budgetOf(String(params.kind)),
        archived: found.archived,
        limits: { max_key_bytes: 40, max_text_bytes: 280, stale_after_days: params.kind === "learning" ? 180 : null },
      };
    });
    fake.stub("memory.set", (params: Record<string, unknown>) => {
      const found = doc(params.kind, params.scope);
      const key = String(params.key);
      const text = String(params.text);
      const old = found.entries.find((item) => item.key === key);
      const budget = budgetOf(String(params.kind));
      const after = bytes(found) - (old ? new TextEncoder().encode(old.key + old.text).length : 0) + new TextEncoder().encode(key + text).length;
      const reply = (outcome: string) => ({ kind: params.kind, scope: params.scope, key, outcome, bytes: after, budget });
      if (after > budget) return reply("over_budget");
      if (old && old.text === text) {
        old.confirmed_at_ms = NOW;
        old.stale = false;
        return reply("confirmed");
      }
      if (old) {
        found.archived.unshift({ id: nextArchive++, key, text: old.text, written_by: old.written_by, archived_at_ms: NOW, reason: "replaced", restored: false });
        old.text = text;
        return reply("updated");
      }
      found.entries.unshift(entry(key, text, { confirmed_at_ms: NOW }));
      return reply("created");
    });
    fake.stub("memory.remove", (params: Record<string, unknown>) => {
      const found = doc(params.kind, params.scope);
      const index = found.entries.findIndex((item) => item.key === params.key);
      if (index >= 0) {
        const [gone] = found.entries.splice(index, 1);
        found.archived.unshift({ id: nextArchive++, key: gone!.key, text: gone!.text, written_by: gone!.written_by, archived_at_ms: NOW, reason: "removed", restored: false });
      }
      return { kind: params.kind, scope: params.scope, key: params.key, removed: index >= 0 };
    });
    fake.stub("memory.restore", (params: Record<string, unknown>) => {
      for (const found of Object.values(docs)) {
        const row = found.archived.find((item) => item.id === params.archive_id);
        if (row) {
          row.restored = true;
          found.entries.unshift(entry(row.key, row.text, { confirmed_at_ms: NOW }));
        }
      }
      return { archive_id: params.archive_id, outcome: "restored" };
    });
  }
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
      <PreferencesSection />
    </ServicesProvider>,
  );
  return { fake, docs, user: userEvent.setup() };
}

const SHARED = (entries: MemoryEntryWire[], archived: MemoryArchivedWire[] = []) => ({ "preference|global": { entries, archived } });
const addShared = () => screen.findByRole("button", { name: "Add shared preference" });
const sharedResult = () => screen.getByRole("status", { name: "Shared preferences result" });
const sharedList = () => screen.getByRole("list", { name: "Shared preferences" });

describe("preferences section", () => {
  it("hides_quietly_without_the_capability", async () => {
    await section({ configure: (daemon) => (daemon.unknownCapabilities = ["standing_approvals"]) });
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(screen.queryByRole("heading", { name: SECTION_TITLE })).not.toBeInTheDocument();
  });

  it("hides_quietly_when_the_daemon_has_the_capability_but_no_memory_methods", async () => {
    const { fake } = await section({ memory: false });
    await waitFor(() => expect(fake.callsTo("memory.list").length).toBeGreaterThan(0));
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(screen.queryByRole("heading", { name: SECTION_TITLE })).not.toBeInTheDocument();
    expect(screen.queryByText(/could not be read/)).not.toBeInTheDocument();
  });

  it("lists_shared_preferences_with_author_date_and_byte_use_and_reads_global_learnings", async () => {
    const { fake } = await section({
      docs: {
        ...SHARED([entry("small-prs", "Prefer small pull requests."), entry("tests-first", "Test first.", { written_by: "orchestrator" })]),
        "learning|global": { entries: [entry("ci-vpn", "CI needs the VPN.", { written_by: "orchestrator", stale: true })], archived: [] },
      },
    });
    await addShared();
    expect(screen.getByRole("heading", { name: SECTION_TITLE })).toBeInTheDocument();
    expect(screen.getByText(/Every conversation follows them unless a message says otherwise/)).toBeInTheDocument();
    const items = within(sharedList()).getAllByRole("listitem");
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveTextContent("small-prs");
    expect(items[0]).toHaveTextContent("Prefer small pull requests.");
    expect(items[0]).toHaveTextContent("Written by the user.");
    expect(items[1]).toHaveTextContent("Written by the orchestrator.");
    expect(screen.getByText("58 of 2048 bytes used.")).toBeInTheDocument();
    expect(fake.callsTo("memory.list").map((call) => call.params)).toEqual(
      expect.arrayContaining([
        { kind: "preference", scope: "global" },
        { kind: "learning", scope: "global" },
      ]),
    );
    // The learning is stale and can be kept.
    const learnings = await screen.findByRole("list", { name: "Learnings" });
    expect(within(learnings).getByText("Stale")).toBeInTheDocument();
    expect(screen.getByText(/confirmed for 180 days is marked stale/)).toBeInTheDocument();
    expect(screen.getByText("Choose a project or workstream to see its preferences.")).toBeInTheDocument();
  });

  it("add_sends_the_key_and_text_with_an_idempotency_key_and_announces_it", async () => {
    const { fake, user } = await section({ docs: SHARED([entry("small-prs", "Prefer small pull requests.")]) });
    await addShared();
    await user.type(screen.getByRole("textbox", { name: "New shared preference key" }), "ask-billing");
    await user.type(screen.getByRole("textbox", { name: "New shared preference text" }), "Ask before touching billing.");
    expect(screen.getByText(/With this entry: 75 of 2048 bytes/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Add shared preference" }));
    await waitFor(() => expect(fake.callsTo("memory.set")).toHaveLength(1));
    expect(fake.callsTo("memory.set")[0]!.params).toEqual({
      kind: "preference",
      scope: "global",
      key: "ask-billing",
      text: "Ask before touching billing.",
      idempotency_key: expect.any(String),
    });
    await waitFor(() => expect(sharedResult()).toHaveTextContent("Added ask-billing."));
    await waitFor(() => expect(within(sharedList()).getAllByRole("listitem")).toHaveLength(2));
    expect(screen.getByRole("textbox", { name: "New shared preference key" })).toHaveValue("");
  });

  it("a_bad_key_or_text_is_refused_before_sending", async () => {
    const { fake } = await section();
    await addShared();
    fireEvent.change(screen.getByRole("textbox", { name: "New shared preference key" }), { target: { value: "Bad Key" } });
    expect(screen.getByRole("alert")).toHaveTextContent(/lowercase letters, digits and hyphens/);
    fireEvent.change(screen.getByRole("textbox", { name: "New shared preference key" }), { target: { value: "ok" } });
    fireEvent.change(screen.getByRole("textbox", { name: "New shared preference text" }), { target: { value: "é".repeat(141) } });
    expect(screen.getByRole("alert")).toHaveTextContent(/282 bytes/);
    expect(screen.getByRole("button", { name: "Add shared preference" })).toBeDisabled();
    expect(fake.callsTo("memory.set")).toHaveLength(0);
  });

  it("edit_keeps_the_key_and_the_old_text_goes_to_the_archive", async () => {
    const { fake, user } = await section({ docs: SHARED([entry("small-prs", "Prefer small pull requests.")]) });
    await addShared();
    await user.click(screen.getByRole("button", { name: "Edit shared preference small-prs" }));
    const box = screen.getByRole("textbox", { name: "Text for small-prs" });
    await user.clear(box);
    await user.type(box, "Prefer tiny pull requests.");
    await user.click(screen.getByRole("button", { name: "Save shared preference small-prs" }));
    await waitFor(() => expect(fake.callsTo("memory.set")).toHaveLength(1));
    expect(fake.callsTo("memory.set")[0]!.params).toMatchObject({ key: "small-prs", text: "Prefer tiny pull requests." });
    await waitFor(() => expect(sharedResult()).toHaveTextContent("Changed small-prs. The old text is under Removed entries."));
    expect(await screen.findByText("Removed entries (1)")).toBeInTheDocument();
    expect(within(sharedList()).getByText("Prefer tiny pull requests.")).toBeInTheDocument();
  });

  it("keep_on_a_stale_entry_sends_the_same_text_and_confirms_it", async () => {
    const { fake, user } = await section({
      docs: { "learning|global": { entries: [entry("ci-vpn", "CI needs the VPN.", { written_by: "orchestrator", stale: true })], archived: [] } },
    });
    const keep = await screen.findByRole("button", { name: "Keep learning ci-vpn" });
    await user.click(keep);
    await waitFor(() => expect(fake.callsTo("memory.set")).toHaveLength(1));
    expect(fake.callsTo("memory.set")[0]!.params).toMatchObject({ kind: "learning", scope: "global", key: "ci-vpn", text: "CI needs the VPN." });
    await waitFor(() => expect(screen.getByRole("status", { name: "Learnings result" })).toHaveTextContent("Kept ci-vpn."));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Keep learning ci-vpn" })).not.toBeInTheDocument());
  });

  it("over_budget_from_the_daemon_shows_the_message_and_keeps_the_form", async () => {
    const { user } = await section({
      configure: (daemon) =>
        daemon.stub("memory.set", (params: Record<string, unknown>) => ({ ...params, outcome: "over_budget", bytes: 2100, budget: 2048 })),
    });
    await addShared();
    await user.type(screen.getByRole("textbox", { name: "New shared preference key" }), "big");
    await user.type(screen.getByRole("textbox", { name: "New shared preference text" }), "Something long.");
    await user.click(screen.getByRole("button", { name: "Add shared preference" }));
    await waitFor(() => expect(sharedResult()).toHaveTextContent("Over the budget: 2100 of 2048 bytes. Shorten or remove an entry."));
    expect(screen.getByRole("textbox", { name: "New shared preference key" })).toHaveValue("big");
    expect(screen.getByRole("textbox", { name: "New shared preference text" })).toHaveValue("Something long.");
  });

  it("a_write_over_the_budget_is_refused_before_sending", async () => {
    const { fake } = await section({ docs: SHARED([entry("long", "x".repeat(270)), ...Array.from({ length: 6 }, (_, i) => entry(`k${i}`, "y".repeat(270)))]) });
    await addShared();
    fireEvent.change(screen.getByRole("textbox", { name: "New shared preference key" }), { target: { value: "more" } });
    fireEvent.change(screen.getByRole("textbox", { name: "New shared preference text" }), { target: { value: "z".repeat(200) } });
    const button = screen.getByRole("button", { name: "Add shared preference" });
    expect(button).toBeDisabled();
    expect(button).toHaveAccessibleDescription(/^Over the budget: \d+ of 2048 bytes\. Shorten or remove an entry\.$/);
    expect(fake.callsTo("memory.set")).toHaveLength(0);
  });

  it("the_daemons_refusal_is_shown_in_words", async () => {
    const { user } = await section({ configure: (daemon) => daemon.failNext("memory.set", { code: -32602, message: "invalid params" }) });
    await addShared();
    await user.type(screen.getByRole("textbox", { name: "New shared preference key" }), "token");
    await user.type(screen.getByRole("textbox", { name: "New shared preference text" }), "something the daemon dislikes");
    await user.click(screen.getByRole("button", { name: "Add shared preference" }));
    await waitFor(() => expect(sharedResult()).toHaveTextContent(REFUSED_TEXT));
    expect(screen.getByRole("textbox", { name: "New shared preference text" })).toHaveValue("something the daemon dislikes");
  });

  it("remove_moves_the_entry_to_the_archive_and_restore_brings_it_back", async () => {
    const { fake, user } = await section({ docs: SHARED([entry("small-prs", "Prefer small pull requests.")]) });
    await addShared();
    await user.click(screen.getByRole("button", { name: "Remove shared preference small-prs" }));
    await waitFor(() => expect(fake.callsTo("memory.remove")).toHaveLength(1));
    expect(fake.callsTo("memory.remove")[0]!.params).toEqual({ kind: "preference", scope: "global", key: "small-prs", idempotency_key: expect.any(String) });
    await waitFor(() => expect(sharedResult()).toHaveTextContent("Removed small-prs. It is under Removed entries."));
    await user.click(await screen.findByText("Removed entries (1)"));
    await user.click(screen.getByRole("button", { name: "Restore shared preference small-prs" }));
    await waitFor(() => expect(fake.callsTo("memory.restore")).toHaveLength(1));
    expect(fake.callsTo("memory.restore")[0]!.params).toEqual({ archive_id: 100, idempotency_key: expect.any(String) });
    await waitFor(() => expect(sharedResult()).toHaveTextContent("Restored small-prs."));
    await waitFor(() => expect(within(sharedList()).getAllByRole("listitem")).toHaveLength(1));
    expect(screen.queryByRole("button", { name: "Restore shared preference small-prs" })).not.toBeInTheDocument();
  });

  it("a_restore_conflict_says_the_key_is_in_use_again", async () => {
    const { user } = await section({
      docs: SHARED([], [{ id: 5, key: "old", text: "Old text.", written_by: "user", archived_at_ms: NOW, reason: "removed", restored: false }]),
      configure: (daemon) => daemon.failNext("memory.restore", { code: -32011, message: "conflict" }),
    });
    await addShared();
    await user.click(screen.getByText("Removed entries (1)"));
    await user.click(screen.getByRole("button", { name: "Restore shared preference old" }));
    await waitFor(() => expect(sharedResult()).toHaveTextContent(/key is in use again, or it was already restored/));
  });

  it("is_read_only_for_a_non_approver_with_the_notice", async () => {
    const { fake } = await section({ approver: false, docs: SHARED([entry("small-prs", "Prefer small pull requests.")]) });
    const button = await addShared();
    expect(button).toBeDisabled();
    expect(button).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
    expect(screen.getByRole("textbox", { name: "New shared preference key" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Edit shared preference small-prs" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Remove shared preference small-prs" })).toHaveAccessibleDescription(APPROVER_REQUIRED_NOTICE);
    expect(screen.getByText("Prefer small pull requests.")).toBeInTheDocument();
    expect(fake.callsTo("memory.set")).toHaveLength(0);
  });

  it("the_project_and_workstream_picker_reads_that_document_with_the_raw_project_id", async () => {
    const PROJECT = "/Users/me/code/shop";
    const { fake, user } = await section({
      docs: {
        [`preference|project:${PROJECT}`]: { entries: [entry("api-review", "Ask before changing the public API.")], archived: [] },
        "preference|workstream:ws-1": { entries: [entry("stream-rule", "Ship behind a flag.")], archived: [] },
      },
      configure: (daemon) => {
        daemon.projects = [{ id: PROJECT }];
        installFakeWorkstreams(daemon, { workstreams: [{ id: "ws-1", title: "First stream", summary: "", state: "active", revision: 1 }] });
      },
    });
    await addShared();
    const picker = screen.getByRole("combobox", { name: "Preferences for" });
    await waitFor(() => expect(within(picker).getAllByRole("option", { name: "shop" })).toHaveLength(1));
    await user.selectOptions(picker, `project:${PROJECT}`);
    expect(await screen.findByText("Ask before changing the public API.")).toBeInTheDocument();
    expect(fake.callsTo("memory.list").map((call) => call.params)).toContainEqual({ kind: "preference", scope: `project:${PROJECT}` });
    await waitFor(() => expect(within(picker).getAllByRole("option", { name: "First stream" })).toHaveLength(1));
    await user.selectOptions(picker, "workstream:ws-1");
    expect(await screen.findByText("Ship behind a flag.")).toBeInTheDocument();
    await user.type(screen.getByRole("textbox", { name: "New project or workstream preference key" }), "second");
    await user.type(screen.getByRole("textbox", { name: "New project or workstream preference text" }), "Second line.");
    await user.click(screen.getByRole("button", { name: "Add project or workstream preference" }));
    await waitFor(() => expect(fake.callsTo("memory.set")).toHaveLength(1));
    expect(fake.callsTo("memory.set")[0]!.params).toMatchObject({ kind: "preference", scope: "workstream:ws-1", key: "second" });
    // The learnings picker offers Global, the project and the workstream.
    const learningsPicker = screen.getByRole("combobox", { name: "Learnings for" });
    expect(within(learningsPicker).getAllByRole("option").map((option) => option.textContent)).toEqual(["Global", "shop", "First stream"]);
  });

  it("fixture_says_nothing_was_saved_and_calls_no_daemon_method", async () => {
    const services = createServices();
    const api = vi.spyOn(services.connection, "api");
    const user = userEvent.setup();
    render(
      <ServicesProvider services={services}>
        <PreferencesSection />
      </ServicesProvider>,
    );
    await addShared();
    expect(within(sharedList()).getAllByRole("listitem")).toHaveLength(2);
    await user.type(screen.getByRole("textbox", { name: "New shared preference key" }), "fixture-line");
    await user.type(screen.getByRole("textbox", { name: "New shared preference text" }), "A fixture line.");
    await user.click(screen.getByRole("button", { name: "Add shared preference" }));
    await waitFor(() => expect(sharedResult()).toHaveTextContent(FIXTURE_TEXT));
    expect(within(sharedList()).getAllByRole("listitem")).toHaveLength(2);
    expect(screen.getByRole("textbox", { name: "New shared preference text" })).toHaveValue("A fixture line.");
    expect(api).not.toHaveBeenCalled();
  });
});
