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

/**
 * Orchestrator Workspace conversation state (M7-10). Keys are opaque target keys chosen by the
 * view (`global`, `project:<id>`); this store only keeps their text. A draft is form text and is
 * never sent anywhere by being stored.
 */
export interface WorkspaceDrafts {
  /** The selected conversation target key, or null before the user has chosen one. */
  target: string | null;
  drafts: Record<string, string>;
}

export const emptyWorkspaceDrafts = (): WorkspaceDrafts => ({ target: null, drafts: {} });

export interface DraftsSnapshot {
  registerPath: string;
  submit: Record<string, SubmitDraft>;
  coordinator: Record<string, string>;
  /** Absent until the workspace view stores something; snapshots persisted before M7-10 lack it. */
  workspace?: WorkspaceDrafts;
}

/** Bump when the persisted shape changes; an unknown version is discarded. */
export const DRAFTS_STORAGE_KEY = "agent-orch.drafts.v1";

export type DraftStorage = Pick<Storage, "getItem" | "setItem">;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

function loadWorkspace(value: unknown): WorkspaceDrafts | null {
  if (!isRecord(value) || !isRecord(value.drafts)) return null;
  const drafts: Record<string, string> = {};
  for (const [key, text] of Object.entries(value.drafts)) {
    if (typeof text === "string") drafts[key] = text;
  }
  return { target: typeof value.target === "string" ? value.target : null, drafts };
}

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
    const snapshot: DraftsSnapshot = { registerPath: parsed.registerPath, submit, coordinator };
    const workspace = loadWorkspace(parsed.workspace);
    if (workspace) snapshot.workspace = workspace;
    return snapshot;
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

  workspace(): WorkspaceDrafts {
    return this.getSnapshot().workspace ?? emptyWorkspaceDrafts();
  }

  workspaceDraft(targetKey: string): string {
    return this.workspace().drafts[targetKey] ?? "";
  }

  setWorkspaceTarget(targetKey: string): void {
    this.update({ workspace: { ...this.workspace(), target: targetKey } });
  }

  /** An empty draft is removed so cleared conversations do not accumulate keys. */
  setWorkspaceDraft(targetKey: string, text: string): void {
    const { [targetKey]: _dropped, ...rest } = this.workspace().drafts;
    this.update({ workspace: { ...this.workspace(), drafts: text === "" ? rest : { ...rest, [targetKey]: text } } });
  }
}
