import { afterEach, describe, expect, it } from "vitest";
import { FakeDaemon } from "../../test/fakeDaemon";
import { connect, populate } from "./testkit";

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
});

async function live(fake: FakeDaemon) {
  const connected = await connect(fake);
  stops.push(connected.stop);
  return connected.surfaces;
}

describe("GitHub Copilot sign-in check, live (DPES-1105)", () => {
  it("makes one daemon call per request and returns the daemon's record as ok data", async () => {
    const fake = populate(new FakeDaemon());
    fake.stub("agents.check_copilot_sign_in", {
      state: "ready",
      reason: null,
      checked_at_ms: 1_700_000_000_000,
      premium_requests: 0.33,
      ai_credits: null,
      warning: null,
    });
    const models = (await live(fake)).models;
    expect(models.checkCopilotSignIn).toBeDefined();
    const result = await models.checkCopilotSignIn!(false);
    expect(result.status).toBe("ok");
    if (result.status !== "ok") return;
    expect(result.data).toMatchObject({ state: "ready", premium_requests: 0.33, ai_credits: null, warning: null });
    const calls = fake.callsTo("agents.check_copilot_sign_in");
    expect(calls).toHaveLength(1);
    expect(calls[0]!.params).toEqual({ force: false });
  });

  it("passes force through only when asked to check again", async () => {
    const fake = populate(new FakeDaemon());
    fake.stub("agents.check_copilot_sign_in", {
      state: "not_signed_in",
      reason: "Not signed in to GitHub Copilot. Run `copilot login`, then retry.",
      checked_at_ms: null,
      premium_requests: null,
      ai_credits: null,
      warning: null,
    });
    const models = (await live(fake)).models;
    await models.checkCopilotSignIn!(true);
    expect(fake.callsTo("agents.check_copilot_sign_in")[0]!.params).toEqual({ force: true });
  });
});
