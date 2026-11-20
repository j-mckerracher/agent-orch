import { useEffect, useId, useState } from "react";
import type {
  ModelCatalog,
  ModelCatalogSource,
  ModelChoice,
  ModelOption,
  ModelProvider,
  ModelSelectionCheck,
  PermissionLevel,
  Provenance,
  SurfaceResult,
} from "../../../sources/surfaces/types";
import type { CopilotSignInRecord, CopilotSignInState } from "../../../daemon/types";
import type { SurfaceState } from "../../services";
import { formatAge } from "../../shell/ResourceStrip";
import { Badge, Button, Disclosure } from "../../theme";
import { usePopover } from "../../usePopover";
import { CopilotSignInCheck } from "./CopilotSignInCheck";
import { clearSavedChoice, readSavedChoice, saveChoice } from "./modelChoice";
import { LastTurnNote, PermissionLevelPicker, type LastTurnLevel } from "./PermissionLevelPicker";
import { permissionLabel } from "./permissionLevel";

const LIVE_NOTE =
  "Models and efforts come from the daemon's catalog, and the daemon checks each choice. A changed choice is sent with each message and remembered in this app until you change it. Task submissions do not use it yet.";
const FIXTURE_NOTE = "Placeholder options. A choice stays in this window and is never sent anywhere.";
const DEFAULT_MODEL = "Default model";
const DEFAULT_EFFORT = "Default effort";
const RETRY_MS = 2000;
const MAX_AUTO_RETRIES = 5;
const COLD_REASONS = new Set(["agent_state_unknown", "models_unknown", "effort_unknown"]);
const COPILOT = "github-copilot";
/** Shown once, under the provider list, while Copilot is the chosen provider. A Copilot message is billed to the user's seat. */
const COPILOT_COST_NOTE = "A Copilot message uses premium requests or AI credits on your Copilot seat.";
/** What the picker says for a Copilot sign-in state the record gives no reason for. */
const SIGN_IN_TEXT: Partial<Record<CopilotSignInState, string>> = {
  not_checked: "Not confirmed yet: sign-in is checked on your first Copilot use. If you have not signed in, run `copilot login`.",
  checking: "Checking GitHub Copilot sign-in now. Retry shortly.",
};
/** The Copilot sign-in states the daemon refuses a choice for, with the label the list shows. Not checked and ready can be chosen. */
const SIGN_IN_BLOCKS: Partial<Record<CopilotSignInState, string>> = {
  not_signed_in: "not signed in",
  no_seat: "no Copilot seat",
  checking: "sign-in being checked",
  error: "sign-in not confirmed",
};

type Check = { key: string; result: SurfaceResult<ModelSelectionCheck> | "checking" };

/**
 * Why a provider cannot be chosen, with its list label and the daemon's reason, or null. A provider the daemon
 * cannot launch is blocked with the daemon's own reason. Copilot is also blocked in the sign-in states the daemon
 * refuses a choice for, so the list never offers a choice that always fails. Until the sign-in state is read, Copilot
 * stays choosable: the daemon decides each send.
 */
function blockOf(item: ModelProvider, signIn: CopilotSignInRecord | null, level: PermissionLevel): { label: string; reason: string } | null {
  const failure = item.chatCompatibility?.[level];
  if (failure) {
    return {
      label: failure.retryable ? "compatibility not confirmed" : "not compatible",
      reason: failure.reason,
    };
  }
  if (!item.launchable) return { label: "not launchable", reason: item.detail ?? "The daemon cannot launch this agent." };
  if (item.id !== COPILOT || signIn === null) return null;
  const label = SIGN_IN_BLOCKS[signIn.state];
  return label ? { label, reason: signIn.reason ?? SIGN_IN_TEXT[signIn.state] ?? `GitHub Copilot sign-in is ${label}.` } : null;
}

/** The line under the provider list that says why a provider cannot be chosen, or that Copilot sign-in is not confirmed yet. */
function providerNote(item: ModelProvider, signIn: CopilotSignInRecord | null, level: PermissionLevel): string | null {
  const block = blockOf(item, signIn, level);
  if (block) return `${item.name}: ${block.reason}`;
  if (item.id === COPILOT && signIn?.state === "not_checked") return `${item.name}: ${SIGN_IN_TEXT.not_checked}`;
  return null;
}

/** Per-model efforts win; an agent that reports none per model uses its adapter-wide list. */
export function effortsFor(provider: ModelProvider | undefined, model: ModelOption | undefined): string[] {
  if (model?.efforts && model.efforts.length > 0) return model.efforts;
  return provider?.efforts.status === "ok" ? provider.efforts.value : [];
}

/** `pending` means the catalog has not confirmed the choice yet: it is not refused, only not sent yet. */
type Problem = { pending: boolean; text: string };

/**
 * Why the user's choice cannot be sent, or null when it can. The daemon's own reason is shown, and the choice is never
 * replaced by the default. A part is checked only once the catalog has read it.
 */
function problemOf(
  choice: ModelChoice,
  catalog: SurfaceState<ModelCatalog>,
  provider: ModelProvider | undefined,
  signIn: CopilotSignInRecord | null,
  efforts: string[],
  level: PermissionLevel,
): Problem | null {
  if (catalog.status === "loading") return { pending: true, text: "the model catalog is still being read" };
  if (catalog.status !== "ok") return { pending: false, text: "the model catalog is not available" };
  if (provider === undefined) return { pending: false, text: `the daemon's catalog does not list ${choice.agent}` };
  const block = blockOf(provider, signIn, level);
  if (block) return { pending: provider.id === COPILOT && signIn?.state === "checking", text: block.reason };
  if (choice.model !== undefined) {
    if (provider.models.status === "unknown") return { pending: true, text: "the model list is still being read" };
    if (provider.models.status !== "ok") return { pending: false, text: provider.models.reason };
    if (!provider.models.value.some((item) => item.id === choice.model)) {
      return { pending: false, text: `the daemon's catalog does not list model ${choice.model}` };
    }
  }
  if (choice.effort !== undefined && !efforts.includes(choice.effort)) {
    return { pending: provider.efforts.status === "unknown", text: `effort ${choice.effort} is not offered` };
  }
  return null;
}

/** A catalog reason shown as helper text: one sentence with a capital letter. */
function sentence(text: string): string {
  const trimmed = text.trim();
  if (trimmed === "") return trimmed;
  const first = trimmed.charAt(0).toUpperCase() + trimmed.slice(1);
  return /[.!?]$/.test(first) ? first : `${first}.`;
}

/** The model part of the chip: the chosen model, the default model, or an honest "not confirmed". */
function modelChip(provider: ModelProvider, model: ModelOption | undefined, defaultWord = "default model"): string {
  if (model) return model.label;
  switch (provider.models.status) {
    case "ok":
    case "unsupported":
      return defaultWord;
    case "unavailable":
      return "models unavailable";
    default:
      return "models not confirmed";
  }
}

/**
 * The agent's name as the chip shows it. A trailing qualifier such as "(placeholder)" is dropped, because the chip's
 * placeholder badge already says so. The popover and the accessible name keep the full name.
 */
function chipName(name: string): string {
  return name.replace(/\s*\([^()]*\)$/, "");
}

/** Helper text under the Model select, or null when there is nothing to say. */
function modelHelp(provider: ModelProvider | undefined): string | null {
  if (provider === undefined) return null;
  if (provider.models.status === "ok") return provider.modelsNote ?? null;
  return sentence(provider.models.reason);
}

/** Helper text under the Effort select when it offers no levels. */
function effortHelp(
  provider: ModelProvider | undefined,
  model: ModelOption | undefined,
  efforts: string[],
  live: boolean,
): string | null {
  if (provider === undefined || efforts.length > 0) return null;
  const perModel = provider.models.status === "ok" && provider.models.value.some((item) => (item.efforts?.length ?? 0) > 0);
  if (live && model === undefined && perModel) return "Effort levels are listed per model. Choose a model to see its levels.";
  if (provider.efforts.status !== "ok") return sentence(provider.efforts.reason);
  return null;
}

/**
 * Provider / model / effort menu. Live (daemon provenance): real catalog data with per-model
 * efforts, each choice checked with `agents.validate_selection`, and a cold catalog retried
 * shortly. The choice is saved on each change and restored on mount, so a remount keeps it
 * (DPES-1105). Fixtures: placeholder options that can be tried locally. Fixtures send nothing
 * and never save a choice. A chat also shows its permission level at the end of the chip and offers it in the panel.
 */
export function ModelPicker({
  catalog,
  provenance,
  validate,
  copilotSignIn,
  onRetry,
  onChoiceChange,
  onChoiceBlocked,
  selectionSupported = true,
  permission,
}: {
  catalog: SurfaceState<ModelCatalog>;
  provenance: Provenance;
  validate?: ModelCatalogSource["validate"];
  copilotSignIn?: Pick<ModelCatalogSource, "checkCopilotSignIn" | "copilotSignInStatus">;
  onRetry?: () => void;
  /**
   * Live only: the choice each message is sent with. Null until the user chooses or a saved choice is restored, and
   * null while a choice is unavailable or not confirmed yet (DPES-1105). When the connected daemon cannot take a model
   * choice (`selectionSupported` false), a saved choice is still reported, so a send is blocked and never sent without it.
   */
  onChoiceChange?: (choice: ModelChoice | null) => void;
  onChoiceBlocked?: (reason: string | null) => void;
  /** Live only: false when the connected daemon has no `conversation_selection` capability (DPES-1105). */
  selectionSupported?: boolean;
  /** A chat's permission level: shown in the chip and chosen in the panel. Omitted where no message runs (DPES-1105). */
  permission?: {
    level: PermissionLevel;
    daemonAdvertises: boolean;
    onChange: (level: PermissionLevel) => void;
    /** What the daemon reported for the last turn, shown at the end of the panel. */
    lastTurn?: LastTurnLevel | null;
  };
}) {
  const popover = usePopover();
  const ids = useId();
  const live = provenance.kind === "daemon";
  // The user's choice, or the saved one restored on mount: null until one is made. Fixtures never restore or save one.
  const [pinned, setPinned] = useState<ModelChoice | null>(() => (live ? readSavedChoice() : null));
  const [check, setCheck] = useState<Check | null>(null);
  const [retries, setRetries] = useState(0);
  const [signIn, setSignIn] = useState<CopilotSignInRecord | null>(null);

  const providers = catalog.status === "ok" ? catalog.data.providers : [];
  const level = permission?.level ?? "restricted";
  const providerNotes = live
    ? providers.map((item) => providerNote(item, signIn, level)).filter((note): note is string => note !== null)
    : [];
  const choosable = (item: ModelProvider) => blockOf(item, signIn, level) === null;
  // A provider that cannot be chosen is listed but never chosen. Live, Copilot is never the default: the daemon's
  // default is Codex, then Claude Code. A chosen or restored provider is shown even when it cannot be chosen now.
  const defaultChoosable = (item: ModelProvider) => item.launchable && (!live || item.id !== COPILOT);
  const pinnedProvider = providers.find((item) => item.id === pinned?.agent);
  const provider: ModelProvider | undefined = pinned === null ? providers.find(defaultChoosable) : pinnedProvider;
  const models = provider?.models.status === "ok" ? provider.models.value : [];
  const modelId = pinned?.model ?? null;
  const effort = pinned?.effort ?? null;
  // No model chosen is the default model: no `--model` is given, so the agent's own choice applies.
  const model = models.find((item) => item.id === modelId);
  const efforts = live ? effortsFor(provider, model) : provider?.efforts.status === "ok" ? provider.efforts.value : [];
  let chosenEffort: string | null;
  if (live) {
    const fallback = model?.defaultEffort && efforts.includes(model.defaultEffort) ? model.defaultEffort : null;
    chosenEffort = efforts.includes(effort ?? "") ? effort : fallback;
  } else {
    chosenEffort = efforts.includes(effort ?? "") ? effort : (efforts[1] ?? efforts[0] ?? null);
  }
  // A chosen or restored part the catalog does not offer is never replaced by the default: it is shown as unavailable,
  // with the reason, and the choice is not sent (DPES-1105).
  const problem = live && pinned !== null ? problemOf(pinned, catalog, provider, signIn, efforts, level) : null;
  const missingAgent = live && pinned !== null && catalog.status === "ok" && pinnedProvider === undefined ? pinned.agent : null;
  const unavailableModel = live && modelId !== null && provider !== undefined && model === undefined ? modelId : null;
  const unavailableEffort = live && effort !== null && !efforts.includes(effort) ? effort : null;
  // A saved choice on a daemon that cannot take one. A message is then blocked, never sent without the choice (DPES-1105).
  const savedBlocked = live && pinned !== null && !selectionSupported;

  const effortText = chosenEffort ?? (live ? "default effort" : "effort not available");
  const problemWord = problem === null ? null : problem.pending ? "not confirmed" : "unavailable";
  const chosenName = provider?.name ?? pinned?.agent;
  let modelSummary =
    provider === undefined ? "Model settings unavailable" : `${provider.name} · ${modelChip(provider, model)} · ${effortText}`;
  // The chip's visible text is shorter than the full summary, so it fits beside the conversation target at 960 CSS px.
  // "default model" reads "default", and one "default" stands for a live choice that is default in both parts.
  let chipText = modelSummary;
  if (provider !== undefined) {
    const chipModel = modelChip(provider, model, "default");
    const bothDefault = live && model === undefined && chosenEffort === null && chipModel === "default";
    chipText = `${chipName(provider.name)} · ${bothDefault ? "default" : `${chipModel} · ${effortText}`}`;
  }
  if (problemWord !== null && chosenName !== undefined) {
    modelSummary = `${chosenName} · ${problemWord}`;
    chipText = `${chipName(chosenName)} · ${problemWord}`;
  }
  const levelLabel = permission ? permissionLabel(permission.level) : null;
  const summary = levelLabel ? `${modelSummary} · ${levelLabel}` : modelSummary;

  const lock = provider === undefined && missingAgent === null;
  const providerLocked = lock && !providers.some(choosable);
  // A blocked send says so in the capability note instead, so this note never claims a send goes without the choice.
  const problemNote =
    problem === null || savedBlocked
      ? null
      : `Your choice is ${problemWord}. ${sentence(problem.text)} Change your choice or select the daemon default to send.`;
  const note = live ? LIVE_NOTE : FIXTURE_NOTE;
  const noteId = `${ids}-note`;
  const modelHelpId = `${ids}-model-help`;
  const effortHelpId = `${ids}-effort-help`;
  const modelHelpText = modelHelp(provider);
  const effortHelpText = effortHelp(provider, model, efforts, live);
  const describedBy = (...parts: (string | null)[]) => parts.filter(Boolean).join(" ");
  const triggerBadge = "PLACEHOLDER (D14)";
  const panelBadge = "PLACEHOLDER DATA (D14)";

  // A cold catalog is "unknown": retry a few times, then leave a manual Retry.
  const cold = live && catalog.status === "ok" && providers.some((item) => item.models.status === "unknown");
  useEffect(() => {
    if (!cold) {
      setRetries(0);
      return;
    }
    if (!onRetry || retries >= MAX_AUTO_RETRIES) return;
    const timer = setTimeout(() => {
      setRetries((count) => count + 1);
      onRetry();
    }, RETRY_MS);
    return () => clearTimeout(timer);
  }, [cold, retries, onRetry]);

  // The Copilot sign-in state is read as the panel opens, and at mount for a restored Copilot choice, so a signed-out
  // choice is known before a send (DPES-1105). The read is free and nothing waits for it: until it arrives,
  // Copilot is choosable, and the daemon still decides each send.
  const signInStatus = copilotSignIn?.copilotSignInStatus;
  const readSignIn = popover.open || pinned?.agent === COPILOT;
  useEffect(() => {
    if (!live || !readSignIn || !signInStatus) return;
    let current = true;
    void signInStatus().then((result) => {
      if (current && result.status === "ok") setSignIn(result.data);
    });
    return () => {
      current = false;
    };
  }, [live, readSignIn, signInStatus]);

  let noProviderLabel = "No launchable providers";
  if (providers.length === 0) noProviderLabel = "Not available";
  else if (providers.some((item) => item.launchable)) noProviderLabel = "No provider available for chat";

  // The choice a message carries is what the picker displays, once the user has chosen or restored one. A choice that is
  // unavailable or unconfirmed blocks sending; only an untouched picker sends without a choice.
  const choice: ModelChoice | null =
    live && pinned !== null && provider !== undefined && problem === null
      ? {
          agent: provider.id,
          ...(model ? { model: model.id } : {}),
          ...(chosenEffort ? { effort: chosenEffort } : {}),
        }
      : null;
  // The saved choice is reported as it is saved when the daemon cannot take one, so the send is blocked (DPES-1105).
  const reported: ModelChoice | null = savedBlocked ? pinned : choice;
  const choiceKey = reported === null ? "" : JSON.stringify(reported);
  useEffect(() => {
    onChoiceChange?.(reported);
    // Keyed by value, so a new object for the same choice is not reported again.
  }, [choiceKey]);
  const blockedReason = problem === null ? null : sentence(problem.text);
  useEffect(() => {
    onChoiceBlocked?.(blockedReason);
  }, [onChoiceBlocked, blockedReason]);

  const selectionKey = `${provider?.id}|${model?.id}|${chosenEffort}|${catalog.status}`;
  useEffect(() => {
    if (!live || !validate || !provider || !popover.open || problem !== null || (!model && !chosenEffort)) return;
    let current = true;
    setCheck({ key: selectionKey, result: "checking" });
    validate(provider.id, {
      ...(model ? { model: model.id } : {}),
      ...(chosenEffort ? { effort: chosenEffort } : {}),
    }).then(
      (result) => current && setCheck({ key: selectionKey, result }),
      (error: unknown) => {
        if (!current) return;
        const observedAtMs = Date.now();
        setCheck({
          key: selectionKey,
          result: {
            status: "unavailable",
            reason: error instanceof Error ? error.message : "check failed",
            provenance,
            observedAt: new Date(observedAtMs).toISOString(),
            ageMs: 0,
          },
        });
      },
    );
    return () => {
      current = false;
    };
    // The check follows the selection; `validate` is stable per source.
  }, [live, validate, popover.open, selectionKey]);

  let unavailable: string | null = null;
  if (catalog.status === "loading") unavailable = "Loading the model catalog…";
  else if (catalog.status !== "ok") {
    unavailable = `Model settings are not available: ${catalog.reason} (observed ${formatAge(catalog.observedAt, catalog.ageMs, Date.now())})`;
  }
  else if (providers.length === 0) unavailable = "No providers are reported.";
  else if (!providers.some((item) => item.launchable)) unavailable = "No launchable providers are reported.";

  const verdict = check?.key === selectionKey ? check.result : null;
  let verdictText: string | null = null;
  if (live && verdict !== null) {
    if (verdict === "checking") verdictText = "Checking this choice with the daemon…";
    else if (verdict.status !== "ok") {
      verdictText = `This choice could not be checked: ${verdict.reason} (observed ${formatAge(verdict.observedAt, verdict.ageMs, Date.now())})`;
    }
    else if (verdict.data.valid) verdictText = "The daemon accepts this choice.";
    else if (COLD_REASONS.has(verdict.data.reason)) {
      verdictText = `Not confirmed yet: the catalog is still being read; retry shortly. (${verdict.data.reason})`;
    } else verdictText = `Not supported: ${verdict.data.detail} (${verdict.data.reason})`;
  }

  // The agent the picker shows: the chosen or saved one, else the default.
  const shownAgent = provider?.id ?? pinned?.agent;
  // Pins the whole choice as the user sees it, so a change to one part keeps the other parts, and saves it (live only).
  const pin = (next: ModelChoice) => {
    setPinned(next);
    if (live) saveChoice(next);
  };
  // The user's own way back to the daemon default: it removes the saved choice and sends no selection until the user
  // chooses again. Nothing else clears a saved choice, so a daemon that cannot take one keeps it (DPES-1105).
  const useDaemonDefault = () => {
    setPinned(null);
    if (live) clearSavedChoice();
  };

  return (
    <div
      ref={popover.rootRef}
      className="ws-popover-root ws-model-root"
      onKeyDown={popover.onKeyDown}
      onBlur={popover.onBlur}
    >
      <button
        ref={popover.buttonRef}
        type="button"
        className="ws-chip-button"
        aria-expanded={popover.open}
        aria-controls={`${ids}-menu`}
        aria-label={`Model settings: ${summary}${!live && catalog.status === "ok" ? ` ${triggerBadge}` : ""}`}
        onClick={() => popover.setOpen((value) => !value)}
      >
        <span>{chipText}</span>
        {levelLabel && <span className="ws-chip-level"> · {levelLabel}</span>}
        {!live && catalog.status === "ok" && (
          <>
            {" "}
            <Badge tone="placeholder">{triggerBadge}</Badge>
          </>
        )}
        <span aria-hidden="true">⌄</span>
      </button>
      <div id={`${ids}-menu`} className="ws-popover ws-model-panel" hidden={!popover.open}>
        <p className="ws-menu-title">
          {live ? "Model settings" : "Conversation settings"} {!live && <Badge tone="placeholder">{panelBadge}</Badge>}
        </p>
        {unavailable && <p role="status">{unavailable}</p>}
        {cold && (
          <p role="status" className="ws-small">
            The catalog is still being read; retry shortly.{" "}
            {retries < MAX_AUTO_RETRIES ? "Retrying automatically." : "Automatic retries stopped."}{" "}
            {onRetry && (
              <Button
                variant="quiet"
                onClick={() => {
                  setRetries(0);
                  onRetry();
                }}
              >
                Retry now
              </Button>
            )}
          </p>
        )}
        <label className="ws-field">
          Provider
          <select
            value={provider?.id ?? missingAgent ?? ""}
            disabled={providerLocked}
            aria-describedby={noteId}
            onChange={(event) => pin({ agent: event.target.value })}
          >
            {provider === undefined && missingAgent === null && (
              <option value="">{providerLocked ? noProviderLabel : "Choose a provider"}</option>
            )}
            {missingAgent !== null && (
              <option value={missingAgent} disabled>
                {`${missingAgent} (saved)`}
              </option>
            )}
            {providers.map((item) => {
              const block = blockOf(item, signIn, level);
              return (
                <option key={item.id} value={item.id} disabled={block !== null}>
                  {item.name}
                  {block ? ` (${block.label})` : ""}
                </option>
              );
            })}
          </select>
        </label>
        {providerNotes.map((note) => (
          <p key={note} className="ws-small">
            {note}
          </p>
        ))}
        {onRetry && providers.some((item) => item.chatCompatibility?.[level]?.retryable) && (
          <Button variant="quiet" onClick={onRetry}>Retry compatibility check</Button>
        )}
        {problemNote && (
          <p role="status" className="ws-small">
            {problemNote}
          </p>
        )}
        {savedBlocked && (
          <p role="status" className="ws-small">
            The connected daemon cannot take a model choice, so a message to the Global orchestrator or a workstream is blocked
            while this choice is saved. Use the daemon default in this panel to send, or update the daemon. Coordinator notes
            are not affected.
          </p>
        )}
        {live && provider?.id === COPILOT && <p className="ws-small">{COPILOT_COST_NOTE}</p>}
        <label className="ws-field">
          Model
          <select
            value={model?.id ?? unavailableModel ?? ""}
            disabled={lock || models.length === 0}
            aria-describedby={describedBy(modelHelpText ? modelHelpId : null, noteId)}
            onChange={(event) => {
              if (shownAgent !== undefined) {
                pin({ agent: shownAgent, ...(event.target.value === "" ? {} : { model: event.target.value }) });
              }
            }}
          >
            {provider === undefined ? <option value="">Not available</option> : <option value="">{DEFAULT_MODEL}</option>}
            {models.map((item) => (
              <option key={item.id} value={item.id}>
                {item.label}
              </option>
            ))}
            {unavailableModel !== null && (
              <option value={unavailableModel} disabled>
                {`${unavailableModel} (saved)`}
              </option>
            )}
          </select>
        </label>
        {modelHelpText && (
          <p id={modelHelpId} className="ws-small">
            {modelHelpText}
          </p>
        )}
        <label className="ws-field">
          Effort
          <select
            value={unavailableEffort ?? chosenEffort ?? ""}
            disabled={lock || efforts.length === 0}
            aria-describedby={describedBy(effortHelpText ? effortHelpId : null, noteId)}
            onChange={(event) => {
              if (shownAgent !== undefined) {
                pin({
                  agent: shownAgent,
                  ...(modelId === null ? {} : { model: modelId }),
                  ...(event.target.value === "" ? {} : { effort: event.target.value }),
                });
              }
            }}
          >
            {efforts.length === 0 && <option value="">{live && provider !== undefined ? DEFAULT_EFFORT : "Not available"}</option>}
            {live && efforts.length > 0 && <option value="">{DEFAULT_EFFORT}</option>}
            {efforts.map((item) => (
              <option key={item} value={item}>
                {item}
              </option>
            ))}
            {unavailableEffort !== null && (
              <option value={unavailableEffort} disabled>
                {`${unavailableEffort} (saved)`}
              </option>
            )}
          </select>
        </label>
        {effortHelpText && (
          <p id={effortHelpId} className="ws-small">
            {effortHelpText}
          </p>
        )}
        {pinned !== null && (
          <Button variant="quiet" onClick={useDaemonDefault}>
            Use the daemon default
          </Button>
        )}
        {verdictText && (
          <p role="status" className="ws-small">
            {verdictText}
          </p>
        )}
        {provider?.id === "github-copilot" && copilotSignIn?.checkCopilotSignIn && copilotSignIn.copilotSignInStatus && (
          <CopilotSignInCheck
            check={copilotSignIn.checkCopilotSignIn}
            status={copilotSignIn.copilotSignInStatus}
            known={signIn}
            onRecord={setSignIn}
          />
        )}
        <Disclosure summary="Technical details">
          <p id={noteId} className="ws-small">
            {note}
          </p>
        </Disclosure>
        {permission && (
          <PermissionLevelPicker
            level={permission.level}
            daemonAdvertises={permission.daemonAdvertises}
            agent={choice?.agent}
            onChange={permission.onChange}
            closePanel={popover.close}
          />
        )}
        {permission && <LastTurnNote lastTurn={permission.lastTurn ?? null} />}
      </div>
    </div>
  );
}
