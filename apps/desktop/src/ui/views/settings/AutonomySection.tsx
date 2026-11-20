/**
 * Settings > Autonomy: three switches that save the moment they are clicked (`autonomous_actions`, `chat_pr_delivery`,
 * `mcp.enabled`). A switch the daemon does not send is left out.
 */
import { useEffect, useId, useState } from "react";
import { APPROVER_REQUIRED_NOTICE, asDaemonError } from "../../../daemon/errors";
import type { GroupsPatch, SettingsSnapshot } from "../../../sources/surfaces/types";
import { useGate, useServices } from "../../services";
import { Disclosure } from "../../theme";
import { Notice } from "../../widgets";
import { FIXTURE_SAVE_TEXT, NOT_CONNECTED_REASON, saveFailureText } from "./settingsModel";
import type { SaveResult, SettingsModel } from "./useSettings";

export const AUTONOMOUS_LABEL = "Let the orchestrator act on its own";
export const AUTONOMOUS_CONSEQUENCE =
  "When this is on, the orchestrator starts tasks, retries them, sends messages to waiting tasks, organizes workstreams, records learnings and hands work to project coordinators by itself after your messages, and a coordinator starts the tasks for work handed to it. Cancelling or revising a task, merging, removing a project, holding a decision and saving a preference are never done on its own. When this is off, every action waits for your approval, unless a standing approval covers it.";
export const CHAT_PR_LABEL = "Propose tasks that end in a pull request";
export const CHAT_PR_CONSEQUENCE =
  "When this is off, chat can only propose a report or a review on this machine. When it is on, a task you approve from chat can end in a pull request, in a project set to a pull request mode.";
export const CHAT_PR_SAVED_TEXT = "Saved. New proposals follow this setting; proposals already waiting are checked again when you approve them.";
export const MCP_LABEL = "Read-only task tools in chat";
export const MCP_CONSEQUENCE =
  "When on, the orchestrator chat can look up a task's state and its saved report while it answers. The tools only read; they cannot change anything. Applies from the next message.";
export const MCP_SAVED_TEXT = "Saved. The next message in the orchestrator chat follows this setting.";

type SwitchKey = "autonomousActions" | "chatPrDelivery" | "mcpEnabled";

interface SwitchSpec {
  key: SwitchKey;
  label: string;
  hint: string;
  detail: string;
  savedText: string;
}

const SWITCHES: readonly SwitchSpec[] = [
  { key: "autonomousActions", label: AUTONOMOUS_LABEL, hint: "Starts and retries tasks without asking.", detail: AUTONOMOUS_CONSEQUENCE, savedText: "Saved." },
  { key: "chatPrDelivery", label: CHAT_PR_LABEL, hint: "You still approve each one.", detail: CHAT_PR_CONSEQUENCE, savedText: CHAT_PR_SAVED_TEXT },
  { key: "mcpEnabled", label: MCP_LABEL, hint: "Lets chat look up a task's state and report. Read only.", detail: MCP_CONSEQUENCE, savedText: MCP_SAVED_TEXT },
];

function SwitchRow({ model, snapshot, spec }: { model: SettingsModel; snapshot: SettingsSnapshot; spec: SwitchSpec }) {
  const { surfaces } = useServices();
  const gate = useGate();
  const id = useId();
  const saved = snapshot[spec.key];
  // The value being saved, shown at once; and what the daemon answered, until the page reads a newer snapshot.
  const [pending, setPending] = useState<boolean | null>(null);
  const [fresh, setFresh] = useState<boolean | null>(null);
  const [result, setResult] = useState<SaveResult | null>(null);
  useEffect(() => setFresh(null), [snapshot]);
  if (saved === undefined) return null;

  const checked = pending ?? fresh ?? saved;
  const locked = model.live && !(gate.connected && gate.connection.approver);
  const blocked = model.live && !gate.connected ? NOT_CONNECTED_REASON : locked ? APPROVER_REQUIRED_NOTICE : null;

  const change = async (next: boolean) => {
    if (pending !== null) return;
    setPending(next);
    setResult(null);
    try {
      const receipt = await surfaces.settings.setGroups({ [spec.key]: next } as GroupsPatch);
      if (receipt.simulated) {
        setResult({ kind: "fixture" });
      } else {
        setFresh(receipt.snapshot[spec.key] ?? next);
        setResult({ kind: "saved" });
        model.reload();
      }
    } catch (error) {
      setResult({ kind: "failed", message: saveFailureText(asDaemonError(error)) });
    } finally {
      setPending(null);
    }
  };

  return (
    <div className="settings-switch">
      <div className="settings-field">
        <input
          id={id}
          type="checkbox"
          role="switch"
          checked={checked}
          disabled={locked}
          aria-busy={pending !== null}
          aria-describedby={`${id}-hint`}
          onChange={(event) => void change(event.target.checked)}
        />
        <label htmlFor={id}>{spec.label}</label>
      </div>
      <p id={`${id}-hint`} className="settings-hint">
        {spec.hint}
      </p>
      {blocked !== null && <p className="settings-reason">{blocked}</p>}
      <Disclosure summary="Details">
        <p className="settings-hint">{spec.detail}</p>
      </Disclosure>
      <div role="status" aria-live="polite" aria-label={`${spec.label} result`} className="settings-result">
        {result?.kind === "saved" && <p>{spec.savedText}</p>}
        {result?.kind === "fixture" && <p>{FIXTURE_SAVE_TEXT}</p>}
        {result?.kind === "failed" && <Notice tone="bad">{result.message}</Notice>}
      </div>
    </div>
  );
}

export function AutonomySection({ model }: { model: SettingsModel }) {
  const snapshot = model.load.status === "ready" ? model.load.snapshot : null;
  if (snapshot === null || SWITCHES.every((spec) => snapshot[spec.key] === undefined)) return null;
  return (
    <section aria-label="Autonomy switches" className="settings-section">
      {SWITCHES.map((spec) => (
        <SwitchRow key={spec.key} model={model} snapshot={snapshot} spec={spec} />
      ))}
    </section>
  );
}
