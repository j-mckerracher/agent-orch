/** Data for the Settings page: one shape for the daemon-backed (live) source and the fixture source (DPES-1105). */
import { useCallback, useEffect, useRef, useState } from "react";
import { asDaemonError } from "../../../daemon/errors";
import { notOk } from "../../../sources/surfaces/result";
import type { LoadLimits, SettingsSnapshot, SurfaceResult } from "../../../sources/surfaces/types";
import { useGate, useServices } from "../../services";
import { gateAnnouncement, saveFailureText } from "./settingsModel";

/** The daemon has no push event and samples every 2 seconds, so the page reads again on this interval. */
export const REFRESH_MS = 5_000;

export type SettingsLoad =
  | { status: "loading" }
  /** An older daemon without the `settings` capability: nothing to read, nothing wrong. */
  | { status: "unsupported" }
  | { status: "failed"; reason: string }
  | { status: "ready"; snapshot: SettingsSnapshot; refreshFailed: boolean };

export type SaveResult = { kind: "saved" } | { kind: "fixture" } | { kind: "failed"; message: string };

export interface SettingsModel {
  /** True when the source is daemon-backed; fixture values never appear then. */
  live: boolean;
  load: SettingsLoad;
  saving: boolean;
  result: SaveResult | null;
  /** The sentence for the polite live region; empty until the gate changes state after the first read. */
  gateAnnouncement: string;
  reload: () => void;
  /** Resolves true when the daemon accepted the change, so the caller can drop the typed text. */
  save: (patch: Partial<LoadLimits>) => Promise<boolean>;
}

function settle(previous: SettingsLoad, next: SurfaceResult<SettingsSnapshot>): SettingsLoad {
  if (next.status === "ok") return { status: "ready", snapshot: next.data, refreshFailed: false };
  if (next.status === "unsupported") return { status: "unsupported" };
  // A failed refresh keeps what is on screen and says so, rather than blanking the page.
  return previous.status === "ready" ? { ...previous, refreshFailed: true } : { status: "failed", reason: next.reason };
}

export function useSettings(): SettingsModel {
  const { surfaces } = useServices();
  const { connection } = useGate();
  const source = surfaces.settings;
  const live = source.provenance.kind === "daemon";
  const [load, setLoad] = useState<SettingsLoad>({ status: "loading" });
  const [saving, setSaving] = useState(false);
  const [result, setResult] = useState<SaveResult | null>(null);
  const [announcement, setAnnouncement] = useState("");
  // Only the newest answer is applied: a slow read never overwrites a later read or a save reply.
  const latest = useRef(0);
  const busy = useRef(false);
  const seenClosed = useRef<boolean | null>(null);

  const read = useCallback(() => {
    const seq = ++latest.current;
    const apply = (next: SurfaceResult<SettingsSnapshot>) => {
      if (seq === latest.current) setLoad((previous) => settle(previous, next));
    };
    source
      .get()
      .then(apply, (error: unknown) =>
        apply(notOk(source.provenance, "unavailable", error instanceof Error ? error.message : "settings read failed")),
      );
  }, [source]);

  // Read now, again after every reconnect, and every few seconds while the page is visible. Nothing runs once it unmounts.
  useEffect(() => {
    read();
    const visible = () => document.visibilityState !== "hidden";
    const timer = setInterval(() => visible() && read(), REFRESH_MS);
    const onVisible = () => visible() && read();
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      latest.current += 1;
      clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [read, connection.generation, connection.status]);

  const closed = load.status === "ready" ? load.snapshot.gate.state === "closed" : null;
  useEffect(() => {
    if (closed === null) return;
    if (seenClosed.current !== null && seenClosed.current !== closed) setAnnouncement(gateAnnouncement(closed));
    seenClosed.current = closed;
  }, [closed]);

  const save = useCallback(
    async (patch: Partial<LoadLimits>) => {
      if (busy.current) return false;
      busy.current = true;
      setSaving(true);
      setResult(null);
      try {
        const receipt = await source.set(patch);
        if (receipt.simulated) {
          setResult({ kind: "fixture" });
          return false;
        }
        latest.current += 1;
        setLoad({ status: "ready", snapshot: receipt.snapshot, refreshFailed: false });
        setResult({ kind: "saved" });
        return true;
      } catch (error) {
        setResult({ kind: "failed", message: saveFailureText(asDaemonError(error)) });
        return false;
      } finally {
        busy.current = false;
        setSaving(false);
      }
    },
    [source],
  );

  return { live, load, saving, result, gateAnnouncement: announcement, reload: read, save };
}
