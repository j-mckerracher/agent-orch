import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import App from "../../../App";
import { createFixtureSources } from "../../../sources/fixtures";
import { FakeDaemon } from "../../../test/fakeDaemon";
import { installFakeWorkstreams } from "../../../test/fakeWorkstreams";
import { connected } from "../../../test/render";
import { createServices } from "../../createServices";
import { workstreamsGlobalViews, workstreamsShellSlots } from "./register";

it("drops a pending archive confirm when another workstream is selected", async () => {
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
  render(<App services={services} shellSlots={workstreamsShellSlots} globalViews={workstreamsGlobalViews} />);
  await connected();
  await user.click(await screen.findByRole("button", { name: /Alpha stream/ }));
  await user.click(await screen.findByRole("button", { name: "Archive workstream" }));
  expect(screen.getByRole("button", { name: /Confirm archive \(cannot be undone\)/ })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Beta stream/ }));
  expect(screen.queryByRole("button", { name: /Confirm archive \(cannot be undone\)/ })).not.toBeInTheDocument();
});
