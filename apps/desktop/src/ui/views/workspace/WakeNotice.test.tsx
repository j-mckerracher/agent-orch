import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { WakeRecord } from "../../../sources/surfaces/types";
import { WakeLines, WakeNotice, WakeRecords } from "./WakeNotice";

afterEach(cleanup);

const record = (extra: Partial<WakeRecord> = {}): WakeRecord => ({
  id: "wake-1",
  event: "task_failed",
  subject: "t-1",
  notice: "Task t-1 (Fix the login bug) failed.",
  state: "noticed",
  outcome: null,
  skipReason: null,
  turnId: null,
  anchorSequence: 1,
  notify: true,
  seen: false,
  createdAt: "2026-11-16T10:00:00Z",
  ...extra,
});

describe("WakeNotice", () => {
  it("wake_notice_shows_the_exact_label_and_the_event_line", () => {
    render(<WakeNotice record={record()} />);
    expect(screen.getByRole("status")).toHaveTextContent("Update, not a reply to you");
    expect(screen.getByText("Task t-1 (Fix the login bug) failed.")).toBeInTheDocument();
    render(<WakeLines text="Task t-2 failed." />);
    expect(screen.getAllByText("Update, not a reply to you")).toHaveLength(2);
  });

  it("wake_notice_names_the_agent_and_version_that_ran_a_known_update", () => {
    render(<WakeLines text="Task t-2 failed." runtime={{ name: "claude-code", version: "2.1.311" }} />);
    expect(screen.getByText("Update, not a reply to you · Claude Code 2.1.311")).toBeInTheDocument();
    expect(screen.queryByText("Update, not a reply to you")).not.toBeInTheDocument();
  });

  it("wake_notice_shows_an_unknown_agent_id_as_the_daemon_named_it", () => {
    render(<WakeLines text="Task t-2 failed." runtime={{ name: "new-agent", version: "3" }} />);
    expect(screen.getByText("Update, not a reply to you · new-agent 3")).toBeInTheDocument();
  });

  it("wake_notice_a_skipped_record_names_the_reason_in_words", () => {
    render(<WakeNotice record={record({ state: "skipped", skipReason: "mode_off" })} />);
    expect(screen.getByText("Monitoring was off")).toBeInTheDocument();
  });

  it("wake_notice_a_deferred_record_adds_no_words_beyond_its_notice", () => {
    const { container } = render(<WakeNotice record={record({ state: "pending" })} />);
    expect(container.querySelector(".ws-wake-note")).toBeNull();
  });

  it("wake_notice_a_quiet_record_collapses_to_one_line_and_hides_its_notice", () => {
    const quiet = (id: string) =>
      record({ id, event: "work_check", state: "handled", outcome: "quiet", notify: false, notice: `secret ${id}` });
    render(<WakeRecords records={[quiet("a"), quiet("b")]} />);
    expect(screen.getByText("Checked 2 updates. Nothing needs you.")).toBeInTheDocument();
    expect(screen.queryByText(/secret/)).not.toBeInTheDocument();
  });

  it("wake_records_fold_only_heartbeat_quiet_records", () => {
    const quiet = (id: string, event: string, notice: string) =>
      record({ id, event, state: "handled", outcome: "quiet", notify: false, notice });
    render(
      <WakeRecords
        records={[quiet("a", "work_check", "Checking on 1 active work units."), quiet("b", "task_failed", "Task t-2 failed.")]}
      />,
    );
    expect(screen.getByText("Checked 1 update. Nothing needs you.")).toBeInTheDocument();
    expect(screen.getByText("Task t-2 failed.")).toBeInTheDocument();
    expect(screen.queryByText(/Checking on/)).not.toBeInTheDocument();
  });

  it("wake_notice_an_unanswered_outcome_keeps_its_notice_and_adds_a_note", () => {
    render(<WakeNotice record={record({ state: "handled", outcome: "unanswered" })} />);
    expect(screen.getByText("Task t-1 (Fix the login bug) failed.")).toBeInTheDocument();
    expect(screen.getByText("No automatic explanation was given. Open the task.")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveAttribute("data-state", "handled");
  });

  it("wake_notice_state_is_in_words_not_colour", () => {
    render(<WakeNotice record={record({ state: "failed" })} />);
    expect(screen.getByText("The automatic reply failed.")).toBeInTheDocument();
  });
});
