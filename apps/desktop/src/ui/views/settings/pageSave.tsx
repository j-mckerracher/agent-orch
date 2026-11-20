/**
 * The one Save bar of a Settings page (DPES-1044). A section that holds typed values calls `usePageSave(reason, run)`;
 * the bar appears once a section has, and Save runs every registered section. Save stays enabled (a disabled-when-unchanged
 * button made saves silently do nothing): a blocked save flags `tried`, so the reason is read as an alert.
 * A page whose controls apply on click registers nothing and shows no bar.
 */
import {
  createContext,
  useContext,
  useLayoutEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type MutableRefObject,
  type ReactNode,
} from "react";
import { Button } from "../../theme";
import { Notice } from "../../widgets";
import { FIXTURE_SAVE_TEXT } from "./settingsModel";
import type { SaveResult } from "./useSettings";

/** What one section tells the person after Save. */
export interface Outcome {
  kind: "saved" | "fixture" | "failed";
  text: string;
}

export const outcomeOf = (result: SaveResult, savedText: string): Outcome =>
  result.kind === "saved"
    ? { kind: "saved", text: savedText }
    : result.kind === "fixture"
      ? { kind: "fixture", text: FIXTURE_SAVE_TEXT }
      : { kind: "failed", text: result.message };

type Run = () => Promise<Outcome>;

interface Registry {
  register: (id: string, run: MutableRefObject<Run>) => () => void;
  setReason: (id: string, reason: string | null) => void;
}

const PageSaveContext = createContext<Registry | null>(null);

/**
 * Registers a section with the page's Save bar. `reason` is why saving is blocked (not connected, no approver, an invalid
 * field), or null. `run` sends the section's values and resolves with what to say; it reads the latest render's values.
 */
export function usePageSave(reason: string | null, run: Run) {
  const registry = useContext(PageSaveContext);
  const id = useId();
  const runRef = useRef(run);
  runRef.current = run;
  useLayoutEffect(() => registry?.register(id, runRef), [registry, id]);
  useLayoutEffect(() => registry?.setReason(id, reason), [registry, id, reason]);
}

/** The frame of one page: its sections, then the Save bar when any section has something to save. */
export function SettingsPage({ children }: { children: ReactNode }) {
  const runs = useRef(new Map<string, MutableRefObject<Run>>());
  const [reasons, setReasons] = useState<Record<string, string | null>>({});
  const [tried, setTried] = useState(false);
  const [saving, setSaving] = useState(false);
  const [outcomes, setOutcomes] = useState<Outcome[]>([]);
  const busy = useRef(false);
  const reasonId = useId();

  const registry = useMemo<Registry>(
    () => ({
      register: (id, run) => {
        runs.current.set(id, run);
        return () => {
          runs.current.delete(id);
          setReasons(({ [id]: _gone, ...rest }) => rest);
        };
      },
      setReason: (id, reason) => setReasons((current) => (current[id] === reason ? current : { ...current, [id]: reason })),
    }),
    [],
  );

  const hasBar = Object.keys(reasons).length > 0;
  const reason = Object.values(reasons).find((item) => item !== null && item !== undefined) ?? null;

  const save = async () => {
    if (busy.current || !hasBar) return;
    setTried(reason !== null);
    if (reason !== null) return;
    busy.current = true;
    setSaving(true);
    setOutcomes([]);
    try {
      // One after another: each answer is a whole snapshot, so two saves in flight could show the older one.
      const done: Outcome[] = [];
      for (const run of [...runs.current.values()]) done.push(await run.current());
      setOutcomes(done);
    } finally {
      busy.current = false;
      setSaving(false);
    }
  };

  // Enter in a text or number field saves, as the sections' own forms did. A field in a dialog is outside the frame.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const target = event.target;
    if (event.key !== "Enter" || !(target instanceof HTMLInputElement) || !event.currentTarget.contains(target)) return;
    if (target.type !== "text" && target.type !== "number") return;
    event.preventDefault();
    void save();
  };

  const texts = new Set<string>();
  const shown = outcomes.filter((outcome) => !texts.has(outcome.text) && texts.add(outcome.text));
  return (
    <PageSaveContext.Provider value={registry}>
      <div onKeyDown={onKeyDown}>
        {children}
        {hasBar && (
          <div className="settings-savebar" data-testid="settings-savebar">
            <Button variant="primary" className="ui-btn-save" aria-busy={saving} aria-describedby={reason === null ? undefined : reasonId} onClick={() => void save()}>
              Save changes
            </Button>
            {reason !== null && (
              <p id={reasonId} role={tried ? "alert" : undefined} className={tried ? "settings-error" : "settings-reason"}>
                {reason}
              </p>
            )}
            <div role="status" aria-live="polite" aria-label="Save result" className="settings-result">
              {shown.map((outcome) =>
                outcome.kind === "failed" ? (
                  <Notice key={outcome.text} tone="bad">
                    {outcome.text}
                  </Notice>
                ) : (
                  <p key={outcome.text}>{outcome.text}</p>
                ),
              )}
            </div>
          </div>
        )}
      </div>
    </PageSaveContext.Provider>
  );
}
