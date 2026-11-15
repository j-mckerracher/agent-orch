import assert from "node:assert/strict";
import { test } from "node:test";
import { localPackageNames } from "./prune-target.mjs";

const registry = "registry+https://github.com/rust-lang/crates.io-index";

test("only local packages are pruned, each name once, sorted", () => {
  const metadata = {
    packages: [
      { name: "orchestrator-ipc", source: null },
      { name: "serde", source: registry },
      { name: "agent-orch-desktop", source: null },
      { name: "orchestrator-ipc", source: null },
      { name: "tauri-plugin-x", source: "git+https://example.invalid/x?rev=abc#abc" },
    ],
  };
  assert.deepEqual(localPackageNames(metadata), ["agent-orch-desktop", "orchestrator-ipc"]);
});

test("a graph with no local package prunes nothing", () => {
  assert.deepEqual(localPackageNames({ packages: [{ name: "serde", source: registry }] }), []);
});
