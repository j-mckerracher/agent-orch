// Install, upgrade and uninstall smoke test for a packaged release artifact. Pure checks and the
// per-platform flows live here; install-smoke.mjs is the command line.
//
// Every step ends PASS, FAIL, INFO (measured and reported, not judged) or UNPROVEN (this run cannot
// prove it, and says why). Only FAIL fails the run. A flow never claims what it did not run: a
// requested real-signing check on an unsigned artifact is a FAIL, not a skip.
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

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

export const AUTHENTICODE_SCRIPT =
  "$s = Get-AuthenticodeSignature -LiteralPath $env:ARTIFACT; " +
  "\"$($s.Status)|$($s.SignerCertificate.Subject)|$([bool]$s.TimeStamperCertificate)\"";

export const parseAuthenticode = (text) => {
  const [status, subject = "", stamped = "False"] = text.trim().split("|");
  return { status, subject, timestamped: stamped === "True" };
};

export const checkAuthenticode = (facts) => {
  const problems = [];
  if (facts.status !== "Valid") problems.push(`Authenticode status is ${facts.status}, expected Valid`);
  if (!facts.subject) problems.push("no signer certificate");
  if (!facts.timestamped) problems.push("no countersignature timestamp (the signature would expire with the certificate)");
  return problems;
};

// ---- flows ----

const smokeArgs = (ctx, oldDaemon, oldVersion, newDaemon) => [
  "--new-daemon", newDaemon, ...(oldDaemon ? ["--old-daemon", oldDaemon, "--expect-old-version", oldVersion] : []),
  "--work", ctx.smokeWork, "--expect-new-version", ctx.version,
];

class Steps {
  constructor() { this.items = []; }
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
  if (!old) steps.add("UNPROVEN", "upgrade from a previous release", "no --old-artifact-dir was given, so no older build ran");
  if (!ctx.smokeBin) {
    steps.add("UNPROVEN", "daemon smoke (install, cancel, recover, approve, reject) against the packaged sidecar", "no --smoke-bin was given");
    return steps;
  }
  const sidecar = (entry) => join(entry.app, "Contents", "MacOS", "orchestrator-daemon");
  const smoke = run(ctx.smokeBin, smokeArgs(ctx, old ? sidecar(old) : null, old?.info.version, sidecar(next)), { stream: true });
  steps.check(`daemon smoke against the packaged sidecar${old ? " (upgrade: old sidecar then new sidecar on one data directory)" : " (reinstall; NO upgrade)"}`, smoke.code === 0 ? [] : [`release_smoke exited ${smoke.code}`], ctx.smokeWork);
  return steps;
};

/** Windows: the NSIS installer in per-user mode. Written for a Windows machine, not run on this one. */
export const runWindows = (ctx) => {
  const steps = new Steps();
  const { run } = ctx;
  const installDir = join(ctx.work, "AgentOrchestrator");
  const authenticode = (path) => parseAuthenticode(run("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", AUTHENTICODE_SCRIPT], { env: { ARTIFACT: path } }).out);

  const prepared = {};
  for (const [label, dir] of [["new", ctx.artifactDir], ["old", ctx.oldArtifactDir]].filter(([, d]) => d)) {
    const { info, ok } = verifyDirectory(steps, label, dir, ctx);
    if (!ok) return steps;
    const installer = join(dir, info.artifacts[0].file);
    const facts = authenticode(installer);
    if (info.signed) steps.check(`${label}: installer Authenticode signature`, checkAuthenticode(facts), facts.subject);
    else steps.add(ctx.requireSigned && label === "new" ? "FAIL" : "INFO", `${label}: UNSIGNED installer (Authenticode status ${facts.status})`, "SmartScreen will warn on a download");
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

  if (prepared.old) install("old");
  const oldSidecar = prepared.old ? join(ctx.work, "old-orchestrator-daemon.exe") : null;
  if (oldSidecar) run("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", `Copy-Item -LiteralPath '${exe("orchestrator-daemon")}' -Destination '${oldSidecar}'`]);
  install("new");

  // The smoke needs both daemon binaries at once, so the old sidecar was copied out before the upgrade overwrote it.
  if (!ctx.smokeBin) steps.add("UNPROVEN", "daemon smoke against the installed sidecar", "no --smoke-bin was given");
  else {
    const smoke = run(ctx.smokeBin, smokeArgs(ctx, oldSidecar, prepared.old?.info.version, exe("orchestrator-daemon")), { stream: true });
    steps.check(`daemon smoke against the installed sidecar${oldSidecar ? " (upgrade)" : " (reinstall; NO upgrade)"}`, smoke.code === 0 ? [] : [`release_smoke exited ${smoke.code}`], ctx.smokeWork);
  }
  if (!prepared.old) steps.add("UNPROVEN", "upgrade from a previous release", "no --old-artifact-dir was given, so no older build ran");

  // NSIS copies its uninstaller aside and returns at once unless `_?=` pins it to the install directory.
  const uninstall = run(exe("uninstall"), ["/S", `_?=${installDir}`]);
  steps.check("silent uninstall", uninstall.code === 0 ? [] : [`uninstaller exited ${uninstall.code}`]);
  steps.check("uninstall removed the app and left the user's data directory", [
    ...(ctx.exists(exe("agent-orch-desktop")) ? ["agent-orch-desktop.exe is still installed"] : []),
    ...(ctx.exists(ctx.smokeWork) || !ctx.smokeBin ? [] : ["the smoke data directory was deleted"]),
  ]);
  steps.add("UNPROVEN", "SmartScreen reputation, UAC absence on a standard user, Windows Defender, and the GUI flow on a CLEAN Windows machine", "needs the clean-machine runbook");
  return steps;
};
