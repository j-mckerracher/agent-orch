import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import { createServices } from "../../createServices";
import { ServicesProvider } from "../../services";
import { LimitsMachineLoadSection } from "./LimitsMachineLoadSection";

afterEach(cleanup);

describe("LimitsMachineLoadSection", () => {
  it("shows the agent slots and System Usage that the activity popover used to show", async () => {
    const services = createServices({ surfaces: createFixtureSurfaces() });
    render(
      <ServicesProvider services={services}>
        <LimitsMachineLoadSection />
      </ServicesProvider>,
    );
    const section = screen.getByRole("region", { name: "Current machine load" });
    expect(await within(section).findByTestId("limits-slots")).toHaveTextContent("20 of 24 agent slots in use · 4 available");
    const usage = within(section).getByTestId("resource-strip");
    expect(usage).toHaveAttribute("open");
    expect(await within(usage).findByRole("meter", { name: "CPU utilization" })).toBeInTheDocument();
  });
});
