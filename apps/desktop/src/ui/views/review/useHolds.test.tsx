import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { createServices } from "../../createServices";
import { ServicesProvider } from "../../services";
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
});
