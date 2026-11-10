/** S5: coordinator view. The conversation is D5 placeholder data; related tasks and approvals are real. */
import { useState } from "react";
import { asDaemonError, type DaemonError } from "../daemon/errors";
import type { CoordinatorMessage, ProposedTask, SimulatedReceipt } from "../sources/types";
import { Crumbs } from "./ConnectionStrip";
import { basename, pendingApprovals } from "./format";
import { useGate, useServices, useSource, useStore } from "./services";
import { Button } from "./theme";
import { ErrorNotice, LoadableRegion, Notice, SimulatedReceipts, SourceSection, StateText } from "./widgets";

export function Coordinator({ projectId }: { projectId: string }) {
  const { data, router, drafts, sources } = useServices();
  const { tasks, approvals } = useStore(data);
  const draft = useStore(drafts).coordinator[projectId] ?? "";
  const gate = useGate();
  const real = sources.coordinator.provenance.kind === "daemon" && !!sources.coordinator.send && !!sources.coordinator.propose;
  const [nonce, setNonce] = useState(0);
  const conversation = useSource(
    () => sources.coordinator.conversation(projectId),
    `conversation:${projectId}:${nonce}`,
    sources.coordinator.provenance,
  );
  const [replies, setReplies] = useState<CoordinatorMessage[]>([]);
  const [proposals, setProposals] = useState<ProposedTask[]>([]);
  const [receipts, setReceipts] = useState<SimulatedReceipt[]>([]);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [failure, setFailure] = useState<DaemonError | null>(null);

  const submit = async (what: "message" | "proposal") => {
    const call = what === "message" ? sources.coordinator.send : sources.coordinator.propose;
    if (!call) return;
    setBusy(true);
    setStatus(null);
    setFailure(null);
    try {
      await call.call(sources.coordinator, projectId, draft);
      setStatus(
        what === "message"
          ? "Message recorded. Nothing acts on it automatically."
          : "Proposal validated and recorded. No task was created.",
      );
      setNonce((n) => n + 1);
    } catch (error) {
      setFailure(asDaemonError(error));
    } finally {
      setBusy(false);
    }
  };

  const own = (tasks.data ?? []).filter((task) => task.contract.project_id === projectId);
  const ownIds = new Set(own.map((task) => task.contract.id));
  const pending = pendingApprovals(approvals.data ?? []).filter((a) => ownIds.has(a.action.task_id));

  return (
    <main className="ps-page">
      <Crumbs
        items={[
          { label: "Projects", onClick: () => router.navigate({ name: "projects" }) },
          { label: basename(projectId), onClick: () => router.navigate({ name: "project", projectId }) },
          { label: "Coordinator" },
        ]}
      />
      <h1>Coordinator</h1>
      {!real && (
        <Notice tone="warn" role="status">
          The coordinator conversation service is unavailable (D5). Everything below it is a preview.
        </Notice>
      )}
      <SourceSection
        title="Coordinator conversation"
        provenance={sources.coordinator.provenance}
        result={conversation}
        emptyText="No conversation yet."
      >
        {(convo) => (
          <div>
            <ul aria-label="Messages" className="ps-messages">
              {[...convo.messages, ...replies].map((message, index) => (
                <li key={`${index}:${message.id}`} data-role={message.role}>
                  <strong>{message.role}</strong>: {message.text}
                </li>
              ))}
            </ul>
            <h4>Structured proposals</h4>
            <ul aria-label="Proposals" className="ps-list">
              {[...convo.proposals, ...proposals].map((proposal, index) => (
                <li key={`${index}:${proposal.id}`}>
                  {proposal.objective} — validation {proposal.validation}
                  {proposal.validationErrors.length > 0 && `: ${proposal.validationErrors.join("; ")}`}
                  {proposal.dependencies.length > 0 && ` (depends on ${proposal.dependencies.join(", ")})`}; suggested
                  delivery {proposal.suggestedDelivery}
                </li>
              ))}
            </ul>
            {convo.outstandingDecisions.length > 0 && (
              <ul aria-label="Outstanding decisions" className="ps-steps">
                {convo.outstandingDecisions.map((text) => (
                  <li key={text}>{text}</li>
                ))}
              </ul>
            )}
          </div>
        )}
      </SourceSection>
      <section aria-label="Objective composer" className="ps-section ps-composer">
        <label>
          {real ? "Message or proposal JSON" : "Draft objective"}
          <textarea value={draft} onChange={(event) => drafts.setCoordinator(projectId, event.target.value)} />
        </label>
        {real ? (
          <div>
            <div className="ps-actions">
              <Button
                variant="primary"
                disabled={busy || !gate.can("coordinator") || draft.trim() === ""}
                onClick={() => void submit("message")}
              >
                Send message
              </Button>
              <Button
                disabled={busy || !gate.can("coordinator") || draft.trim() === ""}
                onClick={() => void submit("proposal")}
              >
                Validate proposal
              </Button>
            </div>
            {status && <p role="status">{status}</p>}
            {failure && <ErrorNotice error={failure} action="Contacting the coordinator" />}
            {failure?.indeterminate && (
              <p role="status">The outcome is unknown. Submitting the same text again reuses the same request key.</p>
            )}
          </div>
        ) : (
          <div>
            <div className="ps-actions">
              <Button
                onClick={() =>
                  void sources.coordinator.previewReply(projectId, draft).then((r) => {
                    setReplies((list) => [...list, r.message]);
                    setReceipts((list) => [...list, r]);
                  })
                }
              >
                Preview reply
              </Button>
              <Button
                onClick={() =>
                  void sources.coordinator.previewProposal(projectId, draft).then((r) => {
                    setProposals((list) => [...list, r.proposal]);
                    setReceipts((list) => [...list, r]);
                  })
                }
              >
                Preview proposed tasks
              </Button>
            </div>
            <SimulatedReceipts label="Simulated receipts" receipts={receipts} />
          </div>
        )}
      </section>

      <section aria-label="Related live work" className="ps-section">
        <h2>Related live tasks and decisions</h2>
        <LoadableRegion
          what="tasks"
          loadable={tasks}
          connected={gate.connected}
          isEmpty={() => own.length === 0 && pending.length === 0}
          emptyText="No live tasks or pending decisions for this project."
          onRetry={() => void data.refreshAll()}
        >
          {() => (
            <div>
              <ul aria-label="Related tasks" className="ps-list">
                {own.map((task) => (
                  <li key={task.contract.id} className="ps-project-row">
                    <code>{task.contract.id}</code> {task.contract.objective} <StateText state={task.state} />{" "}
                    <Button onClick={() => router.navigate({ name: "task", projectId, taskId: task.contract.id })}>
                      Open task
                    </Button>
                  </li>
                ))}
              </ul>
              <ul aria-label="Pending decisions" className="ps-list">
                {pending.map((approval) => (
                  <li key={approval.id} className="ps-project-row">
                    Approval <code>{approval.id}</code> for task <code>{approval.action.task_id}</code>{" "}
                    <Button
                      onClick={() => router.navigate({ name: "review", projectId, taskId: approval.action.task_id })}
                    >
                      Open review
                    </Button>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </LoadableRegion>
      </section>
    </main>
  );
}
