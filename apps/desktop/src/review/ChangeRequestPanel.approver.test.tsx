import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { APPROVER_REQUIRED_NOTICE } from "../daemon/errors";
import type { Approval } from "../daemon/types";
import { RequestChangesForm, useChangeRequests } from "./ChangeRequestPanel";
import type { ChangeRequestInput, ChangeRequestSource } from "./sources";
import { approval } from "./testing";

function recordingSource() {
  const submitted: ChangeRequestInput[] = [];
  const source: ChangeRequestSource = {
    provenance: "daemon",
    labels: { description: "Ask for changes.", submitAction: "Send feedback", receiptNote: "" },
    loadHistory: async () => ({ state: "available", items: [] }),
    submit: async (input) => {
      submitted.push(input);
      return {
        provenance: "daemon",
        requestId: "cr-1",
        taskId: input.taskId,
        candidateCommit: input.candidateCommit,
        feedback: input.feedback,
        resultingState: "changes requested",
        submittedAt: "now",
      };
    },
  };
  return { source, submitted };
}

function Panel({ source, approval, approver }: { source: ChangeRequestSource; approval: Approval; approver: boolean }) {
  const requests = useChangeRequests(source, approval);
  return <RequestChangesForm source={source} requests={requests} approval={approval} approver={approver} />;
}

describe("RequestChangesForm approver policy (DPES-1105)", () => {
  it("the_change_request_panel_shows_the_approver_notice_and_disables_send_without_the_secret", async () => {
    const { source, submitted } = recordingSource();
    render(<Panel source={source} approval={approval()} approver={false} />);
    const panel = screen.getByRole("region", { name: "Request changes" });
    expect(within(panel).getByText(APPROVER_REQUIRED_NOTICE)).toBeInTheDocument();
    await userEvent.type(within(panel).getByLabelText(/Feedback/), "please add tests");
    expect(within(panel).getByRole("button", { name: "Send feedback" })).toBeDisabled();
    expect(submitted).toEqual([]);
  });

  it("the_change_request_panel_is_unchanged_when_the_session_is_an_approver", async () => {
    const { source, submitted } = recordingSource();
    const decision = approval();
    render(<Panel source={source} approval={decision} approver={true} />);
    const panel = screen.getByRole("region", { name: "Request changes" });
    expect(within(panel).queryByText(APPROVER_REQUIRED_NOTICE)).toBeNull();
    await userEvent.type(within(panel).getByLabelText(/Feedback/), "please add tests");
    await userEvent.click(within(panel).getByRole("button", { name: "Send feedback" }));
    expect(within(panel).getByRole("button", { name: "Send feedback" })).toHaveAttribute("aria-busy", "true");
    await within(panel).findByText(/Receipt cr-1: changes requested/);
    expect(submitted).toEqual([
      {
        taskId: decision.action.task_id,
        approvalId: decision.id,
        candidateCommit: decision.action.candidate_commit,
        feedback: "please add tests",
      },
    ]);
  });

  it("a_refused_change_request_says_it_was_not_sent_beside_the_button_and_keeps_the_feedback", async () => {
    const { source } = recordingSource();
    const refusing: ChangeRequestSource = { ...source, submit: async () => Promise.reject(new Error("The daemon did not answer.")) };
    render(<Panel source={refusing} approval={approval()} approver={true} />);
    const panel = screen.getByRole("region", { name: "Request changes" });
    await userEvent.type(within(panel).getByLabelText(/Feedback/), "please add tests");
    await userEvent.click(within(panel).getByRole("button", { name: "Send feedback" }));
    const alert = await within(panel).findByRole("alert");
    expect(alert).toHaveTextContent("Your feedback was not sent. The daemon did not answer.");
    expect(within(alert).getByRole("button", { name: "Retry" })).toBeTruthy();
    expect(within(panel).getByLabelText(/Feedback/)).toHaveValue("please add tests");
  });
});
