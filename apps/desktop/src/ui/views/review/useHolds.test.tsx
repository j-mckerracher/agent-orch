import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { DaemonError } from "../../../daemon/errors";
import { createServices } from "../../createServices";
import { ServicesProvider } from "../../services";
import { holdErrorText } from "./holdDates";
import { useHolds } from "./useHolds";

describe("useHolds", () => {
  it("a_hold_or_a_release_refreshes_the_count_at_once", async () => {
    const services = createServices();
    const refresh = vi.spyOn(services.data, "refreshHolds").mockResolvedValue(undefined);
    const wrapper = ({ children }: { children: ReactNode }) => <ServicesProvider services={services}>{children}</ServicesProvider>;
    const { result } = renderHook(() => useHolds(), { wrapper });

    await act(async () => {
      await result.current.hold("approval", "fixture:task-1", "2099-01-01", "Task 1");
    });
    expect(refresh).toHaveBeenCalledTimes(1);

    const held = result.current.items[0]!;
    await act(async () => {
      await result.current.release(held, "Task 1");
    });
    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it("a_refused_hold_is_a_failed_action_that_says_why_and_keeps_the_held_list", async () => {
    const services = createServices();
    const refused = new DaemonError("not-found", "gone");
    vi.spyOn(services.surfaces.holds, "set").mockRejectedValue(refused);
    const wrapper = ({ children }: { children: ReactNode }) => <ServicesProvider services={services}>{children}</ServicesProvider>;
    const { result } = renderHook(() => useHolds(), { wrapper });
    await waitFor(() => expect(result.current.loading).toBe(false));
    const before = result.current.items;

    await act(async () => {
      expect(await result.current.hold("approval", "fixture:task-1", "2099-01-01", "Task 1")).toBe(false);
    });
    expect(result.current.action.state).toEqual({ phase: "done", outcome: { kind: "failed", text: holdErrorText(refused, "hold") } });
    expect(result.current.items).toEqual(before);
    expect(result.current.busy).toBeNull();
  });
});
