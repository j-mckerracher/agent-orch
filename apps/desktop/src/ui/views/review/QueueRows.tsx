// The rows of the Review queue. A row says what the item is (the type chip), what it is about, and offers the one
// thing to do next: a proposed action or a question is decided in the row; a candidate opens its own page.
import { useId, useMemo, useState, type ReactNode } from "react";
import type { ActionProposal } from "../../../sources/surfaces/types";
import { basename } from "../../format";
import { Button, Disclosure, Pill } from "../../theme";
import { Notice } from "../../widgets";
import type { ActionDecisions } from "../workspace/useActionDecisions";
import {
  actionPhrase,
  approvedByText,
  cardTitle,
  fieldsOf,
  newerRecord,
  overrideLines,
  requestErrorText,
  viewOf,
  type WorkstreamNames,
} from "../workspace/actionCardModel";
import { checkLine, checksSummary, mergeableText, mergeStateText, plainRemote, prStateText, prStateTone, shortSha, agoText } from "./pullsModel";
import { HOLD_REASON_ID, HoldControl } from "./HoldControl";
import { MAX_HOLD_DAYS, heldUntilText, holdRange } from "./holdDates";
import { KIND_LABEL, STATE_LABEL, TYPE_LABEL, approvalKind, type QueueItem } from "./queue";
import type { HoldsModel } from "./useHolds";
import { MAX_ANSWER_CHARS, type QuestionsModel } from "./useQuestions";
import type { PullsModel } from "./usePulls";

/** What every row reads from the view: the models that carry its actions, and where a row goes next. */
export interface RowEnv {
  fixture: boolean;
  names: WorkstreamNames;
  /** Proposed actions: whether the daemon takes a decision now, and who may take it. */
  canDecide: boolean;
  connected: boolean;
  approver: boolean;
  decisions: ActionDecisions;
  questions: QuestionsModel;
  holds: HoldsModel;
  pulls: PullsModel;
  /** Names the row a change started from, so focus can come back to it. */
  start: (rowKey: string) => void;
  open: (taskId: string) => void;
  /** A candidate's title and project, from the task list; null when the task is not known. */
  taskTitle: (taskId: string) => string | null;
}

const projectName = (projectId: string | null) => (projectId === null ? null : basename(projectId));

function Row({ item, env, title, sub, children, actions }: { item: QueueItem; env: RowEnv; title: string; sub: string; children?: ReactNode; actions?: ReactNode }) {
  return (
    <li className="rq-row" data-row-key={item.key} data-type={item.type} tabIndex={-1}>
      <div className="rq-line">
        <Pill tone="neutral" hideMark title={env.fixture ? "Fixture data" : undefined}>
          {TYPE_LABEL[item.type]}
        </Pill>
        <div className="rq-main">
          <span className="rq-title">{title}</span>
          <span className="rq-sub">{sub}</span>
        </div>
        {item.heldUntil !== null && <Pill tone="warn">{heldUntilText(item.heldUntil)}</Pill>}
        {actions && <div className="rq-actions">{actions}</div>}
      </div>
      {children}
    </li>
  );
}

function Release({ item, env, label }: { item: QueueItem; env: RowEnv; label: string }) {
  const { holds } = env;
  if (item.hold === null) return null;
  const hold = item.hold;
  return (
    <Button
      aria-label={`Release: ${label}`}
      aria-describedby={holds.blocked !== null ? HOLD_REASON_ID : undefined}
      disabled={holds.blocked !== null || holds.busy !== null}
      onClick={() => {
        env.start(item.key);
        void holds.release(hold, label);
      }}
    >
      Release
    </Button>
  );
}

function FieldList({ proposal, names }: { proposal: ActionProposal; names: WorkstreamNames }) {
  return (
    <dl className="rq-fields">
      {fieldsOf(proposal, names, null).map((field) => (
        <div key={field.label}>
          <dt>{field.label}</dt>
          <dd>
            {typeof field.value === "string" ? (
              field.value
            ) : (
              <ul>
                {field.value.map((line, index) => (
                  <li key={index}>{line}</li>
                ))}
              </ul>
            )}
          </dd>
        </div>
      ))}
    </dl>
  );
}

const whereOf = (scope: Extract<QueueItem, { source: "proposal" }>["proposal"]["scope"]): string => {
  if (scope.kind === "global") return "Global orchestrator";
  return scope.kind === "project" ? `${basename(scope.projectId)} coordinator` : `Workstream ${scope.workstreamId}`;
};

function ProposalRow({ item, env }: { item: Extract<QueueItem, { source: "proposal" }>; env: RowEnv }) {
  const messageId = useId();
  const { decisions } = env;
  const listed = item.proposal.proposal;
  const proposal = newerRecord(listed, decisions.confirmed.get(listed.id));
  const deciding = decisions.inFlight.has(proposal.id);
  const view = viewOf(proposal, null, {
    canDecide: env.canDecide,
    connected: env.connected,
    approver: env.approver,
    deciding,
    names: env.names,
  });
  const heading = cardTitle(proposal.kind, proposal.params).replace(/^Proposed: /, "");
  const phrase = actionPhrase(proposal, env.names);
  const subject = phrase.slice(heading.length).trim();
  const notice = decisions.notices.get(proposal.id) ?? null;
  const error = decisions.errors.get(proposal.id) ?? null;
  const approvedBy = approvedByText(proposal);
  const pending = view.state === "pending";
  return (
    <Row
      item={item}
      env={env}
      title={subject === "" ? heading : `${heading}: ${subject}`}
      sub={whereOf(item.proposal.scope)}
      actions={
        pending ? (
          <>
            <Button
              variant={view.approve.variant === "danger" ? "danger" : "primary"}
              aria-label={`Approve: ${phrase}`}
              aria-describedby={view.approve.enabled ? undefined : messageId}
              disabled={!view.approve.enabled}
              onClick={() => void decisions.decide(proposal, "approve", null, env.names)}
            >
              Approve
            </Button>
            <Button
              variant="quiet"
              aria-label={`Reject: ${phrase}`}
              aria-describedby={view.reject.enabled ? undefined : messageId}
              disabled={!view.reject.enabled}
              onClick={() => void decisions.decide(proposal, "reject", null, env.names)}
            >
              Reject
            </Button>
            {item.hold !== null && <Release item={item} env={env} label={phrase} />}
          </>
        ) : (
          <Pill tone={view.pill.tone}>{view.pill.text}</Pill>
        )
      }
    >
      {(!pending || !view.approve.enabled) && (
        <Notice id={messageId} tone={view.message.tone}>
          {view.message.text}
        </Notice>
      )}
      {approvedBy !== null && <p className="rq-note">{approvedBy}</p>}
      {overrideLines(proposal).map((line) => (
        <p key={line} className="rq-note">
          {line}
        </p>
      ))}
      {notice !== null && <Notice tone="info">{notice}</Notice>}
      {error !== null && <Notice tone="bad">{requestErrorText(error, null)}</Notice>}
      <Disclosure summary="Details">
        <FieldList proposal={proposal} names={env.names} />
      </Disclosure>
    </Row>
  );
}

function QuestionRow({ item, env }: { item: Extract<QueueItem, { type: "question" }>; env: RowEnv }) {
  const { question } = item;
  const { questions } = env;
  const [text, setText] = useState("");
  const [day, setDay] = useState("");
  const answerId = useId();
  const deferId = useId();
  const reasonId = useId();
  const range = useMemo(() => holdRange(new Date()), []);
  const off = questions.blocked !== null || questions.busy !== null;
  const reason = questions.blocked !== null ? reasonId : undefined;
  const label = question.taskTitle || question.taskId;
  return (
    <Row item={item} env={env} title={question.question} sub={[projectName(item.projectId), `Task: ${label}`].filter(Boolean).join(" · ")}>
      {questions.blocked !== null && (
        <p id={reasonId} className="rq-note">
          {questions.blocked}
        </p>
      )}
      {question.options.length > 0 && (
        <div className="rq-options" role="group" aria-label={`Options for ${label}`}>
          {question.options.map((option) => (
            <Button key={option} variant="quiet" onClick={() => setText(option)}>
              {option}
            </Button>
          ))}
        </div>
      )}
      <form
        className="rq-answer"
        onSubmit={(event) => {
          event.preventDefault();
          void questions.answer(question, text).then((sent) => {
            if (sent) setText("");
          });
        }}
      >
        <textarea
          id={answerId}
          aria-label={`Your answer for ${label}`}
          placeholder="Your answer"
          rows={2}
          maxLength={MAX_ANSWER_CHARS}
          value={text}
          onChange={(event) => setText(event.target.value)}
        />
        <Button type="submit" disabled={off || text.trim() === ""} aria-describedby={reason}>
          Send answer
        </Button>
      </form>
      {question.deferredUntil === null && (
        <Disclosure summary="Defer">
          <form
            className="rq-answer"
            onSubmit={(event) => {
              event.preventDefault();
              void questions.defer(question, day);
            }}
          >
            <label htmlFor={deferId}>Defer until</label>
            <input id={deferId} type="date" required min={range.min} max={range.max} value={day} onChange={(event) => setDay(event.target.value)} />
            <Button type="submit" disabled={off || day === ""} aria-describedby={reason}>
              Defer
            </Button>
            <span className="rd-reason">The task stays blocked until you answer. Up to {MAX_HOLD_DAYS} days.</span>
          </form>
        </Disclosure>
      )}
    </Row>
  );
}

function CandidateRow({ item, env }: { item: Extract<QueueItem, { type: "candidate" }>; env: RowEnv }) {
  const { approval } = item;
  const taskId = approval.action.task_id;
  const pending = approval.state.state === "requested";
  const title = `Review changes: ${env.taskTitle(taskId) ?? `task ${taskId}`}`;
  const sub = [
    projectName(item.projectId),
    `commit ${approval.action.candidate_commit.slice(0, 7)}`,
    pending ? null : STATE_LABEL[approval.state.state],
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <Row
      item={item}
      env={env}
      title={title}
      sub={sub}
      actions={
        <>
          <Button aria-label={`Review: ${title}`} onClick={() => env.open(taskId)}>
            {pending ? "Review" : "Open"}
          </Button>
          {pending && item.hold === null && env.holds.supported && (
            <HoldControl taskId={taskId} holds={env.holds} onStart={() => env.start(item.key)} />
          )}
          <Release item={item} env={env} label={`task ${taskId}`} />
        </>
      }
    />
  );
}

function OtherApprovalRow({ item, env }: { item: Extract<QueueItem, { source: "daemon" }>; env: RowEnv }) {
  const { approval } = item;
  const taskId = approval.action.task_id;
  return (
    <Row
      item={item}
      env={env}
      title={`${KIND_LABEL[approvalKind(approval)] ?? approvalKind(approval)} for task ${taskId}`}
      sub={[projectName(item.projectId), "Cannot be decided in this app"].filter(Boolean).join(" · ")}
      actions={
        <>
          {item.hold === null && env.holds.supported && <HoldControl taskId={taskId} holds={env.holds} onStart={() => env.start(item.key)} />}
          <Release item={item} env={env} label={`task ${taskId}`} />
        </>
      }
    />
  );
}

function PrRow({ item, env }: { item: Extract<QueueItem, { type: "pr" }>; env: RowEnv }) {
  const { pull } = item;
  const { pulls } = env;
  const open = pull.state === "open";
  const merge = mergeStateText(pull.merge);
  const reasonId = useId();
  const cooling = pulls.cooling.has(pull.taskId);
  return (
    <Row
      item={item}
      env={env}
      title={`Pull request ${pull.number}: ${plainRemote(pull.title)}`}
      sub={[projectName(item.projectId), `${plainRemote(pull.branch, 80)} into ${plainRemote(pull.baseBranch, 80)}`, prStateText(pull.state), checksSummary(pull)]
        .filter(Boolean)
        .join(" · ")}
      actions={
        <>
          <Pill tone={prStateTone(pull.state)}>{prStateText(pull.state)}</Pill>
          {open && (
            <Button
              variant="quiet"
              aria-label={`Refresh pull request ${pull.number}`}
              aria-describedby={cooling ? reasonId : undefined}
              disabled={cooling || pulls.busy !== null}
              onClick={() => void pulls.refresh(pull)}
            >
              Refresh
            </Button>
          )}
        </>
      }
    >
      {cooling && (
        <span id={reasonId} className="rd-reason">
          Read a moment ago. Refresh is available again in about 10 seconds.
        </span>
      )}
      <Disclosure summary="Details">
        <p className="rq-note">
          Commit <code aria-label={`Commit ${shortSha(pull.headSha)}`}>{shortSha(pull.headSha)}</code> · {mergeableText(pull.mergeable)} · {agoText(pull.observedAgeSecs)}
        </p>
        {merge !== null && <p className="rq-note">{merge}</p>}
        {pull.checks.length > 0 && (
          <ul aria-label={`Checks for pull request ${pull.number}`}>
            {pull.checks.map((check, index) => (
              <li key={index}>{checkLine(check)}</li>
            ))}
          </ul>
        )}
      </Disclosure>
    </Row>
  );
}

export function QueueRow({ item, env }: { item: QueueItem; env: RowEnv }) {
  switch (item.type) {
    case "approval":
      return item.source === "proposal" ? <ProposalRow item={item} env={env} /> : <OtherApprovalRow item={item} env={env} />;
    case "question":
      return <QuestionRow item={item} env={env} />;
    case "candidate":
      return <CandidateRow item={item} env={env} />;
    case "pr":
      return <PrRow item={item} env={env} />;
  }
}
