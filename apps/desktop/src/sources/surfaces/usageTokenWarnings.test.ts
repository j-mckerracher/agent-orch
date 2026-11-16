import { describe, expect, it } from "vitest";
import { parseTokenWarnings } from "./usageTokenWarnings";

const rule = { provider: "codex", metric: "output_tokens", limit: "20", observed: "21", exceeded: true };
const payload = (rules: unknown[] = [rule]) => ({ schema_version: 1, state: "ok", rules });

describe("token warning boundary", () => {
  it("accepts an older daemon without warnings and preserves exact large counters", () => {
    expect(parseTokenWarnings(undefined)).toBeUndefined();
    const large = { ...rule, limit: "18446744073709551614", observed: "18446744073709551615" };
    expect(parseTokenWarnings(payload([large]))?.rules).toEqual([large]);
  });

  it.each([
    { ...rule, observed: null, exceeded: false },
    { ...rule, observed: "20", exceeded: true },
    { ...rule, observed: "18446744073709551616" },
    { ...rule, limit: "0" },
    { ...rule, metric: "reasoning_tokens" },
    { ...rule, metric: { toString: null } },
  ])("discloses invalid or contradictory warning data as unavailable (case %#)", (invalid) => {
    expect(parseTokenWarnings(payload([invalid]))).toEqual({ schema_version: 1, state: "unavailable", rules: [] });
  });

  it("keeps unavailable counts unknown and refuses duplicate or excessive rules", () => {
    const missing = { ...rule, observed: null, exceeded: null };
    expect(parseTokenWarnings(payload([missing]))?.rules).toEqual([missing]);
    expect(parseTokenWarnings(payload([rule, rule]))?.state).toBe("unavailable");
    expect(parseTokenWarnings(payload(Array.from({ length: 33 }, () => rule)))?.state).toBe("unavailable");
  });
});
