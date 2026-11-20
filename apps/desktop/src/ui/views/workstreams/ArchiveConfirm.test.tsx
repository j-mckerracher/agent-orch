import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import App from "../../../App";
import { createFixtureSources } from "../../../sources/fixtures";
import { FakeDaemon } from "../../../test/fakeDaemon";
import { installFakeWorkstreams } from "../../../test/fakeWorkstreams";
import { connected } from "../../../test/render";
import { backToList, chooseAction, openWorkstream } from "../../../test/workstreamsUi";
import { createServices } from "../../createServices";
import { workstreamsGlobalViews } from "./register";

const CONFIRM = /Confirm archive \(cannot be undone\)/;

async function mountTwoStreams() {
  const fake = new FakeDaemon();
  installFakeWorkstreams(fake, {
    workstreams: [
      { id: "ws-a", title: "Alpha stream", summary: "", state: "active", revision: 1 },
      { id: "ws-b", title: "Beta stream", summary: "", state: "active", revision: 1 },
    ],
    objectives: [],
    links: [],
  });
  const services = createServices({
    connector: fake,
    initialRoute: { name: "global", view: "workstreams" },
    connection: { backoffMs: [5], requestTimeoutMs: 250 },
    data: { coalesceMs: 10 },
    sources: createFixtureSources(),
  });
  const user = userEvent.setup();
  render(<App services={services} globalViews={workstreamsGlobalViews} />);
  await connected();
  await openWorkstream(user, "Alpha stream");
  await chooseAction(user, "Workstream actions", "Archive workstream");
  expect(screen.getByRole("button", { name: CONFIRM })).toBeInTheDocument();
  return { user, services };
}

describe("a pending archive confirmation", () => {
  it("is dropped when another workstream is selected by a link (the chat's Open workstream)", async () => {
    const { services } = await mountTwoStreams();
    await act(async () => services.router.navigate({ name: "global", view: "workstreams", workstreamId: "ws-b" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Beta stream" })).toBeVisible();
    expect(screen.queryByRole("button", { name: CONFIRM })).not.toBeInTheDocument();
  });

  it("is dropped when the operator goes back to the list and opens another workstream", async () => {
    const { user } = await mountTwoStreams();
    await backToList(user);
    expect(screen.queryByRole("button", { name: CONFIRM })).not.toBeInTheDocument();
    await openWorkstream(user, "Beta stream");
    expect(screen.queryByRole("button", { name: CONFIRM })).not.toBeInTheDocument();
  });
});
