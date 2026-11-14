import { describe, expect, it } from "vitest";
import { usageHeading } from "./usageWindow";

const DENVER = "America/Denver";

describe("usageHeading", () => {
  it("says Today's only when the window starts on today's local day", () => {
    // 01:00 MST on 2026-11-14 in Denver; the window opened at that day's local midnight.
    expect(usageHeading("2026-11-14T00:00:00-07:00", DENVER, Date.parse("2026-11-14T08:00:00Z"))).toBe("Today’s AI usage");
  });

  it("uses the reporting timezone for the day, not the host clock", () => {
    // 23:30 MST on 2026-11-13 in Denver, which is already 2026-11-14 in UTC. The window starts tomorrow in Denver.
    expect(usageHeading("2026-11-14T00:00:00-07:00", DENVER, Date.parse("2026-11-14T06:30:00Z"))).toBe("AI usage");
  });

  it("does not claim Today's for a window from an earlier day", () => {
    expect(usageHeading("2026-01-01T00:00:00-07:00", DENVER, Date.parse("2026-11-14T08:00:00Z"))).toBe("AI usage");
  });

  it("does not claim Today's when the window start is unreadable or the zone is unknown", () => {
    expect(usageHeading("not a date", DENVER, Date.parse("2026-11-14T08:00:00Z"))).toBe("AI usage");
    expect(usageHeading("2026-11-14T00:00:00-07:00", "Not/AZone", Date.parse("2026-11-14T08:00:00Z"))).toBe("AI usage");
  });
});
