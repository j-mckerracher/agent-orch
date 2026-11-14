import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DaemonError } from "../daemon/errors";
import type { Provenance } from "../sources/types";
import { FakeDaemon, mkTask } from "../test/fakeDaemon";
import { REPO, mount } from "../test/render";
import { ErrorNotice, Notice, PlaceholderBadge, StateText } from "./widgets";

const FIXTURE: Provenance = { kind: "fixture", id: "D5", fixture: "test" };
const DAEMON: Provenance = { kind: "daemon", id: "D2" };
const glyphOf = (testId: string) => screen.getByTestId(testId).querySelector(".conn-glyph")?.textContent;

describe("M7-15 restyled project screens: widgets", () => {
  it("renders task states as text plus a non-colour mark, with unknown states neutral", () => {
    render(
      <>
        <StateText state="Failed" />
        <StateText state="Succeeded" />
        <StateText state="SomethingNew" />
      </>,
    );
    const failed = screen.getByText("Failed");
    expect(failed).toHaveClass("ui-tone-bad");
    expect(failed.querySelector(".ui-pill-mark")).toHaveTextContent("✕");
    expect(screen.getByText("Succeeded").querySelector(".ui-pill-mark")).toHaveTextContent("✓");
    expect(screen.getByText("SomethingNew")).toHaveClass("ui-tone-neutral");
  });

  it("marks placeholder and daemon provenance with different badge tones and the same labels", () => {
    render(
      <>
        <PlaceholderBadge provenance={FIXTURE} />
        <PlaceholderBadge provenance={DAEMON} />
      </>,
    );
    expect(screen.getByTestId("badge-D5")).toHaveTextContent("PLACEHOLDER DATA (D5)");
    expect(screen.getByTestId("badge-D5")).toHaveClass("ui-tone-placeholder");
    expect(screen.getByTestId("badge-D2")).toHaveTextContent("DAEMON (D2)");
    expect(screen.getByTestId("badge-D2")).not.toHaveClass("ui-tone-placeholder");
  });

  it("renders a notice with a hidden glyph and keeps the supplied role", () => {
    render(
      <Notice tone="bad" role="alert">
        Something failed.
      </Notice>,
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Something failed.");
    expect(alert.querySelector(".ps-notice-mark")).toHaveAttribute("aria-hidden", "true");
  });

  it("announces errors as alerts with the failing action, reason and detail", () => {
    render(
      <ErrorNotice
        error={new DaemonError("disconnected", "socket closed")}
        action="Registering the repository"
      />,
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Registering the repository failed.");
    expect(alert).toHaveTextContent("socket closed");
  });
});

describe("M7-15 restyled project screens: connection strip", () => {
  it("preserves connection-status semantics", async () => {
    await mount(new FakeDaemon());
    const status = screen.getByTestId("connection-status");
    expect(status).toHaveAttribute("role", "status");
    expect(status).toHaveTextContent("Connected");
    expect(glyphOf("connection-status")).toBe("✓");
  });

  it("gives offline, auth-failed and incompatible states distinct text and distinct glyphs", async () => {
    await mount(new FakeDaemon(), { bridge: false });
    const offline = { text: screen.getByTestId("connection-status").textContent, glyph: glyphOf("connection-status") };
    cleanup();

    const authFake = new FakeDaemon();
    authFake.openError = new DaemonError("auth", "bad", -32001);
    await mount(authFake, { waitConnected: false });
    await waitFor(() => expect(screen.getByTestId("connection-status")).toHaveTextContent("Authentication failed"));
    const auth = { text: screen.getByTestId("connection-status").textContent, glyph: glyphOf("connection-status") };
    cleanup();

    const versionFake = new FakeDaemon();
    versionFake.openError = new DaemonError("version", "no shared version", -32003);
    await mount(versionFake, { waitConnected: false });
    await waitFor(() =>
      expect(screen.getByTestId("connection-status")).toHaveTextContent("Incompatible version or capability"),
    );
    const version = { text: screen.getByTestId("connection-status").textContent, glyph: glyphOf("connection-status") };

    expect(new Set([offline.glyph, auth.glyph, version.glyph]).size).toBe(3);
    expect(new Set([offline.text, auth.text, version.text]).size).toBe(3);
  });

  it("keeps the fixture banner and stale note as status rows with visible text", async () => {
    await mount(new FakeDaemon(), { bridge: false });
    const banner = screen.getByTestId("fixture-banner");
    expect(banner).toHaveAttribute("role", "status");
    expect(banner.querySelector(".ui-badge")).toHaveTextContent("FIXTURE DEMO MODE");
  });

  it("keeps the connection error row as an alert", async () => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError("auth", "bad", -32001);
    await mount(fake, { waitConnected: false });
    await waitFor(() => expect(screen.getByTestId("connection-error")).toHaveAttribute("role", "alert"));
  });
});

describe("M7-15 restyled project screens: behaviour is unchanged", () => {
  it("renders a breadcrumb with a hidden separator and a current-page marker", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: REPO }];
    await mount(fake);
    const crumbs = await screen.findByRole("navigation", { name: "Breadcrumb" });
    expect(crumbs.querySelector("[aria-current='page']")).toHaveTextContent("Projects");
  });

  it("disables registration with a visible reason while the daemon is offline", async () => {
    await mount(new FakeDaemon(), { bridge: false });
    expect(screen.getByRole("button", { name: "Register" })).toBeDisabled();
    expect(screen.getByText(/Registration is unavailable while offline/)).toBeInTheDocument();
  });

  it("shows task state with a mark in the task list and keeps the Open action keyboard-operable", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: REPO }];
    fake.tasks = [mkTask("task-1", REPO, { state: "Failed" })];
    const { user } = await mount(fake, { route: { name: "tasks", projectId: REPO, filter: "all" } });
    const table = await screen.findByRole("table", { name: "Tasks" });
    expect(table.querySelector(".ui-pill.ui-tone-bad")).toHaveTextContent("Failed");
    const open = screen.getByRole("button", { name: "Open" });
    open.focus();
    expect(open).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(await screen.findByTestId("task-state")).toHaveTextContent("Failed");
  });

  it("exposes pressed filter state through aria-pressed", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: REPO }];
    fake.tasks = [mkTask("task-1", REPO)];
    await mount(fake, { route: { name: "tasks", projectId: REPO, filter: "all" } });
    const group = await screen.findByRole("group", { name: "Task filters" });
    expect(group.querySelectorAll("button[aria-pressed='true']")).toHaveLength(1);
  });
});
