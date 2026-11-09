/** In-memory drafts; they survive navigation and disconnects, never the process. */
import { Store } from "../daemon/store";
import type { TaskKind, DeliveryPolicy, AutonomyPolicy } from "../daemon/types";

export interface SubmitDraft {
  objective: string;
  kind: TaskKind;
  agent: string;
  parentId: string;
  dependencies: string;
  verification: "None" | "Commands";
  commands: string;
  delivery: DeliveryPolicy;
  autonomy: AutonomyPolicy;
  maxRetries: string;
  maxRepairs: string;
}

export const emptySubmitDraft = (): SubmitDraft => ({
  objective: "",
  kind: "Implement",
  agent: "",
  parentId: "",
  dependencies: "",
  verification: "None",
  commands: "",
  delivery: "LocalReview",
  autonomy: "ApprovalRequired",
  maxRetries: "1",
  maxRepairs: "1",
});

export interface DraftsSnapshot {
  registerPath: string;
  submit: Record<string, SubmitDraft>;
  coordinator: Record<string, string>;
}

export class DraftsStore extends Store<DraftsSnapshot> {
  constructor() {
    super({ registerPath: "", submit: {}, coordinator: {} });
  }

  setRegisterPath = (registerPath: string): void => this.update({ registerPath });

  submitDraft(projectId: string): SubmitDraft {
    return this.getSnapshot().submit[projectId] ?? emptySubmitDraft();
  }

  patchSubmit(projectId: string, patch: Partial<SubmitDraft>): void {
    this.update({ submit: { ...this.getSnapshot().submit, [projectId]: { ...this.submitDraft(projectId), ...patch } } });
  }

  clearSubmit(projectId: string): void {
    const { [projectId]: _dropped, ...rest } = this.getSnapshot().submit;
    this.update({ submit: rest });
  }

  setCoordinator(projectId: string, text: string): void {
    this.update({ coordinator: { ...this.getSnapshot().coordinator, [projectId]: text } });
  }
}
