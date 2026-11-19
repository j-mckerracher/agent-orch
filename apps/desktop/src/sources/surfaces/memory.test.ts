import { describe, expect, it } from "vitest";
import { createFixtureMemory, FIXTURE_MEMORY_NOTICE, mapMemory } from "./memory";

describe("memory source", () => {
  it("memory_wire_maps_entries_archive_and_limits", () => {
    const snapshot = mapMemory({
      kind: "learning",
      scope: "project:/repo",
      entries: [{ key: "ci", text: "Needs VPN.", written_by: "orchestrator", created_at_ms: 1, confirmed_at_ms: 2, stale: true }],
      bytes: 12,
      budget: 1024,
      archived: [{ id: 7, key: "old", text: "Gone.", written_by: "user", archived_at_ms: 3, reason: "replaced", restored: false }],
      limits: { max_key_bytes: 40, max_text_bytes: 280, stale_after_days: 180 },
    });
    expect(snapshot).toEqual({
      kind: "learning",
      scope: "project:/repo",
      entries: [{ key: "ci", text: "Needs VPN.", writtenBy: "orchestrator", createdAtMs: 1, confirmedAtMs: 2, stale: true }],
      bytes: 12,
      budget: 1024,
      archived: [{ id: 7, key: "old", text: "Gone.", writtenBy: "user", archivedAtMs: 3, reason: "replaced", restored: false }],
      maxKeyBytes: 40,
      maxTextBytes: 280,
      staleAfterDays: 180,
    });
  });

  it("memory_fixture_source_saves_nothing_and_says_so", async () => {
    const source = createFixtureMemory("success", () => 0);
    const before = await source.list("preference", "global");
    expect(await source.set("preference", "global", "only-this", "Only this.")).toMatchObject({ simulated: true, notice: FIXTURE_MEMORY_NOTICE });
    expect(await source.remove("preference", "global", "small-prs")).toMatchObject({ simulated: true });
    expect(await source.restore(1)).toMatchObject({ simulated: true });
    expect(FIXTURE_MEMORY_NOTICE).toBe("Fixture data: nothing was saved.");
    expect(await source.list("preference", "global")).toEqual(before);
  });

  it("memory_fixture_budgets_follow_the_kind_and_empty_has_no_entries", async () => {
    const learning = await createFixtureMemory("success", () => 0).list("learning", "global");
    const empty = await createFixtureMemory("empty", () => 0).list("preference", "global");
    expect(learning).toMatchObject({ status: "ok", data: { budget: 1024, staleAfterDays: 180 } });
    expect(empty).toMatchObject({ status: "ok", data: { budget: 2048, staleAfterDays: null, entries: [], archived: [] } });
  });
});
