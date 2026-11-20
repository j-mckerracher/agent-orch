import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { APPROVER_REQUIRED_NOTICE, DaemonError } from "../../../daemon/errors";
import type { CoordinatorState, ScopeReceipt } from "../../../sources/surfaces/types";
import { CoordinatorStrip } from "./CoordinatorStrip";
import { COORDINATOR_HEADING, NO_SCOPE_TEXT, REPORTS_HEADING, REQUESTS_HEADING } from "./coordinatorModel";

afterEach(cleanup);

const state = (extra: Partial<CoordinatorState> = {}): CoordinatorState => ({
  scopeSentence: "Owns the user documentation.",
  openTasks: 1,
  openRequests: 1,
  routedRequests: [{ id: "r-0123456789abcdef", work: "Write the troubleshooting page.", answered: false, createdAt: "2026-11-16T21:04:11Z" }],
  reports: [
    {
      id: "rep-2",
      author: "coordinator",
      verb: "caveat",
      requestId: "r-0123456789abcdef",
      taskId: null,
      key: null,
      note: "**Screens** may go stale.",
      createdAt: "2026-11-16T21:05:11Z",
    },
    { id: "rep-1", author: "daemon", verb: "task_done", requestId: null, taskId: "t-12ab", key: null, note: null, createdAt: "2026-11-16T21:04:11Z" },
  ],
  ...extra,
});
const receipt = (scopeSentence: string | null): ScopeReceipt => ({
  simulated: false,
  provenance: { kind: "daemon", id: "D10", methods: ["coordinator.set_scope"] },
  scopeSentence,
  notice: "Scope saved.",
});

describe("CoordinatorStrip", () => {
  it("coordinator_strip_is_hidden_when_the_field_is_absent", () => {
    render(<CoordinatorStrip snapshot={{}} lockReason={null} />);
    expect(screen.queryByRole("region", { name: COORDINATOR_HEADING })).toBeNull();
    render(<CoordinatorStrip snapshot={null} lockReason={null} />);
    expect(screen.queryByRole("region", { name: COORDINATOR_HEADING })).toBeNull();
  });

  it("coordinator_strip_with_nothing_routed_shows_no_scope_and_hides_the_empty_lists", () => {
    render(<CoordinatorStrip snapshot={{ coordinator: state({ scopeSentence: null, routedRequests: [], reports: [], openTasks: 0, openRequests: 0 }) }} lockReason={null} />);
    const region = screen.getByRole("region", { name: COORDINATOR_HEADING });
    expect(region.textContent).toContain(`Scope: ${NO_SCOPE_TEXT}`);
    expect(region.textContent).toContain("Open tasks: 0. Open routed requests: 0.");
    expect(within(region).queryByRole("list", { name: REQUESTS_HEADING })).toBeNull();
    expect(within(region).queryByRole("list", { name: REPORTS_HEADING })).toBeNull();
  });

  it("coordinator_strip_lists_open_requests_and_reports_with_the_note_as_plain_text", () => {
    render(<CoordinatorStrip snapshot={{ coordinator: state() }} lockReason={null} />);
    const region = screen.getByRole("region", { name: COORDINATOR_HEADING });
    expect(region.textContent).toContain("Scope: Owns the user documentation.");
    expect(within(region).getByRole("list", { name: REQUESTS_HEADING }).textContent).toContain(
      "r-0123456789abcdef: Write the troubleshooting page.",
    );
    const reports = within(within(region).getByRole("list", { name: REPORTS_HEADING })).getAllByRole("listitem");
    expect(reports[0]!.textContent).toContain("Coordinator: Caveat, request r-0123456789abcdef");
    expect(reports[0]!.textContent).toContain("**Screens** may go stale.");
    expect(reports[0]!.querySelector("strong")).toBeNull();
    expect(reports[1]!.textContent).toContain("App: Task done, task t-12ab");
  });

  it("coordinator_strip_saves_the_typed_scope_and_reads_the_conversation_again", async () => {
    const user = userEvent.setup();
    const onSaveScope = vi.fn(async (sentence: string | null) => receipt(sentence));
    const onSaved = vi.fn();
    render(<CoordinatorStrip snapshot={{ coordinator: state() }} lockReason={null} onSaveScope={onSaveScope} onSaved={onSaved} />);
    const box = screen.getByRole("textbox", { name: "Scope sentence" });
    expect(box).toHaveValue("Owns the user documentation.");
    await user.clear(box);
    await user.type(box, "  Owns docs and release notes.  ");
    await user.click(screen.getByRole("button", { name: "Save scope" }));
    expect(onSaveScope).toHaveBeenCalledWith("Owns docs and release notes.");
    await waitFor(() => expect(onSaved).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("Scope saved.")).toBeInTheDocument();

    await user.clear(box);
    await user.click(screen.getByRole("button", { name: "Save scope" }));
    expect(onSaveScope).toHaveBeenLastCalledWith(null);
  });

  it("coordinator_strip_blocks_a_scope_that_is_too_long_and_says_why", async () => {
    const user = userEvent.setup();
    const onSaveScope = vi.fn(async (sentence: string | null) => receipt(sentence));
    render(<CoordinatorStrip snapshot={{ coordinator: state() }} lockReason={null} onSaveScope={onSaveScope} />);
    const box = screen.getByRole("textbox", { name: "Scope sentence" });
    await user.clear(box);
    await user.click(box);
    await user.paste("é".repeat(151));
    expect(screen.getByRole("button", { name: "Save scope" })).toBeDisabled();
    expect(box).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByText(/too long: 302 bytes/)).toBeInTheDocument();
    expect(onSaveScope).not.toHaveBeenCalled();
  });

  it("coordinator_strip_disables_the_scope_editor_for_a_non_approver_session", () => {
    render(<CoordinatorStrip snapshot={{ coordinator: state() }} lockReason={APPROVER_REQUIRED_NOTICE} onSaveScope={vi.fn()} />);
    expect(screen.getByRole("textbox", { name: "Scope sentence" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Save scope" })).toBeDisabled();
    expect(screen.getByText(APPROVER_REQUIRED_NOTICE)).toBeInTheDocument();
  });

  it("coordinator_strip_says_the_daemon_refused_an_invalid_scope", async () => {
    const user = userEvent.setup();
    const onSaveScope = vi.fn(async () => {
      throw new DaemonError("invalid-params", "invalid params", -32602);
    });
    const onSaved = vi.fn();
    render(<CoordinatorStrip snapshot={{ coordinator: state() }} lockReason={null} onSaveScope={onSaveScope} onSaved={onSaved} />);
    await user.click(screen.getByRole("button", { name: "Save scope" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/The daemon refused this scope/);
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("coordinator_strip_holds_save_pending_until_the_daemon_answers", async () => {
    const user = userEvent.setup();
    let finish: () => void = () => {};
    const onSaveScope = vi.fn((sentence: string | null) => new Promise<ReturnType<typeof receipt>>((resolve) => (finish = () => resolve(receipt(sentence)))));
    render(<CoordinatorStrip snapshot={{ coordinator: state() }} lockReason={null} onSaveScope={onSaveScope} />);
    const save = screen.getByRole("button", { name: "Save scope" });
    await user.click(save);
    expect(save).toHaveAttribute("aria-busy", "true");
    await user.click(save);
    expect(onSaveScope).toHaveBeenCalledTimes(1);
    finish();
    expect(await screen.findByText("Scope saved.")).toBeInTheDocument();
    expect(save).not.toHaveAttribute("aria-busy", "true");
  });
});
