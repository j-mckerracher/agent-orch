import { describe, expect, it } from "vitest";
import type { SupervisionSettings, WakeLimitSettings } from "../../../sources/surfaces/types";
import {
  CROSS_FIELD_ERROR,
  anyUnlimited,
  parseCap,
  previewIntervals,
  previewText,
  supervisionErrors,
  supervisionPatch,
  supervisionText,
  wakeError,
  wakePatch,
} from "./supervisionModel";

const SAVED: SupervisionSettings = { baseIntervalSecs: 600, minIntervalSecs: 60, attemptWallMinutes: 120 };
const LIMITS: WakeLimitSettings = { hourlyCap: 6, dailyCap: 40, dailyTokenBudget: null };

describe("supervision model", () => {
  it("supervision_model_validation_text_for_the_base_minutes_and_the_minimum_seconds", () => {
    const base = (text: string) => supervisionErrors(SAVED, { baseMinutes: text }).baseMinutes;
    for (const bad of ["", "0", "241", "1.5"]) expect(base(bad)).toBe("Enter a whole number from 1 to 240.");
    expect(base("240")).toBeNull();
    const min = (text: string) => supervisionErrors(SAVED, { minSeconds: text }).minSeconds;
    for (const bad of ["9", "", "1.5"]) expect(min(bad)).toBe("Enter a whole number from 10 to 600.");
    expect(min("10")).toBeNull();
  });

  it("supervision_model_refuses_a_minimum_slower_than_the_base_and_a_base_under_the_saved_minimum", () => {
    expect(supervisionErrors(SAVED, { minSeconds: "700" }).minSeconds).toBe(CROSS_FIELD_ERROR);
    expect(supervisionErrors(SAVED, { baseMinutes: "10", minSeconds: "700" }).minSeconds).toBe(CROSS_FIELD_ERROR);
    expect(supervisionErrors(SAVED, { baseMinutes: "1", minSeconds: "30" }).minSeconds).toBeNull();
    expect(supervisionErrors({ ...SAVED, minIntervalSecs: 120 }, { baseMinutes: "1" }).minSeconds).toBe(CROSS_FIELD_ERROR);
  });

  it("supervision_model_preview_pins_the_worked_example_and_the_equal_values_text", () => {
    expect(previewIntervals(600, 60)).toEqual([600, 300, 150, 75, 60]);
    expect(previewText(600, 60)).toBe("With 1 task: every 10 min. 2 tasks: 5 min. 3: 2 min 30 s. 4: 1 min 15 s. 5 or more: 1 min.");
    expect(previewText(600, 600)).toBe("Checks always run every 10 min.");
  });

  it("supervision_model_patch_holds_every_valid_field_from_its_typed_text_or_else_the_saved_value", () => {
    expect(supervisionPatch(SAVED, {})).toEqual({ baseIntervalSecs: 600, minIntervalSecs: 60, attemptWallMinutes: 120 });
    expect(supervisionPatch(SAVED, { baseMinutes: "20", minSeconds: "30", wallMinutes: "60" })).toEqual({
      baseIntervalSecs: 1200,
      minIntervalSecs: 30,
      attemptWallMinutes: 60,
    });
    expect(supervisionText(SAVED)).toEqual({ baseMinutes: "10", minSeconds: "60", wallMinutes: "120" });
  });

  it("supervision_model_wake_patch_sends_null_for_a_cleared_cap_and_the_saved_value_for_an_untouched_one", () => {
    expect(wakePatch(LIMITS, {})).toEqual({ hourlyCap: 6, dailyCap: 40, dailyTokenBudget: null });
    expect(wakePatch(LIMITS, { hourlyCap: "" })).toEqual({ hourlyCap: null, dailyCap: 40, dailyTokenBudget: null });
    expect(wakePatch(LIMITS, { dailyCap: "no limit", dailyTokenBudget: "500000" })).toEqual({ hourlyCap: 6, dailyCap: null, dailyTokenBudget: 500000 });
    expect(wakePatch(LIMITS, { dailyTokenBudget: "off" })).toEqual({ hourlyCap: 6, dailyCap: 40, dailyTokenBudget: null });
    expect(wakePatch(LIMITS, { hourlyCap: "0" })).toEqual({ dailyCap: 40, dailyTokenBudget: null });
  });

  it("supervision_model_wake_fields_accept_numbers_or_words_and_say_the_range_otherwise", () => {
    expect(parseCap("", 1000)).toBeNull();
    expect(parseCap("1001", 1000)).toBeUndefined();
    expect(wakeError("hourlyCap", "1001")).toBe("Enter a whole number from 1 to 1000, or leave it empty for no limit.");
    expect(wakeError("dailyTokenBudget", "0")).toBe("Enter a whole number above 0, or leave it empty for no budget.");
    expect(wakeError("dailyCap", "40")).toBeNull();
  });

  it("supervision_model_says_when_any_reply_limit_is_empty", () => {
    expect(anyUnlimited(LIMITS, {})).toBe(true);
    expect(anyUnlimited(LIMITS, { dailyTokenBudget: "1000" })).toBe(false);
    expect(anyUnlimited(LIMITS, { dailyTokenBudget: "1000", hourlyCap: "" })).toBe(true);
  });
});
