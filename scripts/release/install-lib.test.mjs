import assert from "node:assert/strict";
import { test } from "node:test";
import {
  artifactNamePattern, checkArtifactDir, checkAuthenticode, checkDeveloperIdSignature, parseAuthenticode,
  parseCodesignDetails, parseSums, runMacos, runWindows,
} from "./install-lib.mjs";

const SHA = "a".repeat(64);
const dirInputs = (over = {}) => {
  const file = "Agent-Orchestrator-0.1.0-macos-arm64-unsigned.zip";
  return {
    platform: "macos", version: "0.1.0", requireSigned: false,
    info: { version: "0.1.0", platform: "macos", signed: false, notarized: false, artifacts: [{ file, sha256: SHA }] },
    sumsText: `${SHA}  ${file}\n`, hashes: { [file]: SHA }, ...over,
  };
};

test("a consistent artifact directory has no problems", () => {
  assert.deepEqual(checkArtifactDir(dirInputs()), []);
});

test("artifact directory problems are named", () => {
  const base = dirInputs();
  const file = base.info.artifacts[0].file;
  assert.match(checkArtifactDir(dirInputs({ info: null }))[0], /release-info/);
  assert.match(checkArtifactDir(dirInputs({ hashes: { [file]: "b".repeat(64) } })).join(), /differs from release-info/);
  assert.match(checkArtifactDir(dirInputs({ sumsText: "" })).join(), /SHA256SUMS does not list/);
  assert.match(checkArtifactDir(dirInputs({ sumsText: "garbage\n" })).join(), /malformed/);
  assert.match(checkArtifactDir(dirInputs({ version: "0.2.0" })).join(), /version is 0.1.0, expected 0.2.0/);
  assert.match(checkArtifactDir(dirInputs({ requireSigned: true })).join(), /signed artifact was required/);
  assert.match(checkArtifactDir(dirInputs({ info: { ...base.info, signed: true } })).join(), /disagrees with signed=true/);
  assert.match(checkArtifactDir(dirInputs({ info: { ...base.info, notarized: true } })).join(), /notarized but not signed/);
  assert.match(checkArtifactDir(dirInputs({ info: { ...base.info, artifacts: [] } })).join(), /exactly one artifact/);
});

test("artifact names are platform specific and anchored", () => {
  assert.ok(artifactNamePattern("windows", "0.1.0").test("Agent-Orchestrator-0.1.0-windows-x64-signed-setup.exe"));
  assert.ok(!artifactNamePattern("windows", "0.1.0").test("Agent-Orchestrator-0x1x0-windows-x64-signed-setup.exe"));
  assert.ok(!artifactNamePattern("macos", "0.1.0").test("evil-Agent-Orchestrator-0.1.0-macos-arm64-signed.zip"));
  assert.equal(parseSums(`${SHA}  a b.zip\n`).sums.get("a b.zip"), SHA);
});

const DEVID = [
  "Executable=/x/agent-orch-desktop", "CodeDirectory v=20500 size=1 flags=0x10000(runtime) hashes=1+7 location=embedded",
  "Authority=Developer ID Application: Example (ABCDE12345)", "Authority=Developer ID Certification Authority",
  "TeamIdentifier=ABCDE12345",
].join("\n");

test("codesign details: Developer ID, ad hoc and unsigned", () => {
  const dev = parseCodesignDetails(DEVID);
  assert.deepEqual([dev.adhoc, dev.teamId, dev.hardenedRuntime], [false, "ABCDE12345", true]);
  assert.deepEqual(checkDeveloperIdSignature(dev, { notarized: true }), []);
  const adhoc = parseCodesignDetails("Signature=adhoc\nTeamIdentifier=not set\nCodeDirectory v=20400 flags=0x2(adhoc)");
  assert.deepEqual([adhoc.adhoc, adhoc.teamId], [true, null]);
  assert.equal(checkDeveloperIdSignature(adhoc, { notarized: false }).length, 3);
  const noRuntime = parseCodesignDetails(DEVID.replace("0x10000(runtime)", "0x0(none)"));
  assert.match(checkDeveloperIdSignature(noRuntime, { notarized: true }).join(), /hardened runtime/);
  assert.deepEqual(checkDeveloperIdSignature(noRuntime, { notarized: false }), []);
});

test("Authenticode: only a valid, timestamped signature passes", () => {
  assert.deepEqual(checkAuthenticode(parseAuthenticode("Valid|CN=Example|True\n")), []);
  assert.match(checkAuthenticode(parseAuthenticode("NotSigned||False")).join(), /NotSigned.*no signer.*no countersignature/);
  assert.match(checkAuthenticode(parseAuthenticode("Valid|CN=Example|False")).join(), /timestamp/);
});

// A fake machine: records commands and answers from a table.
const machine = (answers = {}, present = () => true) => {
  const commands = [];
  const run = (command, args, options) => {
    commands.push([command, ...args].join(" "));
    for (const [pattern, reply] of Object.entries(answers)) {
      if (new RegExp(pattern).test(commands.at(-1))) return { code: 0, out: "", err: "", ...reply };
    }
    return { code: 0, out: "", err: "" };
  };
  return { commands, run, exists: present, options: {} };
};
const infoFor = (platform, signed, version = "0.1.0") => ({
  info: { version, platform, signed, notarized: false, artifacts: [{ file: platform === "macos" ? `Agent-Orchestrator-${version}-macos-arm64-${signed ? "signed" : "unsigned"}.zip` : `Agent-Orchestrator-${version}-windows-x64-${signed ? "signed" : "unsigned"}-setup.exe`, sha256: SHA }] },
});
const readDirFor = (platform, signed) => () => {
  const { info } = infoFor(platform, signed);
  return { info: info, sumsText: `${SHA}  ${info.artifacts[0].file}\n`, hashes: { [info.artifacts[0].file]: SHA } };
};
const ctxFor = (platform, fake, over = {}) => ({
  platform, version: "0.1.0", requireSigned: false, artifactDir: "/a", work: "/w", smokeWork: "/w/s",
  run: fake.run, exists: fake.exists, readDir: readDirFor(platform, false), ...over,
});
const quiet = (fn) => { const log = console.log; console.log = () => {}; try { return fn(); } finally { console.log = log; } };

test("macOS unsigned: reported, never claimed signed, upgrade and clean-machine steps UNPROVEN", () => {
  const fake = machine({ PlistBuddy: { out: "0.1.0\n" }, "codesign -dvv": { err: "Signature=adhoc\nTeamIdentifier=not set" }, "spctl": { code: 3, err: "rejected" } });
  const steps = quiet(() => runMacos(ctxFor("macos", fake)));
  assert.equal(steps.failed, false);
  const verdicts = Object.fromEntries(steps.items.map((s) => [s.name, s.verdict]));
  assert.equal(verdicts["new: UNSIGNED artifact; codesign --verify exit 0"], "INFO");
  assert.ok(steps.items.some((s) => s.verdict === "UNPROVEN" && /CLEAN Mac/.test(s.name)));
  assert.ok(steps.items.some((s) => s.verdict === "UNPROVEN" && /upgrade from a previous release/.test(s.name)));
  assert.ok(!steps.items.some((s) => s.verdict === "PASS" && /Developer ID/.test(s.name)));
});

test("macOS: requiring a signature on an unsigned artifact fails", () => {
  const fake = machine({ PlistBuddy: { out: "0.1.0\n" }, "codesign -dvv": { err: "Signature=adhoc" } });
  const steps = quiet(() => runMacos(ctxFor("macos", fake, { requireSigned: true })));
  assert.equal(steps.failed, true);
});

test("macOS signed with an old build: team ids compared, smoke gets both sidecars", () => {
  const fake = machine({
    PlistBuddy: { out: "0.1.0\n" }, "codesign -dvv": { err: DEVID }, "spctl": { code: 0, err: "accepted" },
  });
  const ctx = ctxFor("macos", fake, { requireSigned: true, smokeBin: "/smoke", oldArtifactDir: "/old", readDir: readDirFor("macos", true) });
  const steps = quiet(() => runMacos(ctx));
  assert.equal(steps.failed, false, JSON.stringify(steps.items.filter((s) => s.verdict === "FAIL")));
  const smoke = fake.commands.find((c) => c.startsWith("/smoke "));
  assert.match(smoke, /--old-daemon \/w\/Applications-old\/Agent Orchestrator.app\/Contents\/MacOS\/orchestrator-daemon/);
  assert.match(smoke, /--new-daemon \/w\/Applications-new\/.*--expect-new-version 0\.1\.0/);
  assert.ok(!steps.items.some((s) => /upgrade from a previous release/.test(s.name)));
});

test("macOS: a sidecar signed by another team fails", () => {
  let call = 0;
  const run = (command, args) => {
    const line = [command, ...args].join(" ");
    if (line.startsWith("codesign -dvv") && line.endsWith("orchestrator-daemon")) return { code: 0, out: "", err: DEVID.replaceAll("ABCDE12345", "ZZZZZ99999") };
    if (line.startsWith("codesign -dvv")) return { code: 0, out: "", err: DEVID };
    if (line.includes("PlistBuddy")) return { code: 0, out: "0.1.0", err: "" };
    call++;
    return { code: 0, out: "", err: "accepted" };
  };
  const steps = quiet(() => runMacos(ctxFor("macos", { run, exists: () => true }, { requireSigned: true, readDir: readDirFor("macos", true) })));
  assert.ok(steps.items.some((s) => s.verdict === "FAIL" && /sidecar carries the app's team/.test(s.name)));
  assert.ok(call > 0);
});

test("Windows: installs silently per user, upgrades over the same directory, uninstalls pinned to the directory", () => {
  const commands = [];
  const run = (command, args) => {
    commands.push([command, ...args].join(" "));
    const text = args.join(" ");
    if (text.includes("Get-AuthenticodeSignature")) return { code: 0, out: "Valid|CN=Example|True", err: "" };
    if (text.includes("ProductVersion")) return { code: 0, out: "0.1.0", err: "" };
    return { code: 0, out: "", err: "" };
  };
  // Everything exists until the uninstaller has run; then the app is gone but the data directory stays.
  const exists = (path) => !(path.endsWith("agent-orch-desktop.exe") && commands.some((c) => c.includes("uninstall.exe")));
  const ctx = ctxFor("windows", { run, exists }, {
    requireSigned: true, smokeBin: "C:/smoke.exe", oldArtifactDir: "/old", readDir: readDirFor("windows", true),
  });
  const steps = quiet(() => runWindows(ctx));
  assert.equal(steps.failed, false, JSON.stringify(steps.items.filter((s) => s.verdict === "FAIL")));
  const order = commands.filter((c) => /setup\.exe|smoke|uninstall/.test(c));
  assert.equal(order.length, 4, order.join("\n"));
  assert.match(order[0], /-setup\.exe \/S \/D=.*AgentOrchestrator$/);
  assert.match(order[1], /-setup\.exe \/S \/D=.*AgentOrchestrator$/);
  assert.match(order[2], /smoke\.exe --new-daemon .*--old-daemon .*old-orchestrator-daemon\.exe/);
  assert.match(order[3], /uninstall\.exe \/S _\?=.*AgentOrchestrator$/);
  assert.ok(steps.items.some((s) => s.verdict === "UNPROVEN" && /CLEAN Windows/.test(s.name)));
});

test("Windows: an unsigned installer is INFO unless a signature is required", () => {
  const run = (command, args) => ({ code: 0, out: args.join(" ").includes("Get-AuthenticodeSignature") ? "NotSigned||False" : "0.1.0", err: "" });
  const loose = quiet(() => runWindows(ctxFor("windows", { run, exists: () => false })));
  assert.ok(loose.items.some((s) => s.verdict === "INFO" && /UNSIGNED installer/.test(s.name)));
  const strict = quiet(() => runWindows(ctxFor("windows", { run, exists: () => false }, { requireSigned: true })));
  assert.equal(strict.failed, true);
});
