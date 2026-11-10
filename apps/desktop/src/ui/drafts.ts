/** Form drafts; they survive navigation and disconnects, and the window lifecycle when storage is given. */
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

/** Bump when the persisted shape changes; an unknown version is discarded. */
export const DRAFTS_STORAGE_KEY = "agent-orch.drafts.v1";

export type DraftStorage = Pick<Storage, "getItem" | "setItem">;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

function loadDrafts(storage: DraftStorage): DraftsSnapshot | null {
  try {
    const raw = storage.getItem(DRAFTS_STORAGE_KEY);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (!isRecord(parsed) || typeof parsed.registerPath !== "string") return null;
    if (!isRecord(parsed.submit) || !isRecord(parsed.coordinator)) return null;
    const submit: Record<string, SubmitDraft> = {};
    for (const [project, draft] of Object.entries(parsed.submit)) {
      if (isRecord(draft)) submit[project] = { ...emptySubmitDraft(), ...(draft as Partial<SubmitDraft>) };
    }
    const coordinator: Record<string, string> = {};
    for (const [project, text] of Object.entries(parsed.coordinator)) {
      if (typeof text === "string") coordinator[project] = text;
    }
    return { registerPath: parsed.registerPath, submit, coordinator };
  } catch {
    return null;
  }
}

/**
 * Drafts survive navigation and disconnects. With a `storage`, they also
 * survive closing and reopening the window; they hold form text only.
 */
export class DraftsStore extends Store<DraftsSnapshot> {
  constructor(storage?: DraftStorage) {
    super((storage && loadDrafts(storage)) || { registerPath: "", submit: {}, coordinator: {} });
    if (storage) {
      this.subscribe(() => {
        try {
          storage.setItem(DRAFTS_STORAGE_KEY, JSON.stringify(this.getSnapshot()));
        } catch {
          // Persistence is best effort; the in-memory draft is still intact.
        }
      });
    }
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
