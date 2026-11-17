import { describe, expect, it } from "vitest";
import { createFixtureNotes, FIXTURE_NOTES_NOTICE, mapNotes } from "./notes";

describe("notes source", () => {
  it("notes_wire_maps_to_bodies_in_position_order_with_the_limits", () => {
    const snapshot = mapNotes({
      scope: "global",
      notes: [
        { position: 1, body: "second" },
        { position: 0, body: "first" },
      ],
      limits: { max_notes: 10, max_bytes: 200 },
    });
    expect(snapshot).toEqual({ scope: "global", notes: ["first", "second"], maxNotes: 10, maxBytes: 200 });
  });

  it("notes_fixture_source_saves_nothing_and_says_so", async () => {
    const source = createFixtureNotes("success", () => 0);
    const before = await source.list("global");
    const receipt = await source.set("global", ["only this"]);
    expect(receipt).toMatchObject({ simulated: true, notice: FIXTURE_NOTES_NOTICE });
    expect(FIXTURE_NOTES_NOTICE).toBe("Fixture data: nothing was saved.");
    const after = await source.list("global");
    expect(after).toEqual(before);
  });
});
