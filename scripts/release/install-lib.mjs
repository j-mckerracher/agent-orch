// Install, upgrade and uninstall smoke test for a packaged release artifact. Pure checks and the
// per-platform flows live here; install-smoke.mjs is the command line.
//
// Every step ends PASS, FAIL, INFO (measured and reported, not judged) or UNPROVEN (this run cannot
// prove it, and says why). Only FAIL fails the run. A flow never claims what it did not run: a
// requested real-signing check on an unsigned artifact is a FAIL, not a skip.
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";

export const APP_NAME = "Agent Orchestrator";
const escapeRegExp = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

export const artifactNamePattern = (platform, version) => {
  const v = escapeRegExp(version);
  return platform === "macos"
    ? new RegExp(`^Agent-Orchestrator-${v}-macos-(arm64|x64)-(signed|unsigned)\\.zip$`)
    : new RegExp(`^Agent-Orchestrator-${v}-windows-x64-(signed|unsigned)-setup\\.exe$`);
};

export const parseSums = (text) => {
  const sums = new Map();
  for (const line of text.split(/\r?\n/).filter(Boolean)) {
    const match = /^([0-9a-f]{64}) {2}(.+)$/.exec(line);
    if (!match) return { error: `malformed SHA256SUMS line: ${line}`, sums };
    sums.set(match[2], match[1]);
  }
  return { error: null, sums };
};

/** Problems with one artifact directory as `package.mjs` writes it; `hashes` maps file to sha256. */
export const checkArtifactDir = ({ platform, version, requireSigned, info, sumsText, hashes }) => {
  const problems = [];
  if (!info) return ["release-info.json is missing or unreadable"];
  if (info.platform !== platform) problems.push(`release-info platform is ${info.platform}, expected ${platform}`);
  if (info.version !== version) problems.push(`release-info version is ${info.version}, expected ${version}`);
  if (info.notarized && !info.signed) problems.push("release-info says notarized but not signed");
  if (requireSigned && !info.signed) problems.push("a signed artifact was required but release-info says signed=false");
  const artifacts = info.artifacts ?? [];
  if (artifacts.length !== 1) problems.push(`expected exactly one artifact, found ${artifacts.length}`);
  const parsed = parseSums(sumsText ?? "");
  if (parsed.error) problems.push(parsed.error);
  for (const { file, sha256 } of artifacts) {
    const match = artifactNamePattern(platform, version).exec(file);
    if (!match) problems.push(`artifact name ${file} does not match the ${platform} naming for ${version}`);
    else if ((match.at(-1) === "signed") !== Boolean(info.signed)) problems.push(`artifact name ${file} disagrees with signed=${info.signed}`);
    if (hashes[file] === undefined) problems.push(`${file} is not in the directory`);
    else if (hashes[file] !== sha256) problems.push(`${file} sha256 differs from release-info.json`);
    if (parsed.sums.get(file) !== sha256) problems.push(`SHA256SUMS does not list ${file} with the release-info sha256`);
  }
  if (parsed.sums.size !== artifacts.length) problems.push("SHA256SUMS lists a different number of files than release-info.json");
  return problems;
};

export const readArtifactDir = (dir) => {
  const read = (name) => (existsSync(join(dir, name)) ? readFileSync(join(dir, name), "utf8") : null);
  const infoText = read("release-info.json");
  let info = null;
  try { info = infoText ? JSON.parse(infoText) : null; } catch { info = null; }
  const hashes = {};
  for (const file of readdirSync(dir)) {
    if (file === "release-info.json" || file === "SHA256SUMS") continue;
    hashes[file] = createHash("sha256").update(readFileSync(join(dir, file))).digest("hex");
  }
  return { info, sumsText: read("SHA256SUMS"), hashes };
};

// ---- macOS signature facts ----

/** Reads `codesign -dvv` output (it prints to stderr). */
export const parseCodesignDetails = (text) => ({
  adhoc: /^Signature=adhoc$/m.test(text),
  authorities: [...text.matchAll(/^Authority=(.+)$/gm)].map((m) => m[1]),
  teamId: /^TeamIdentifier=(?!not set)(\S+)$/m.exec(text)?.[1] ?? null,
  hardenedRuntime: /^CodeDirectory .*flags=0x[0-9a-f]+\([^)]*runtime/m.test(text),
});

/** What a Developer ID release must show; returns the problems. */
export const checkDeveloperIdSignature = (details, { notarized }) => {
  const problems = [];
  if (details.adhoc) problems.push("the signature is ad hoc, not a Developer ID signature");
  if (!details.authorities.some((a) => a.startsWith("Developer ID Application:"))) problems.push("no 'Developer ID Application' authority");
  if (!details.teamId) problems.push("no TeamIdentifier");
  if (notarized && !details.hardenedRuntime) problems.push("the hardened runtime flag is absent, which notarization requires");
  return problems;
};

// ---- Windows signature facts ----

// No double quote appears in the script and it is passed as -EncodedCommand (base64 of UTF-16LE), so no
// argument quoting on any Windows shell can change it.
export const AUTHENTICODE_SCRIPT =
  "$s = Get-AuthenticodeSignature -LiteralPath $env:ARTIFACT; " +
  "($s.Status, $s.SignerCertificate.Subject, [bool]$s.TimeStamperCertificate) -join '|'";

export const authenticodeArgs = () => ["-NoProfile", "-NonInteractive", "-EncodedCommand", Buffer.from(AUTHENTICODE_SCRIPT, "utf16le").toString("base64")];

const firstLine = (text) => String(text ?? "").trim().split(/\r?\n/)[0];

/**
 * Reads the answer of AUTHENTICODE_SCRIPT. An empty or garbled answer is status "Unknown" with the reason in
 * `note`, never a blank status: a value that was not measured must say so.
 */
export const parseAuthenticode = (text, { code = 0, err = "" } = {}) => {
  const [status = "", subject = "", stamped = "False"] = String(text ?? "").trim().split(/\r?\n/)[0].split("|");
  if (!status.trim()) {
    const why = firstLine(err);
    return { status: "Unknown", subject: "", timestamped: false, note: `powershell exited ${code} with no status${why ? `: ${why}` : ""}` };
  }
  return { status: status.trim(), subject: subject.trim(), timestamped: stamped.trim() === "True" };
};

// ---- source checkout facts ----

/** Paths `git status --porcelain=v1` lists, one per line (LF or CRLF), as a sorted array. */
export const parsePorcelain = (text) => String(text ?? "").split(/\r?\n/).filter(Boolean).sort();

/** What a run changed in the checkout: lines that appeared or vanished between two porcelain listings. */
export const diffPorcelain = (before, after) => {
  const was = new Set(before);
  const is = new Set(after);
  return { added: after.filter((line) => !was.has(line)), removed: before.filter((line) => !is.has(line)) };
};

/** The commit, the tree and the checkout state around a run; `before`/`after` are porcelain arrays, or null when git failed. */
export const describeSource = ({ head, tree, before, after }) => {
  const known = before !== null && after !== null;
  const changed = known ? diffPorcelain(before, after) : null;
  const state = (list) => (list === null ? "unknown (git status failed)" : list.length === 0 ? "clean" : `${list.length} path(s) listed`);
  const lines = [
    `source commit: ${head ?? "unknown"}`,
    `source tree: ${tree ?? "unknown"}`,
    `source checkout before the run: ${state(before)}`,
    `source checkout after the run: ${state(after)}`,
    ...(known ? [changed.added.length || changed.removed.length
      ? `changed by this run: ${[...changed.added.map((l) => `+ ${l}`), ...changed.removed.map((l) => `- ${l}`)].join("; ")}`
      : "changed by this run: nothing"] : []),
    ...(after !== null && after.length ? [`still listed after the run: ${after.join("; ")}`] : []),
  ];
  return { head, tree, statusBefore: before, statusAfter: after, changedByRun: changed, cleanAfterRun: after === null ? null : after.length === 0, lines };
};

export const checkAuthenticode = (facts) => {
  const problems = [];
  if (facts.status !== "Valid") problems.push(`Authenticode status is ${facts.status}${facts.note ? ` (${facts.note})` : ""}, expected Valid`);
  if (!facts.subject) problems.push("no signer certificate");
  if (!facts.timestamped) problems.push("no countersignature timestamp (the signature would expire with the certificate)");
  return problems;
};

// ---- flows ----

export const SMOKE_STEP_COUNT = 12;
const SMOKE_RESULT_SCHEMA = 1;

const sameFile = (a, b) => typeof a === "string" && typeof b === "string" && resolve(a) === resolve(b);

/**
 * Problems with the structured result `release_smoke --out` writes. The exit code alone proves
 * nothing: a stub that exits 0 must fail here. `oldDaemon` is null for a reinstall.
 */
export const validateSmokeResult = (result, { version, oldVersion, oldDaemon, newDaemon }) => {
  if (result === null || typeof result !== "object" || Array.isArray(result)) return ["the smoke result is not a JSON object"];
  const problems = [];
  if (result.schema !== SMOKE_RESULT_SCHEMA) problems.push(`result schema is ${result.schema}, expected ${SMOKE_RESULT_SCHEMA}`);
  if (result.result !== "PASS") problems.push(`result is ${JSON.stringify(result.result)}, expected "PASS"`);
  const numbers = Array.isArray(result.steps) ? result.steps.map((s) => s?.n) : [];
  if (numbers.join() !== Array.from({ length: SMOKE_STEP_COUNT }, (_, i) => i + 1).join()) problems.push(`steps ${JSON.stringify(numbers)} are not 1..${SMOKE_STEP_COUNT} in order`);
  const versions = result.daemon_versions;
  if (!Array.isArray(versions) || versions.length !== 2 || versions.some((v) => typeof v !== "string")) problems.push("daemon_versions is not two strings");
  else {
    if (versions[1] !== version) problems.push(`new daemon version is ${versions[1]}, expected ${version}`);
    if (versions[0] !== (oldDaemon ? oldVersion : versions[1])) problems.push(`old daemon version is ${versions[0]}, expected ${oldDaemon ? oldVersion : "the new version (reinstall)"}`);
    if (result.same_version !== (versions[0] === versions[1])) problems.push("same_version disagrees with daemon_versions");
  }
  if (result.upgrade_requested !== Boolean(oldDaemon)) problems.push(`upgrade_requested is ${result.upgrade_requested}, expected ${Boolean(oldDaemon)}`);
  if (!sameFile(result.new_daemon, newDaemon)) problems.push(`the smoke ran ${result.new_daemon}, not the packaged sidecar ${newDaemon}`);
  if (oldDaemon && !sameFile(result.old_daemon, oldDaemon)) problems.push(`the smoke ran ${result.old_daemon} as the old daemon, expected ${oldDaemon}`);
  return problems;
};

/**
 * "upgrade" only when the two builds differ in declared version AND in bytes. A same-version or
 * byte-identical pair is a reinstall rehearsal, never an N-1 upgrade; unknown hashes prove nothing.
 */
export const classifyUpgrade = ({ oldInfo, newInfo, oldHash, newHash }) => {
  if (!oldInfo) return { kind: "none", why: "no --old-artifact-dir was given, so no older build ran" };
  if (oldInfo.version === newInfo.version) return { kind: "same-version", why: `old and new are both version ${newInfo.version}` };
  if (!oldHash || !newHash) return { kind: "unverified", why: "the daemon sidecar hashes could not be compared" };
  if (oldHash === newHash) return { kind: "same-version", why: "the old and new daemon sidecars are byte-identical" };
  return { kind: "upgrade", why: `${oldInfo.version} -> ${newInfo.version}` };
};

const smokeSuffix = { upgrade: " (upgrade: old sidecar then new sidecar on one data directory)", none: " (reinstall; NO upgrade)" };
const SAME_VERSION_LABEL = " (SAME-VERSION rerun; NOT an upgrade)";

/** Runs release_smoke and accepts it only with exit 0 AND a valid structured result. */
const runSmoke = (steps, ctx, { oldDaemon, oldVersion, newDaemon, kind }) => {
  const out = ctx.smokeOut;
  const args = [
    "--new-daemon", newDaemon, ...(oldDaemon ? ["--old-daemon", oldDaemon, "--expect-old-version", oldVersion] : []),
    "--work", ctx.smokeWork, "--expect-new-version", ctx.version, "--out", out,
  ];
  const label = `daemon smoke against the packaged sidecar${smokeSuffix[kind] ?? SAME_VERSION_LABEL}`;
  const smoke = ctx.run(ctx.smokeBin, args, { stream: true });
  const problems = [];
  if (smoke.code !== 0) {
    problems.push(`release_smoke exited ${smoke.code}${smoke.signal ? ` (signal ${smoke.signal})` : ""}${smoke.error ? `: ${smoke.error}` : ""}`);
  } else {
    const text = ctx.readText(out);
    let parsed = null;
    if (text === null) problems.push(`release_smoke exited 0 but wrote no structured result at ${out}`);
    else {
      try { parsed = JSON.parse(text); } catch (error) { problems.push(`the smoke result is not valid JSON: ${error.message}`); }
      if (parsed !== null) problems.push(...validateSmokeResult(parsed, { version: ctx.version, oldVersion, oldDaemon, newDaemon }));
    }
  }
  // A failed smoke keeps its work directory, and stops its fake agents; the path is the evidence.
  steps.check(label, problems, `${ctx.smokeWork}${problems.length ? " (kept for inspection)" : ""}`);
};

const reportUpgrade = (steps, upgrade) => {
  steps.upgrade = upgrade.kind;
  if (upgrade.kind !== "upgrade") steps.add("UNPROVEN", "upgrade from a previous release (N-1 to N)", upgrade.why);
};

class Steps {
  constructor() { this.items = []; this.upgrade = "none"; }
  add(verdict, name, detail = "") {
    this.items.push({ verdict, name, detail });
    console.log(`${verdict} ${name}${detail ? `: ${String(detail).split("\n")[0]}` : ""}`);
  }
  check(name, problems, detail = "") { this.add(problems.length ? "FAIL" : "PASS", name, problems.length ? problems.join("; ") : detail); }
  get failed() { return this.items.some((s) => s.verdict === "FAIL"); }
}

// A quarantined bundle that fails Gatekeeper is killed on exec and macOS then deletes it, so the
// quarantine bit goes on a throwaway copy, never on the copy the daemon smoke runs from.
const assessQuarantined = (run, app, work, label) => {
  const probe = join(work, `quarantine-probe-${label}`);
  run("ditto", [app, join(probe, `${APP_NAME}.app`)]);
  run("xattr", ["-w", "com.apple.quarantine", "0081;00000000;release-smoke;", join(probe, `${APP_NAME}.app`)]);
  const spctl = run("spctl", ["--assess", "--type", "execute", "--verbose=2", join(probe, `${APP_NAME}.app`)]);
  run("rm", ["-rf", probe]);
  return spctl;
};

const verifyDirectory = (steps, label, dir, ctx) => {
  const { info, sumsText, hashes } = ctx.readDir(dir);
  const version = label === "new" ? ctx.version : info?.version;
  const problems = checkArtifactDir({ platform: ctx.platform, version, requireSigned: ctx.requireSigned && label === "new", info, sumsText, hashes });
  steps.check(`${label} artifact directory is consistent (checksums, naming, release-info)`, problems, dir);
  return { info, ok: problems.length === 0 };
};

/** macOS: the zip is "installed" by extracting it into a folder that stands in for /Applications. */
export const runMacos = (ctx) => {
  const steps = new Steps();
  const { run } = ctx;
  const install = (label, dir) => {
    const { info, ok } = verifyDirectory(steps, label, dir, ctx);
    if (!ok) return null;
    const apps = join(ctx.work, `Applications-${label}`);
    const unzip = run("ditto", ["-x", "-k", join(dir, info.artifacts[0].file), apps]);
    steps.check(`${label}: extract the zip into a stand-in Applications folder`, unzip.code === 0 ? [] : [`ditto exited ${unzip.code}: ${unzip.err}`]);
    return unzip.code === 0 ? { info, app: join(apps, `${APP_NAME}.app`) } : null;
  };

  const next = install("new", ctx.artifactDir);
  const old = ctx.oldArtifactDir ? install("old", ctx.oldArtifactDir) : null;
  if (!next) return steps;

  for (const [label, entry] of [["new", next], ["old", old]].filter(([, e]) => e)) {
    const { app, info } = entry;
    const plist = run("/usr/libexec/PlistBuddy", ["-c", "Print :CFBundleShortVersionString", join(app, "Contents", "Info.plist")]);
    steps.check(`${label}: bundle version`, plist.out.trim() === info.version ? [] : [`CFBundleShortVersionString is '${plist.out.trim()}', release-info says ${info.version}`], info.version);
    const missing = ["agent-orch-desktop", "orchestrator-daemon"].filter((f) => !ctx.exists(join(app, "Contents", "MacOS", f)));
    steps.check(`${label}: the app executable and the daemon sidecar are in Contents/MacOS`, missing.map((f) => `${f} is missing`));

    const verify = run("codesign", ["--verify", "--deep", "--strict", "--verbose=2", app]);
    const details = parseCodesignDetails(run("codesign", ["-dvv", app]).err);
    if (info.signed) {
      steps.check(`${label}: codesign --verify --deep --strict`, verify.code === 0 ? [] : [verify.err]);
      steps.check(`${label}: Developer ID signature, team and runtime flag`, checkDeveloperIdSignature(details, { notarized: info.notarized }), details.teamId);
      const sidecar = parseCodesignDetails(run("codesign", ["-dvv", join(app, "Contents", "MacOS", "orchestrator-daemon")]).err);
      steps.check(`${label}: the sidecar carries the app's team identifier`, sidecar.teamId && sidecar.teamId === details.teamId ? [] : [`sidecar team ${sidecar.teamId}, app team ${details.teamId}`]);
      const spctl = assessQuarantined(run, app, ctx.work, label);
      steps.check(`${label}: Gatekeeper (spctl) accepts the quarantined app`, spctl.code === 0 && (!info.notarized || /Notarized Developer ID/.test(spctl.err)) ? [] : [`spctl exited ${spctl.code}: ${spctl.err}`]);
      if (info.notarized) {
        const staple = run("xcrun", ["stapler", "validate", app]);
        steps.check(`${label}: the notarization ticket is stapled`, staple.code === 0 ? [] : [staple.err]);
      }
    } else {
      steps.add(ctx.requireSigned && label === "new" ? "FAIL" : "INFO", `${label}: UNSIGNED artifact; codesign --verify exit ${verify.code}`, details.adhoc ? "ad hoc signature only" : "no Developer ID signature");
      const spctl = assessQuarantined(run, app, ctx.work, label);
      steps.add("INFO", `${label}: Gatekeeper on the quarantined unsigned app (expected: rejected)`, `spctl exit ${spctl.code} ${spctl.err.trim()}`);
    }
  }

  steps.add("UNPROVEN", "Gatekeeper first-launch on a CLEAN Mac, the quarantine prompt, and launching the GUI", "needs the clean-machine runbook; this run never opens the app");
  const sidecar = (entry) => join(entry.app, "Contents", "MacOS", "orchestrator-daemon");
  const upgrade = classifyUpgrade({
    oldInfo: old?.info, newInfo: next.info, oldHash: old && ctx.fileHash(sidecar(old)), newHash: ctx.fileHash(sidecar(next)),
  });
  reportUpgrade(steps, upgrade);
  if (!ctx.smokeBin) {
    steps.add("UNPROVEN", "daemon smoke (install, cancel, recover, approve, reject) against the packaged sidecar", "no --smoke-bin was given");
    return steps;
  }
  runSmoke(steps, ctx, { oldDaemon: old && sidecar(old), oldVersion: old?.info.version, newDaemon: sidecar(next), kind: upgrade.kind });
  return steps;
};

/** Windows: the NSIS installer in per-user mode. Written for a Windows machine, not run on this one. */
export const runWindows = (ctx) => {
  const steps = new Steps();
  const { run } = ctx;
  const installDir = join(ctx.work, "AgentOrchestrator");
  // A PowerShell 7 parent leaves a PSModulePath that Windows PowerShell cannot load Microsoft.PowerShell.Security from, so it is dropped.
  const authenticode = (path) => {
    const answer = run("powershell.exe", authenticodeArgs(), { env: { ARTIFACT: path, PSModulePath: undefined } });
    return parseAuthenticode(answer.out, answer);
  };

  const prepared = {};
  for (const [label, dir] of [["new", ctx.artifactDir], ["old", ctx.oldArtifactDir]].filter(([, d]) => d)) {
    const { info, ok } = verifyDirectory(steps, label, dir, ctx);
    if (!ok) return steps;
    const installer = join(dir, info.artifacts[0].file);
    const facts = authenticode(installer);
    if (info.signed) steps.check(`${label}: installer Authenticode signature`, checkAuthenticode(facts), facts.subject);
    else steps.add(ctx.requireSigned && label === "new" ? "FAIL" : "INFO", `${label}: UNSIGNED installer (Authenticode status ${facts.status})`, `SmartScreen will warn on a download${facts.note ? `; the status was NOT measured: ${facts.note}` : ""}`);
    prepared[label] = { info, installer };
  }

  const exe = (name) => join(installDir, `${name}.exe`);
  const install = (label) => {
    const result = run(prepared[label].installer, ["/S", `/D=${installDir}`]);
    steps.check(`${label}: silent per-user install with no elevation prompt`, result.code === 0 ? [] : [`installer exited ${result.code}`], installDir);
    const missing = ["agent-orch-desktop", "orchestrator-daemon", "uninstall"].filter((f) => !ctx.exists(exe(f)));
    steps.check(`${label}: the app, the daemon sidecar and the uninstaller are installed`, missing.map((f) => `${f}.exe is missing`));
    const version = run("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", `(Get-Item -LiteralPath '${exe("agent-orch-desktop")}').VersionInfo.ProductVersion`]).out.trim();
    steps.check(`${label}: installed product version`, version.startsWith(prepared[label].info.version) ? [] : [`ProductVersion is '${version}', expected ${prepared[label].info.version}`], version);
    if (prepared[label].info.signed) {
      for (const name of ["agent-orch-desktop", "orchestrator-daemon"]) {
        steps.check(`${label}: ${name}.exe Authenticode signature`, checkAuthenticode(authenticode(exe(name))));
      }
    }
  };

  // The real data directory (%LOCALAPPDATA%\agent-orch, see the daemon's default) gets a sentinel before the first
  // install; neither the upgrade-over install nor the uninstall may remove it. No directory, no check: FAIL.
  const data = ctx.userData;
  const sentinel = { name: `install-smoke-sentinel-${ctx.nonce}.txt`, text: `install-smoke sentinel ${ctx.nonce}` };
  let seeded = false;
  if (!data?.root) steps.add("FAIL", "seed a sentinel file in the real user data directory (%LOCALAPPDATA%\\agent-orch)", "LOCALAPPDATA is not set, so the data-survival check cannot run");
  else {
    try { data.seed(sentinel.name, sentinel.text); seeded = true; } catch (error) { steps.add("FAIL", `seed a sentinel file in ${data.root}`, error.message); }
    if (seeded) steps.add("PASS", `seeded a sentinel file in the real user data directory`, join(data.root, sentinel.name));
  }
  const sentinelSurvived = (when) => steps.check(`the user data sentinel in the real data directory survived ${when}`,
    !seeded ? ["the sentinel was not seeded"] : data.read(sentinel.name) === sentinel.text ? [] : [`${join(data.root, sentinel.name)} is missing or changed`]);

  try {
    if (prepared.old) install("old");
    // The smoke needs both daemon binaries at once, so the old sidecar is copied out before the upgrade overwrites it.
    const oldSidecar = prepared.old ? join(ctx.work, "old-orchestrator-daemon.exe") : null;
    let oldCopied = false;
    if (oldSidecar) {
      const copy = run("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", `$ErrorActionPreference = 'Stop'; Copy-Item -LiteralPath '${exe("orchestrator-daemon")}' -Destination '${oldSidecar}'`]);
      oldCopied = copy.code === 0 && ctx.exists(oldSidecar);
      steps.check("old: copy the old sidecar out of the install directory", oldCopied ? [] : [`Copy-Item exited ${copy.code}${copy.err ? `: ${copy.err.trim()}` : ""}${copy.code === 0 ? "; the copy is not there" : ""}`], oldSidecar);
    }
    install("new");
    if (prepared.old) sentinelSurvived("the upgrade-over install");

    const upgrade = classifyUpgrade({
      oldInfo: oldCopied ? prepared.old.info : null, newInfo: prepared.new.info,
      oldHash: oldCopied ? ctx.fileHash(oldSidecar) : null, newHash: ctx.fileHash(exe("orchestrator-daemon")),
    });
    reportUpgrade(steps, oldSidecar && !oldCopied ? { kind: "unverified", why: "the old sidecar could not be copied out" } : upgrade);
    if (!ctx.smokeBin) steps.add("UNPROVEN", "daemon smoke against the installed sidecar", "no --smoke-bin was given");
    else if (oldSidecar && !oldCopied) steps.add("FAIL", "daemon smoke against the installed sidecar", "skipped: the old sidecar copy failed");
    else runSmoke(steps, ctx, { oldDaemon: oldSidecar, oldVersion: prepared.old?.info.version, newDaemon: exe("orchestrator-daemon"), kind: upgrade.kind });
  } finally {
    // NSIS copies its uninstaller aside and returns at once unless `_?=` pins it to the install directory.
    const uninstall = run(exe("uninstall"), ["/S", `_?=${installDir}`]);
    steps.check("silent uninstall", uninstall.code === 0 ? [] : [`uninstaller exited ${uninstall.code}`]);
    steps.check("uninstall removed the app", ctx.exists(exe("agent-orch-desktop")) ? ["agent-orch-desktop.exe is still installed"] : []);
    sentinelSurvived("the uninstall");
    if (seeded) data.remove(sentinel.name);
  }
  steps.add("UNPROVEN", "SmartScreen reputation, UAC absence on a standard user, Windows Defender, and the GUI flow on a CLEAN Windows machine", "needs the clean-machine runbook");
  return steps;
};
