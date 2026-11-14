import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Badge, Bubble, Button, Disclosure, EmptyState, Meter, Panel, Pill } from "./primitives";

describe("Disclosure", () => {
  it("is a native details element whose summary is in the tab order and toggles", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <Disclosure summary="Running" count={3}>
        <p>Queue body</p>
      </Disclosure>,
    );
    const details = container.querySelector("details")!;
    expect(details).toHaveClass("ui-disclosure");
    expect(details.open).toBe(false);
    expect(screen.getByText("3")).toBeInTheDocument();

    // jsdom does not synthesise Enter/Space activation of <summary>; browsers do natively.
    await user.tab();
    expect(container.querySelector("summary")).toHaveFocus();
    await user.click(screen.getByText("Running"));
    expect(details.open).toBe(true);
    await user.click(screen.getByText("Running"));
    expect(details.open).toBe(false);
  });

  it("can start open", () => {
    const { container } = render(
      <Disclosure summary="Attention" defaultOpen>
        body
      </Disclosure>,
    );
    expect(container.querySelector("details")!.open).toBe(true);
  });
});

describe("Pill and Badge", () => {
  it("carries a text glyph so state is not colour-only", () => {
    render(
      <>
        <Pill tone="ok">Connected</Pill>
        <Pill tone="warn">Stale</Pill>
        <Pill tone="bad">Down</Pill>
        <Pill tone="info">Note</Pill>
        <Pill tone="neutral">Idle</Pill>
      </>,
    );
    expect(screen.getByText("Connected")).toHaveTextContent("✓Connected");
    expect(screen.getByText("Stale")).toHaveTextContent("!Stale");
    expect(screen.getByText("Down")).toHaveTextContent("✕Down");
    expect(screen.getByText("Down")).toHaveClass("ui-pill", "ui-tone-bad");
  });

  it("can omit the glyph when the text already names the state", () => {
    render(<Pill hideMark>Plain</Pill>);
    expect(screen.getByText("Plain").querySelector(".ui-pill-mark")).toBeNull();
  });

  it("renders placeholder badges with the dashed tone", () => {
    render(<Badge tone="placeholder">PLACEHOLDER</Badge>);
    expect(screen.getByText("PLACEHOLDER")).toHaveClass("ui-badge", "ui-tone-placeholder");
  });
});

describe("Button", () => {
  it("defaults to type=button, is focusable and activates with Enter and Space", async () => {
    const user = userEvent.setup();
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Pause</Button>);
    const button = screen.getByRole("button", { name: "Pause" });
    expect(button).toHaveAttribute("type", "button");
    expect(button).toHaveClass("ui-btn", "ui-btn-secondary");
    await user.tab();
    expect(button).toHaveFocus();
    await user.keyboard("{Enter}");
    await user.keyboard(" ");
    expect(onClick).toHaveBeenCalledTimes(2);
  });

  it("is skipped by tab and inert when disabled", async () => {
    const user = userEvent.setup();
    const onClick = vi.fn();
    render(
      <Button variant="primary" disabled onClick={onClick}>
        Approve
      </Button>,
    );
    await user.tab();
    expect(screen.getByRole("button", { name: "Approve" })).not.toHaveFocus();
    await user.click(screen.getByRole("button", { name: "Approve" }));
    expect(onClick).not.toHaveBeenCalled();
  });
});

describe("Panel, Bubble and EmptyState", () => {
  it("Panel renders a titled section at the requested heading level", () => {
    render(
      <Panel title="Queue" headingLevel={2} actions={<span>4</span>}>
        content
      </Panel>,
    );
    expect(screen.getByRole("heading", { level: 2, name: "Queue" })).toBeInTheDocument();
    expect(screen.getByText("content")).toBeInTheDocument();
  });

  it("Bubble labels its speaker for both roles", () => {
    render(
      <>
        <Bubble from="coordinator" author="Global orchestrator" time="10:02">
          Plan ready.
        </Bubble>
        <Bubble from="human" author="You">
          Go.
        </Bubble>
      </>,
    );
    expect(screen.getByText("Global orchestrator")).toBeInTheDocument();
    expect(screen.getByText("Plan ready.").closest("article")).toHaveClass("ui-bubble-coordinator");
    expect(screen.getByText("Go.").closest("article")).toHaveClass("ui-bubble-human");
  });

  it("EmptyState shows title, description and an action", () => {
    render(<EmptyState title="No tasks" description="Nothing queued." action={<Button>Create</Button>} />);
    expect(screen.getByText("No tasks")).toBeInTheDocument();
    expect(screen.getByText("Nothing queued.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create" })).toBeInTheDocument();
  });
});

describe("Meter", () => {
  it("exposes role=meter with value, range and text", () => {
    render(<Meter label="CPU" value={42} valueText="42%" />);
    const meter = screen.getByRole("meter", { name: "CPU" });
    expect(meter).toHaveAttribute("aria-valuenow", "42");
    expect(meter).toHaveAttribute("aria-valuemin", "0");
    expect(meter).toHaveAttribute("aria-valuemax", "100");
    expect(meter).toHaveAttribute("aria-valuetext", "42%");
    expect(meter).toHaveTextContent("42%");
    expect(meter.querySelector<HTMLElement>(".ui-meter-track")!.style.getPropertyValue("--level")).toBe("42%");
  });

  it("clamps out-of-range values", () => {
    render(<Meter label="Disk" value={140} />);
    const meter = screen.getByRole("meter", { name: "Disk" });
    expect(meter).toHaveAttribute("aria-valuenow", "100");
    expect(meter.querySelector<HTMLElement>(".ui-meter-track")!.style.getPropertyValue("--level")).toBe("100%");
  });

  it("shows unavailable, never zero, when there is no value", () => {
    render(
      <>
        <Meter label="GPU" value={null} />
        <Meter label="Net" value={Number.NaN} />
      </>,
    );
    expect(screen.queryByRole("meter")).toBeNull();
    expect(screen.getAllByText("unavailable")).toHaveLength(2);
    expect(screen.queryByText("0")).toBeNull();
  });
});
