import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Timestamp } from "../../chat/Timestamp";
import { formatAt } from "./time";

const flat = (text: string) => text.replace(/\s/g, "");
const denver = (at: string) => flat(formatAt(at, "America/Denver"));

describe("formatAt", () => {
  it("shows a zone-less UTC string from the daemon as the Denver clock, not the UTC clock", () => {
    // Real values from the bug: the stored text 06:41 is UTC, so Denver reads 11:41 PM the evening before.
    expect(denver("2026-11-20 06:41:00")).toBe("11:41PMMST");
    expect(denver("2026-11-19 10:53:00")).toBe("03:53AMMST");
  });

  it("gives the same instant the same clock in every accepted form", () => {
    for (const at of ["2026-11-20T06:41:00Z", "2026-11-20 06:41:00", "2026-11-19T23:41:00-07:00", "2026-11-20T08:41:00+02:00"]) {
      expect(denver(at)).toBe("11:41PMMST");
    }
    expect(flat(formatAt("2026-11-20T06:41:00Z", "UTC"))).toBe("06:41AMUTC");
  });

  it("names MDT or MST as the date requires across the November change", () => {
    expect(denver("2026-11-01T07:59:00Z")).toBe("01:59AMMDT");
    expect(denver("2026-11-01T08:00:00Z")).toBe("01:00AMMST");
    expect(denver("2026-03-08T08:59:00Z")).toBe("01:59AMMST");
    expect(denver("2026-03-08T09:00:00Z")).toBe("03:00AMMDT");
  });

  it("refuses zone-less ISO text instead of reading it as local time", () => {
    expect(formatAt("2026-11-20T06:41:00", "America/Denver")).toBe("Unrecognized time");
    expect(formatAt("not-a-time", "America/Denver")).toBe("Unrecognized time");
  });
});

describe("Timestamp", () => {
  it("shows a legacy zone-less value as the same device clock as its RFC 3339 form", () => {
    const { container } = render(
      <>
        <Timestamp at="2026-11-20 06:41:00" />
        <Timestamp at="2026-11-20T06:41:00Z" />
      </>,
    );
    const [legacy, modern] = Array.from(container.querySelectorAll("time"));
    expect(legacy!.textContent).toBe(modern!.textContent);
    expect(legacy!.textContent).toBe(formatAt("2026-11-20T06:41:00Z"));
  });
});
