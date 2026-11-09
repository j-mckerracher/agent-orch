/** S5: coordinator view. The conversation is D5 placeholder data; related tasks and approvals are real. */
import { useState } from "react";
import type { CoordinatorMessage, ProposedTask } from "../sources/types";
import { Crumbs } from "./ConnectionStrip";
import { basename, pendingApprovals } from "./format";
import { useGate, useServices, useSource, useStore } from "./services";
import { LoadableRegion, SourceSection } from "./widgets";

export function Coordinator({ projectId }: { projectId: string }) {
  const { data, router, drafts, sources } = useServices();
  const { tasks, approvals } = useStore(data);
  const draft = useStore(drafts).coordinator[projectId] ?? "";
  const gate = useGate();
  const conversation = useSource(() => sources.coordinator.conversation(projectId), `conversation:${projectId}`);
  const [replies, setReplies] = useState<CoordinatorMessage[]>([]);
  const [proposals, setProposals] = useState<ProposedTask[]>([]);
  const [receipts, setReceipts] = useState<string[]>([]);

  const own = (tasks.data ?? []).filter((task) => task.contract.project_id === projectId);
  const ownIds = new Set(own.map((task) => task.contract.id));
  const pending = pendingApprovals(approvals.data ?? []).filter((a) => ownIds.has(a.action.task_id));

  return (
    <main>
      <Crumbs
        items={[
          { label: "Projects", onClick: () => router.navigate({ name: "projects" }) },
          { label: basename(projectId), onClick: () => router.navigate({ name: "project", projectId }) },
          { label: "Coordinator" },
        ]}
      />
      <h1>Coordinator</h1>
      <p role="status">The coordinator conversation service is unavailable (D5). Everything below it is a preview.</p>
      <SourceSection
        title="Coordinator conversation"
        provenance={sources.coordinator.provenance}
        result={conversation}
        emptyText="No conversation yet."
      >
        {(convo) => (
          <div>
            <ul aria-label="Messages">
              {[...convo.messages, ...replies].map((message) => (
                <li key={message.id}>
                  <strong>{message.role}</strong>: {message.text}
                </li>
              ))}
            </ul>
            <h4>Structured proposals</h4>
            <ul aria-label="Proposals">
              {[...convo.proposals, ...proposals].map((proposal) => (
                <li key={proposal.id}>
                  {proposal.objective} — validation {proposal.validation}
                  {proposal.validationErrors.length > 0 && `: ${proposal.validationErrors.join("; ")}`}
                  {proposal.dependencies.length > 0 && ` (depends on ${proposal.dependencies.join(", ")})`}; suggested
                  delivery {proposal.suggestedDelivery}
                </li>
              ))}
            </ul>
            {convo.outstandingDecisions.length > 0 && (
              <ul aria-label="Outstanding decisions">
                {convo.outstandingDecisions.map((text) => (
                  <li key={text}>{text}</li>
                ))}
              </ul>
            )}
          </div>
        )}
      </SourceSection>
      <section aria-label="Objective composer">
        <label>
          Draft objective
          <textarea value={draft} onChange={(event) => drafts.setCoordinator(projectId, event.target.value)} />
        </label>{" "}
        <button
          type="button"
          onClick={() =>
            void sources.coordinator.previewReply(projectId, draft).then((r) => {
              setReplies((list) => [...list, r.message]);
              setReceipts((list) => [...list, r.notice]);
            })
          }
        >
          Preview reply
        </button>{" "}
        <button
          type="button"
          onClick={() =>
            void sources.coordinator.previewProposal(projectId, draft).then((r) => {
              setProposals((list) => [...list, r.proposal]);
              setReceipts((list) => [...list, r.notice]);
            })
          }
        >
          Preview proposed tasks
        </button>
        <ul aria-label="Simulated receipts">
          {receipts.map((text, index) => (
            <li key={index} data-testid="simulated-receipt">
              <span className="badge badge-fixture">SIMULATED</span> {text}
            </li>
          ))}
        </ul>
      </section>

      <section aria-label="Related live work">
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
              <ul aria-label="Related tasks">
                {own.map((task) => (
                  <li key={task.contract.id}>
                    <code>{task.contract.id}</code> {task.contract.objective} ({task.state}){" "}
                    <button
                      type="button"
                      onClick={() => router.navigate({ name: "task", projectId, taskId: task.contract.id })}
                    >
                      Open task
                    </button>
                  </li>
                ))}
              </ul>
              <ul aria-label="Pending decisions">
                {pending.map((approval) => (
                  <li key={approval.id}>
                    Approval <code>{approval.id}</code> for task <code>{approval.action.task_id}</code>{" "}
                    <button
                      type="button"
                      onClick={() => router.navigate({ name: "review", projectId, taskId: approval.action.task_id })}
                    >
                      Open review
                    </button>
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
