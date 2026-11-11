import assert from "node:assert/strict";
import { test } from "node:test";
import {
  artifactNamePattern, checkArtifactDir, checkAuthenticode, checkDeveloperIdSignature, classifyUpgrade, parseAuthenticode,
  parseCodesignDetails, parseSums, runMacos, runWindows, validateSmokeResult,
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
// The directory "/old" holds the previous release, at `oldVersion`.
const readDirFor = (platform, signed, oldVersion = "0.1.0") => (dir) => {
  const { info } = infoFor(platform, signed, dir === "/old" ? oldVersion : "0.1.0");
  return { info: info, sumsText: `${SHA}  ${info.artifacts[0].file}\n`, hashes: { [info.artifacts[0].file]: SHA } };
};
// A user data directory held in memory; `files` is what a uninstaller could delete.
const fakeUserData = (root = "C:/Users/u/AppData/Local/agent-orch") => {
  const files = new Map();
  return { root, files, seed: (name, text) => files.set(name, text), read: (name) => files.get(name) ?? null, remove: (name) => files.delete(name) };
};
const ctxFor = (platform, fake, over = {}) => ({
  platform, version: "0.1.0", requireSigned: false, artifactDir: "/a", work: "/w", smokeWork: "/w/s", smokeOut: "/w/result.json",
  run: fake.run, exists: fake.exists, readDir: readDirFor(platform, false), readText: () => null,
  fileHash: (path) => `hash:${path}`, userData: fakeUserData(), nonce: "n1", ...over,
});

// What a correct release_smoke reports for the arguments it was given.
const smokeResult = (args, over = {}) => {
  const get = (flag) => args[args.indexOf(flag) + 1];
  const upgrade = args.includes("--old-daemon");
  const versions = [upgrade ? get("--expect-old-version") : get("--expect-new-version"), get("--expect-new-version")];
  return {
    schema: 1, result: "PASS", steps: Array.from({ length: 12 }, (_, i) => ({ n: i + 1, text: "step" })), upgrade_requested: upgrade,
    same_version: versions[0] === versions[1], daemon_versions: versions, old_daemon: upgrade ? get("--old-daemon") : get("--new-daemon"), new_daemon: get("--new-daemon"), ...over,
  };
};
// Wraps a fake machine's run: a call to the smoke binary exits `code` and writes `write(result)` to its --out file
// (null: writes nothing, like a stub). Returns the wrapped context pieces.
const withSmoke = (base, { code = 0, write = (result) => result } = {}) => {
  const written = new Map();
  const run = (command, args, options) => {
    if (command !== "/smoke" && command !== "C:/smoke.exe") return base(command, args, options);
    base(command, args, options);
    const result = write(smokeResult(args));
    if (result !== null) written.set(args[args.indexOf("--out") + 1], typeof result === "string" ? result : JSON.stringify(result));
    return { code, out: "", err: "" };
  };
  return { run, readText: (path) => written.get(path) ?? null };
};
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

const macRun = ({ oldVersion = "0.1.0" } = {}) => {
  const commands = [];
  const run = (command, args) => {
    const line = [command, ...args].join(" ");
    commands.push(line);
    if (line.startsWith("codesign -dvv")) return { code: 0, out: "", err: DEVID };
    if (line.includes("PlistBuddy")) return { code: 0, out: line.includes("Applications-old") ? oldVersion : "0.1.0", err: "" };
    return { code: 0, out: "", err: "accepted" };
  };
  return { commands, run, exists: () => true };
};
const macCtx = (machine, over = {}) => ctxFor("macos", machine, {
  requireSigned: true, smokeBin: "/smoke", oldArtifactDir: "/old", readDir: readDirFor("macos", true, "0.0.9"), ...withSmoke(machine.run), ...over,
});

test("macOS signed with a genuinely older build: team ids compared, smoke gets both sidecars, upgrade is proven", () => {
  const machine = macRun({ oldVersion: "0.0.9" });
  const steps = quiet(() => runMacos(macCtx(machine)));
  assert.deepEqual(failures(steps), []);
  const smoke = machine.commands.find((c) => c.startsWith("/smoke "));
  assert.match(smoke, /--old-daemon \/w\/Applications-old\/Agent Orchestrator.app\/Contents\/MacOS\/orchestrator-daemon --expect-old-version 0\.0\.9/);
  assert.match(smoke, /--new-daemon \/w\/Applications-new\/.*--expect-new-version 0\.1\.0 --out \/w\/result\.json/);
  assert.equal(steps.upgrade, "upgrade");
  assert.ok(steps.items.some((s) => s.verdict === "PASS" && /\(upgrade: old sidecar then new sidecar/.test(s.name)));
  assert.ok(!steps.items.some((s) => /upgrade from a previous release/.test(s.name)));
});

test("macOS: a same-version 'old' build is a labelled rerun and the upgrade stays UNPROVEN", () => {
  const steps = quiet(() => runMacos(macCtx(macRun(), { readDir: readDirFor("macos", true) })));
  assert.deepEqual(failures(steps), []);
  assert.equal(steps.upgrade, "same-version");
  assert.ok(steps.items.some((s) => s.verdict === "PASS" && /SAME-VERSION rerun; NOT an upgrade/.test(s.name)));
  assert.ok(!steps.items.some((s) => /\(upgrade: old sidecar/.test(s.name)));
  assert.ok(steps.items.some((s) => s.verdict === "UNPROVEN" && /upgrade from a previous release \(N-1 to N\).*both version 0\.1\.0/.test(`${s.name}${s.detail}`)));
});

// Mutation tests: every way a smoke binary can pass its exit code without having run the smoke.
const smokeVerdict = (smokeOptions, over = {}) => {
  const machine = macRun({ oldVersion: "0.0.9" });
  const steps = quiet(() => runMacos(macCtx(machine, { ...withSmoke(machine.run, smokeOptions), ...over })));
  return steps.items.find((s) => /^daemon smoke/.test(s.name));
};

test("smoke: the baseline passes", () => {
  assert.equal(smokeVerdict({}).verdict, "PASS");
});

test("smoke: a stub that exits 0 and writes nothing fails", () => {
  const step = smokeVerdict({ write: () => null });
  assert.equal(step.verdict, "FAIL");
  assert.match(step.detail, /exited 0 but wrote no structured result/);
});

test("smoke: a stale result file does not rescue a failing exit code", () => {
  const step = smokeVerdict({ code: 101 });
  assert.equal(step.verdict, "FAIL");
  assert.match(step.detail, /release_smoke exited 101/);
});

test("smoke: wrong, truncated or foreign results fail", () => {
  const cases = {
    "not json": [() => "pass", /not valid JSON/],
    "result is not PASS": [(r) => ({ ...r, result: "FAIL" }), /expected "PASS"/],
    "11 steps": [(r) => ({ ...r, steps: r.steps.slice(0, 11) }), /not 1\.\.12 in order/],
    "steps out of order": [(r) => ({ ...r, steps: [...r.steps].reverse() }), /not 1\.\.12 in order/],
    "wrong new version": [(r) => ({ ...r, daemon_versions: [r.daemon_versions[0], "9.9.9"] }), /new daemon version is 9\.9\.9/],
    "wrong old version": [(r) => ({ ...r, daemon_versions: ["0.0.1", r.daemon_versions[1]] }), /old daemon version is 0\.0\.1/],
    "same_version lies": [(r) => ({ ...r, same_version: true }), /same_version disagrees/],
    "ran another sidecar": [(r) => ({ ...r, new_daemon: "/elsewhere/orchestrator-daemon" }), /not the packaged sidecar/],
    "ran another old sidecar": [(r) => ({ ...r, old_daemon: "/elsewhere/old" }), /as the old daemon/],
    "not an upgrade run": [(r) => ({ ...r, upgrade_requested: false }), /upgrade_requested is false/],
    "wrong schema": [(r) => ({ ...r, schema: 2 }), /schema is 2/],
    "an array": [() => [], /not a JSON object/],
  };
  for (const [name, [write, pattern]] of Object.entries(cases)) {
    const step = smokeVerdict({ write });
    assert.equal(step.verdict, "FAIL", name);
    assert.match(step.detail, pattern, name);
  }
});

test("smoke: a spawn failure keeps its cause", () => {
  const machine = macRun();
  const run = (command, args, options) => (command === "/smoke" ? { code: -1, out: "", err: "", error: "spawnSync /smoke ENOENT" } : machine.run(command, args, options));
  const steps = quiet(() => runMacos(macCtx(machine, { run, readDir: readDirFor("macos", true) })));
  assert.match(steps.items.find((s) => /^daemon smoke/.test(s.name)).detail, /exited -1: spawnSync \/smoke ENOENT/);
});

test("classifyUpgrade needs different versions and different bytes", () => {
  const info = (version) => ({ version });
  assert.equal(classifyUpgrade({ oldInfo: null, newInfo: info("1"), oldHash: "a", newHash: "b" }).kind, "none");
  assert.equal(classifyUpgrade({ oldInfo: info("1"), newInfo: info("1"), oldHash: "a", newHash: "b" }).kind, "same-version");
  assert.equal(classifyUpgrade({ oldInfo: info("0"), newInfo: info("1"), oldHash: "a", newHash: "a" }).kind, "same-version");
  assert.equal(classifyUpgrade({ oldInfo: info("0"), newInfo: info("1"), oldHash: null, newHash: "a" }).kind, "unverified");
  assert.equal(classifyUpgrade({ oldInfo: info("0"), newInfo: info("1"), oldHash: "a", newHash: "b" }).kind, "upgrade");
});

test("validateSmokeResult accepts exactly what release_smoke writes", () => {
  const args = ["--new-daemon", "/n", "--expect-new-version", "0.1.0", "--out", "/o"];
  assert.deepEqual(validateSmokeResult(smokeResult(args), { version: "0.1.0", oldVersion: undefined, oldDaemon: null, newDaemon: "/n" }), []);
  assert.deepEqual(validateSmokeResult(null, { version: "0.1.0", newDaemon: "/n" }), ["the smoke result is not a JSON object"]);
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

// A fake Windows machine. `uninstallHook(userData)` is what the uninstaller does to the data directory;
// `copyCode` is the exit code of the Copy-Item of the old sidecar.
const windowsMachine = ({ uninstallHook = () => {}, installHook = () => {}, copyCode = 0, authenticode = "Valid|CN=Example|True", oldVersion = "0.0.9" } = {}) => {
  const commands = [];
  const userData = fakeUserData();
  let uninstalled = false;
  let installs = 0;
  let version = oldVersion;
  const run = (command, args) => {
    commands.push([command, ...args].join(" "));
    const text = args.join(" ");
    if (text.includes("Get-AuthenticodeSignature")) return { code: 0, out: authenticode, err: "" };
    if (text.includes("ProductVersion")) return { code: 0, out: version, err: "" };
    if (text.includes("Copy-Item")) return { code: copyCode, out: "", err: copyCode ? "Cannot find path" : "" };
    if (command.endsWith("-setup.exe") && ++installs === 2) { version = "0.1.0"; installHook(userData); }
    if (command.endsWith("uninstall.exe")) { uninstalled = true; uninstallHook(userData); }
    return { code: 0, out: "", err: "" };
  };
  // The old-sidecar copy only exists when Copy-Item succeeded; the app is gone once uninstalled.
  const exists = (path) => !(path.endsWith("agent-orch-desktop.exe") && uninstalled) && !(path.endsWith("old-orchestrator-daemon.exe") && copyCode);
  return { commands, run, exists, userData };
};
const windowsCtx = (machine, over = {}) => ctxFor("windows", machine, {
  requireSigned: true, smokeBin: "C:/smoke.exe", oldArtifactDir: "/old", readDir: readDirFor("windows", true, "0.0.9"), userData: machine.userData, ...withSmoke(machine.run), ...over,
});
const failures = (steps) => steps.items.filter((s) => s.verdict === "FAIL").map((s) => s.name);

test("Windows: installs silently per user, upgrades over the same directory, uninstalls pinned to the directory", () => {
  const machine = windowsMachine();
  const steps = quiet(() => runWindows(windowsCtx(machine)));
  assert.deepEqual(failures(steps), []);
  const order = machine.commands.filter((c) => /setup\.exe|smoke|uninstall/.test(c));
  assert.equal(order.length, 4, order.join("\n"));
  assert.match(order[0], /-setup\.exe \/S \/D=.*AgentOrchestrator$/);
  assert.match(order[1], /-setup\.exe \/S \/D=.*AgentOrchestrator$/);
  assert.match(order[2], /smoke\.exe --new-daemon .*--old-daemon .*old-orchestrator-daemon\.exe.*--out \/w\/result\.json/);
  assert.match(order[3], /uninstall\.exe \/S _\?=.*AgentOrchestrator$/);
  assert.ok(steps.items.some((s) => s.verdict === "UNPROVEN" && /CLEAN Windows/.test(s.name)));
  assert.ok(steps.items.some((s) => s.verdict === "PASS" && /sentinel in the real data directory survived the upgrade-over/.test(s.name)));
  assert.ok(steps.items.some((s) => s.verdict === "PASS" && /sentinel in the real data directory survived the uninstall/.test(s.name)));
  assert.equal(machine.userData.files.size, 0, "the sentinel is removed when the run ends");
});

test("Windows: an uninstaller that deletes the user's data directory content fails", () => {
  const machine = windowsMachine({ uninstallHook: (data) => data.files.clear() });
  const steps = quiet(() => runWindows(windowsCtx(machine)));
  assert.deepEqual(failures(steps), ["the user data sentinel in the real data directory survived the uninstall"]);
});

test("Windows: an upgrade-over install that deletes the user's data fails", () => {
  const machine = windowsMachine({ installHook: (data) => data.files.clear() });
  const steps = quiet(() => runWindows(windowsCtx(machine)));
  assert.ok(failures(steps).includes("the user data sentinel in the real data directory survived the upgrade-over install"));
});

test("Windows: a missing LOCALAPPDATA fails closed instead of passing the data check", () => {
  const machine = windowsMachine();
  const steps = quiet(() => runWindows(windowsCtx(machine, { userData: { root: null } })));
  assert.ok(failures(steps).some((name) => /sentinel file in the real user data directory/.test(name)));
  assert.ok(failures(steps).some((name) => /survived the uninstall/.test(name)));
});

test("Windows: a failed copy of the old sidecar fails at its cause and the smoke is not run", () => {
  const machine = windowsMachine({ copyCode: 1 });
  const steps = quiet(() => runWindows(windowsCtx(machine)));
  assert.ok(failures(steps).includes("old: copy the old sidecar out of the install directory"));
  assert.ok(!machine.commands.some((c) => c.startsWith("C:/smoke.exe")));
  assert.equal(steps.upgrade, "unverified");
});

test("Windows: an unsigned installer is INFO unless a signature is required", () => {
  const loose = quiet(() => runWindows(windowsCtx(windowsMachine({ authenticode: "NotSigned||False", oldVersion: "0.1.0" }), { requireSigned: false, oldArtifactDir: undefined, readDir: readDirFor("windows", false) })));
  assert.ok(loose.items.some((s) => s.verdict === "INFO" && /UNSIGNED installer/.test(s.name)));
  assert.deepEqual(failures(loose), []);
  const strict = quiet(() => runWindows(windowsCtx(windowsMachine({ authenticode: "NotSigned||False", oldVersion: "0.1.0" }), { oldArtifactDir: undefined, readDir: readDirFor("windows", false) })));
  assert.ok(strict.failed, "an unsigned artifact must fail when a signature is required");
  assert.ok(strict.items.some((s) => s.verdict === "FAIL" && /signed artifact was required/.test(`${s.name}${s.detail}`)));
});

test("Windows: the same-version rerun is labelled and is not an upgrade pass", () => {
  const machine = windowsMachine({ oldVersion: "0.1.0" });
  const steps = quiet(() => runWindows(windowsCtx(machine, { readDir: readDirFor("windows", true) })));
  assert.equal(steps.upgrade, "same-version");
  assert.deepEqual(failures(steps), []);
  const identical = quiet(() => runWindows(windowsCtx(windowsMachine(), { fileHash: () => "same-bytes" })));
  assert.equal(identical.upgrade, "same-version", "different declared versions but byte-identical sidecars are not an upgrade either");
  assert.ok(steps.items.some((s) => s.verdict === "PASS" && /SAME-VERSION rerun; NOT an upgrade/.test(s.name)));
  assert.ok(steps.items.some((s) => s.verdict === "UNPROVEN" && /upgrade from a previous release \(N-1 to N\)/.test(s.name)));
});
