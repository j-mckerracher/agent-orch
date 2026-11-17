/** Save path for the supervision and wake-limit groups (DPES-1105 Phase 3). Each section has its own result, so two saves never overwrite each other. */
import { useCallback, useRef, useState } from "react";
import { asDaemonError } from "../../../daemon/errors";
import type { GroupsPatch, SettingsSnapshot } from "../../../sources/surfaces/types";
import { useServices } from "../../services";
import { groupsFailureText } from "./supervisionModel";
import type { SaveResult } from "./useSettings";

export type GroupSection = "supervision" | "wake";

export interface SettingsGroupsModel {
  saving: Record<GroupSection, boolean>;
  results: Record<GroupSection, SaveResult | null>;
  /** The snapshot the daemon answered a save with, until the page reads a newer one. */
  fresh: SettingsSnapshot | null;
  dropFresh: () => void;
  /** Resolves true when the daemon accepted the change, so the caller can drop the typed text. */
  save: (section: GroupSection, patch: GroupsPatch) => Promise<boolean>;
}

export function useSettingsGroups(): SettingsGroupsModel {
  const { surfaces } = useServices();
  const source = surfaces.settings;
  const [saving, setSaving] = useState<Record<GroupSection, boolean>>({ supervision: false, wake: false });
  const [results, setResults] = useState<Record<GroupSection, SaveResult | null>>({ supervision: null, wake: null });
  const [fresh, setFresh] = useState<SettingsSnapshot | null>(null);
  const busy = useRef<Record<GroupSection, boolean>>({ supervision: false, wake: false });

  const save = useCallback(
    async (section: GroupSection, patch: GroupsPatch) => {
      if (busy.current[section]) return false;
      busy.current[section] = true;
      setSaving((current) => ({ ...current, [section]: true }));
      setResults((current) => ({ ...current, [section]: null }));
      const finish = (result: SaveResult) => setResults((current) => ({ ...current, [section]: result }));
      try {
        const receipt = await source.setGroups(patch);
        if (receipt.simulated) {
          finish({ kind: "fixture" });
          return false;
        }
        setFresh(receipt.snapshot);
        finish({ kind: "saved" });
        return true;
      } catch (error) {
        finish({ kind: "failed", message: groupsFailureText(asDaemonError(error)) });
        return false;
      } finally {
        busy.current[section] = false;
        setSaving((current) => ({ ...current, [section]: false }));
      }
    },
    [source],
  );

  const dropFresh = useCallback(() => setFresh(null), []);
  return { saving, results, fresh, dropFresh, save };
}
