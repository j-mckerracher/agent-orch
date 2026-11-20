import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { insideTauri } from "../native/tauriConnector";
import { Button } from "./theme";
import "./DesktopUpdates.css";

const CHECK_INTERVAL_MS = 30 * 60 * 1000;
const SEMVER = /^\d+\.\d+\.\d+$/;

type CheckResult =
  | { kind: "disabled" }
  | { kind: "current"; version: string }
  | { kind: "available"; version: string };

const ERROR_KINDS = [
  "gh_missing",
  "github_unavailable",
  "invalid_release",
  "offer_changed",
  "download_failed",
  "verification_failed",
  "daemon_busy",
  "daemon_unavailable",
  "install_failed",
  "busy",
] as const;
type UpdateErrorKind = (typeof ERROR_KINDS)[number] | "unknown";

const ERROR_TEXT: Record<UpdateErrorKind, string> = {
  gh_missing: "GitHub CLI was not found. Install GitHub CLI (gh), run gh auth login --hostname github.com, then try again.",
  github_unavailable:
    "Can't check GitHub Releases. Verify your connection and run gh auth login --hostname github.com if needed.",
  invalid_release: "The newest release is not a valid update, so nothing was installed.",
  offer_changed: "The available update changed. Check for updates to see the current one.",
  download_failed: "The update could not be downloaded. Try again.",
  verification_failed: "The update did not pass verification, so nothing was installed.",
  daemon_busy: "Work is still running. Finish it, then try the update again.",
  daemon_unavailable: "The background service could not confirm it is safe to update. Try again in a moment.",
  install_failed: "The update could not be installed. Try again.",
  busy: "Another update check or install is already running. Try again in a moment.",
  unknown: "The update failed. Try again.",
};

/** One value for everything the row can show; the offered or running version travels with its state. */
type UpdateState =
  | { kind: "idle" }
  | { kind: "disabled" }
  | { kind: "current"; version: string; announce: boolean }
  | { kind: "available"; version: string; error?: UpdateErrorKind }
  | { kind: "installing"; version: string }
  | { kind: "failed"; error: UpdateErrorKind };

function isNativeHost(): boolean {
  return typeof window !== "undefined" && insideTauri(window);
}

function parseCheck(value: unknown): CheckResult {
  if (typeof value === "object" && value !== null && "kind" in value) {
    if (value.kind === "disabled") return { kind: "disabled" };
    if (
      (value.kind === "current" || value.kind === "available") &&
      "version" in value &&
      typeof value.version === "string" &&
      SEMVER.test(value.version)
    ) {
      return { kind: value.kind, version: value.version };
    }
  }
  throw new Error("Native updater returned an invalid result.");
}

function parseErrorKind(value: unknown): UpdateErrorKind {
  const kind = typeof value === "object" && value !== null && "kind" in value ? value.kind : undefined;
  return ERROR_KINDS.find((known) => known === kind) ?? "unknown";
}

export function DesktopUpdates() {
  const native = isNativeHost();
  const [state, setState] = useState<UpdateState>({ kind: "idle" });
  // The one version the user postponed; a newer offer or a manual check shows it again.
  const [dismissed, setDismissed] = useState<string | null>(null);
  const mounted = useRef(false);
  // Checks and installs share one native lock, so overlapping calls are skipped here too.
  const inFlight = useRef(false);
  // A manual check asked for while a check runs waits for it, so the click is not dropped.
  const queued = useRef(false);
  const [checking, setChecking] = useState(false);

  const check = useCallback(async (manual: boolean): Promise<void> => {
    if (manual) setChecking(true);
    if (inFlight.current) {
      if (manual) queued.current = true;
      return;
    }
    inFlight.current = true;
    try {
      const result = parseCheck(await invoke<unknown>("desktop_update_check"));
      if (!mounted.current) return;
      if (result.kind === "available" && manual) setDismissed(null);
      setState(
        result.kind === "current"
          ? { kind: "current", version: result.version, announce: manual }
          : result,
      );
    } catch (error) {
      // Background failures stay quiet (a machine without gh would otherwise nag forever); the user can ask again.
      if (manual && mounted.current) setState({ kind: "failed", error: parseErrorKind(error) });
    } finally {
      inFlight.current = false;
      if (queued.current) {
        queued.current = false;
        void check(true);
      } else if (manual) {
        setChecking(false);
      }
    }
  }, []);

  useEffect(() => {
    if (!native) return;
    mounted.current = true;
    void check(false);
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void check(false);
    }, CHECK_INTERVAL_MS);
    return () => {
      mounted.current = false;
      window.clearInterval(timer);
    };
  }, [native, check]);

  const install = async (version: string) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setState({ kind: "installing", version });
    try {
      await invoke("desktop_update_install", { version });
    } catch (error) {
      if (mounted.current) setState({ kind: "available", version, error: parseErrorKind(error) });
    } finally {
      inFlight.current = false;
    }
  };

  if (!native || state.kind === "idle" || state.kind === "disabled") return null;

  const installing = state.kind === "installing";
  const offer = state.kind === "available" && state.version !== dismissed ? state : null;
  const shown = offer ?? (state.kind === "installing" ? state : null);
  const error = state.kind === "failed" ? state.error : offer?.error;
  const message = shown
    ? installing
      ? "Preparing update…"
      : `MCAO ${shown.version} is available.`
    : state.kind === "current" && state.announce
      ? `MCAO ${state.version} is up to date.`
      : null;

  return (
    <div className="strip update-strip" data-testid="desktop-updates">
      <p role="status">{checking ? "Checking for updates…" : message}</p>
      {shown && (
        <>
          <Button variant="primary" pending={installing} onClick={() => void install(shown.version)}>
            Install and restart
          </Button>
          <Button
            disabled={installing}
            onClick={() => {
              setDismissed(shown.version);
              setState({ kind: "available", version: shown.version });
            }}
          >
            Later
          </Button>
        </>
      )}
      <Button pending={checking} disabled={installing} onClick={() => void check(true)}>
        Check for updates
      </Button>
      {error && <p role="alert">{ERROR_TEXT[error]}</p>}
    </div>
  );
}
