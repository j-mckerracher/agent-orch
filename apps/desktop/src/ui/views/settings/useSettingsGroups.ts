/** Save path for the supervision and wake-limit groups (DPES-1105 Phase 3). The page's Save bar calls it; the busy guard lives there. */
import { useCallback, useState } from "react";
import { asDaemonError } from "../../../daemon/errors";
import type { GroupsPatch, SettingsSnapshot } from "../../../sources/surfaces/types";
import { useServices } from "../../services";
import { groupsFailureText } from "./supervisionModel";
import type { SaveResult } from "./useSettings";

export interface SettingsGroupsModel {
  /** The snapshot the daemon answered a save with, until the page reads a newer one. */
  fresh: SettingsSnapshot | null;
  dropFresh: () => void;
  /** Resolves with what happened; `saved` means the daemon accepted the change, so the caller can drop the typed text. */
  save: (patch: GroupsPatch) => Promise<SaveResult>;
}

export function useSettingsGroups(): SettingsGroupsModel {
  const { surfaces } = useServices();
  const source = surfaces.settings;
  const [fresh, setFresh] = useState<SettingsSnapshot | null>(null);

  const save = useCallback(
    async (patch: GroupsPatch): Promise<SaveResult> => {
      try {
        const receipt = await source.setGroups(patch);
        if (receipt.simulated) return { kind: "fixture" };
        setFresh(receipt.snapshot);
        return { kind: "saved" };
      } catch (error) {
        return { kind: "failed", message: groupsFailureText(asDaemonError(error)) };
      }
    },
    [source],
  );

  const dropFresh = useCallback(() => setFresh(null), []);
  return { fresh, dropFresh, save };
}
