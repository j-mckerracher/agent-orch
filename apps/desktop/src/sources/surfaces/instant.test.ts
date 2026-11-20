import { describe, expect, it } from "vitest";
import { parseInstant } from "./instant";

const NOON_UTC = Date.UTC(2026, 10, 20, 12, 0, 0);

describe("parseInstant", () => {
  it("reads RFC 3339 with Z, with an offset and with fractional seconds", () => {
    expect(parseInstant("2026-11-20T12:00:00Z")).toBe(NOON_UTC);
    expect(parseInstant("2026-11-20T05:00:00-07:00")).toBe(NOON_UTC);
    expect(parseInstant("2026-11-20T14:30:00+02:30")).toBe(NOON_UTC);
    expect(parseInstant("2026-11-20T12:00:00.250Z")).toBe(NOON_UTC + 250);
  });

  it("reads SQLite's zone-less text as UTC whatever the device zone is", () => {
    expect(parseInstant("2026-11-20 12:00:00")).toBe(NOON_UTC);
  });

  it("refuses zone-less ISO text, date-only text, an impossible date and noise", () => {
    for (const bad of ["2026-11-20T12:00:00", "2026-11-20", "2026-13-45T12:00:00Z", "yesterday", ""]) {
      expect(parseInstant(bad)).toBeNaN();
    }
  });
});
