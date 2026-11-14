import { describe, expect, it } from "vitest";
import { calendarDay, zoneDayStart } from "./localDay";

describe("calendarDay", () => {
  it("reads the day in the named zone, not in UTC", () => {
    // 2026-11-14 06:30Z is 23:30 on 2026-11-13 in Denver (MST, UTC-7).
    expect(calendarDay(new Date("2026-11-14T06:30:00Z"), "America/Denver")).toBe("2026-11-13");
    expect(calendarDay(new Date("2026-11-14T06:30:00Z"), "UTC")).toBe("2026-11-14");
  });

  it("returns null for a zone the runtime does not know", () => {
    expect(calendarDay(new Date("2026-11-14T06:30:00Z"), "Not/AZone")).toBeNull();
  });
});

describe("zoneDayStart", () => {
  it("is local midnight of the current day in the zone", () => {
    // 2026-11-14 08:00Z is 01:00 on 2026-11-14 in Denver, so the day began at 2026-11-14T07:00Z.
    expect(zoneDayStart(Date.parse("2026-11-14T08:00:00Z"), "America/Denver")?.toISOString()).toBe("2026-11-14T07:00:00.000Z");
  });

  it("uses the zone's day when the host's UTC day is already ahead", () => {
    // 06:30Z on 11-14 is still 11-13 in Denver, so the window starts at 2026-11-13T07:00Z.
    expect(zoneDayStart(Date.parse("2026-11-14T06:30:00Z"), "America/Denver")?.toISOString()).toBe("2026-11-13T07:00:00.000Z");
  });

  it("handles a 25-hour fall-back day (America/Denver, 2026-11-01)", () => {
    // Midnight on 2026-11-01 is MDT (UTC-6), so 06:00Z.
    expect(zoneDayStart(Date.parse("2026-11-01T20:00:00Z"), "America/Denver")?.toISOString()).toBe("2026-11-01T06:00:00.000Z");
  });

  it("returns null for an unknown zone", () => {
    expect(zoneDayStart(Date.parse("2026-11-14T08:00:00Z"), "Not/AZone")).toBeNull();
  });
});

describe("usage fixture window", () => {
  it("starts at local midnight of the clock's day, so the placeholder never claims a stale window", async () => {
    const { createFixtureSurfaces } = await import("./fixtures");
    for (const iso of ["2026-11-14T08:00:00Z", "2027-03-08T12:00:00Z"]) {
      const result = await createFixtureSurfaces({ now: () => Date.parse(iso) }).usage.usage();
      expect(result.status).toBe("ok");
      if (result.status !== "ok") return;
      expect(result.data.windowStart).toBe(zoneDayStart(Date.parse(iso), "America/Denver")?.toISOString());
    }
  });
});
