// DPES-1100 security gate: pinned Trivy and TruffleHog runner.
// PASS only when every scanner ran at its pinned version, exited cleanly, produced valid evidence, and found nothing.
// Exit codes: 0 PASS, 1 FAIL (findings or refused waiver/config), 2 ERROR (execution, provenance, evidence, controls).
import { spawnSync } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { runBounded } from "../ci/bounded.mjs";

export const PINS = {
  trivy: {
    version: "0.76.2",
    base: "https://github.com/aquasecurity/trivy/releases/download/v0.76.2/",
    bin: "trivy",
    archives: {
      "darwin-arm64": { file: "trivy_0.76.2_macOS-ARM64.tar.gz", sha256: "ae7ea358326960406b703c77b2ac490190797d80b306b5b50c2a27221da0a5b8" },
      "win32-x64": { file: "trivy_0.76.2_windows-64bit.zip", sha256: "691f737a7e5566a673c3a74dab9160552ec32101e953b659e5c77443d16dddb6" },
    },
  },
  trufflehog: {
    version: "3.99.9",
    base: "https://github.com/trufflesecurity/trufflehog/releases/download/v3.99.9/",
    bin: "trufflehog",
    archives: {
      "darwin-arm64": { file: "trufflehog_3.99.9_darwin_arm64.tar.gz", sha256: "1d85e5c9c7eddbf12ae2714eb89317ba2cbbbf59be4e161e4b2ff396adbaa3a8" },
      "win32-x64": { file: "trufflehog_3.99.9_windows_amd64.tar.gz", sha256: "da6754ea6f16be84fbda9860bdc49492edfa7a141eddc3ac900047fe88d16648" },
    },
  },
};

const SCHEMA = "dpes-1100-security-gate/1";
const REPORT = "security-gate-report.json";
const PLATFORM = `${process.platform}-${process.arch}`;
const EXE = process.platform === "win32" ? ".exe" : "";
const TIMEOUTS = { trivy: 1_200_000, trufflehog: 600_000 };
const EXIT = { PASS: 0, FAIL: 1, ERROR: 2 };
const MAX_FINDINGS = 200;
const WAIVER_FILES = new Set([
  ".trivyignore", ".trivyignore.yaml", ".trivyignore.yml", "trivy.yaml", "trivy.yml", "trivy-secret.yaml", "trivy-secret.yml",
]);
// Concatenated so this file never contains the inline-ignore markers it refuses.
const INLINE_MARKERS = ["trivy" + ":ignore", "trufflehog" + ":ignore"];
const VERSION_RE = { trivy: /^Version: (\S+)/m, trufflehog: /^trufflehog (\S+)/m };
// trivy fs reports "repository" for git worktrees and "filesystem" otherwise.
const TRIVY_ARTIFACT_TYPES = new Set(["filesystem", "repository"]);

export class GateError extends Error {}

const minimalEnv = () => {
  const env = { GIT_TERMINAL_PROMPT: "0" };
  for (const k of ["PATH", "HOME", "USERPROFILE", "SYSTEMROOT", "TEMP", "TMP", "TMPDIR", "LANG", "LC_ALL"]) {
    if (process.env[k] !== undefined) env[k] = process.env[k];
  }
  return env;
};

const git = (repo, args) => {
  const r = spawnSync("git", ["-C", repo, ...args], { encoding: "utf8", env: minimalEnv(), maxBuffer: 256 << 20 });
  if (r.error || r.status !== 0) throw new GateError(`git ${args[0]} failed`);
  return r.stdout;
};

export const checkVersion = (tool, output) => {
  const seen = VERSION_RE[tool].exec(output)?.[1];
  if (seen !== PINS[tool].version) throw new GateError(`${tool} version ${seen ?? "unparsed"} is not pinned ${PINS[tool].version}`);
  return seen;
};

const versionOf = (bin) => {
  const r = spawnSync(bin, ["--version"], { encoding: "utf8", env: minimalEnv(), timeout: 60_000 });
  if (r.error || r.status !== 0) throw new GateError("--version failed");
  return `${r.stdout}\n${r.stderr}`;
};

export const install = async (tool, dest) => {
  const pin = PINS[tool];
  const archive = pin.archives[PLATFORM];
  if (!archive) throw new GateError(`no ${tool} pin for ${PLATFORM}`);
  const res = await fetch(pin.base + archive.file, { signal: AbortSignal.timeout(300_000) });
  if (!res.ok) throw new GateError(`${tool} download failed: HTTP ${res.status}`);
  const bytes = Buffer.from(await res.arrayBuffer());
  if (createHash("sha256").update(bytes).digest("hex") !== archive.sha256) throw new GateError(`${tool} archive checksum mismatch`);
  const toolDir = join(dest, tool);
  rmSync(toolDir, { recursive: true, force: true });
  mkdirSync(toolDir, { recursive: true });
  const archivePath = join(dest, archive.file);
  writeFileSync(archivePath, bytes);
  // Windows: System32 bsdtar reads zip; a Git-for-Windows GNU tar earlier on PATH does not.
  const tar = process.platform === "win32" ? join(process.env.SYSTEMROOT ?? "C:\\Windows", "System32", "tar.exe") : "tar";
  const extracted = spawnSync(tar, ["-xf", archivePath, "-C", toolDir]);
  rmSync(archivePath, { force: true, maxRetries: 10, retryDelay: 200 });
  if (extracted.status !== 0) throw new GateError(`${tool} extract failed`);
  const bin = join(toolDir, pin.bin + EXE);
  if (!existsSync(bin)) throw new GateError(`${tool} binary missing after extract`);
  return { tool, version: checkVersion(tool, versionOf(bin)), archiveSha256: archive.sha256, binary: bin };
};

const scannerBin = (scanners, tool) => {
  const bin = join(scanners, tool, PINS[tool].bin + EXE);
  if (!existsSync(bin)) throw new GateError(`${tool} binary not installed`);
  return { bin, observed: checkVersion(tool, versionOf(bin)) };
};

export const refusalsFor = (repo) => {
  const refusals = git(repo, ["ls-files", "-z"])
    .split("\0")
    .filter((f) => WAIVER_FILES.has(basename(f)))
    .map((f) => `waiver or config file tracked: ${f}`);
  for (const marker of INLINE_MARKERS) {
    const r = spawnSync("git", ["-C", repo, "grep", "-I", "-l", "-F", "-e", marker, "--", "."], {
      encoding: "utf8", env: minimalEnv(), maxBuffer: 256 << 20,
    });
    if (r.status === 1) continue;
    if (r.error || r.status !== 0) throw new GateError("git grep failed");
    refusals.push(...r.stdout.split("\n").filter(Boolean).map((f) => `inline ignore marker present: ${f}`));
  }
  return refusals;
};

export const provenance = (repo, base, head) => {
  if (git(repo, ["status", "--porcelain=v1", "--ignored=traditional", "--untracked-files=all"]).trim() !== "") {
    throw new GateError("working tree is not clean");
  }
  const headSha = git(repo, ["rev-parse", "--verify", "--quiet", `${head}^{commit}`]).trim();
  const baseSha = git(repo, ["rev-parse", "--verify", "--quiet", `${base}^{commit}`]).trim();
  if (git(repo, ["rev-parse", "HEAD"]).trim() !== headSha) throw new GateError("HEAD is not the requested head");
  const mergeBase = git(repo, ["merge-base", baseSha, headSha]).trim();
  return { base: baseSha, head: headSha, mergeBase };
};

// Flags beyond the mandated command only harden (no repo config, ignore file, or secret config) or capture evidence.
export const trivyArgs = (cacheDir, outFile) => [
  "fs", "--scanners", "vuln,secret,misconfig", "--severity", "HIGH,CRITICAL", "--exit-code", "1",
  "--format", "json", "--output", outFile,
  "--config", "", "--secret-config", "", "--ignorefile", "",
  "--cache-dir", cacheDir, "--timeout", "15m", "--disable-telemetry", "--skip-version-check", "--no-progress",
  ".",
];

export const truffleHogArgs = (mergeBase, head) => [
  "git", "file://.", "--since-commit", mergeBase, "--branch", head,
  "--results=verified,unverified,unknown", "--no-verification", "--no-update", "--no-ignore-tag",
  "--fail", "--fail-on-scan-errors", "--json",
];

const parseJson = (text, what) => {
  try {
    return JSON.parse(text);
  } catch {
    throw new GateError(`${what} is not valid JSON`);
  }
};

// Keeps only identifying metadata. Trivy's Match/Code and TruffleHog's Raw/RawV2/Redacted are never copied.
export const curateTrivy = (report) => {
  if (typeof report.SchemaVersion !== "number" || !TRIVY_ARTIFACT_TYPES.has(report.ArtifactType) || report.ArtifactName !== ".") {
    throw new GateError("trivy evidence header mismatch");
  }
  return (report.Results ?? []).flatMap((result) =>
    [...(result.Vulnerabilities ?? []), ...(result.Secrets ?? []), ...(result.Misconfigurations ?? [])].map((item) => ({
      target: result.Target ?? null,
      id: item.VulnerabilityID ?? item.RuleID ?? item.ID ?? null,
      severity: item.Severity ?? null,
      line: item.StartLine ?? item.CauseMetadata?.StartLine ?? null,
    })));
};

export const curateTruffleHog = (finding) => {
  const data = finding.SourceMetadata?.Data ?? {};
  const meta = data.git ?? data.Git ?? {};
  return {
    detector: finding.DetectorName ?? null,
    verified: finding.Verified === true,
    file: meta.file ?? null,
    line: meta.line ?? null,
    commit: meta.commit ?? null,
  };
};

const exitCodeOf = (run) => {
  if (run.timedOut || run.signal || run.spawnError || run.pipesAbandoned || run.code === null) {
    throw new GateError("scanner did not exit normally");
  }
  return run.code;
};

// WI-DPES-1100: an analysis error is not clean evidence, even when a scanner exits zero.
const rejectErrorDiagnostics = (run, scanner) => {
  const failed = (run.stderr ?? "").split("\n").some((line) => {
    try {
      const diagnostic = JSON.parse(line);
      return ["error", "fatal", "panic"].includes(String(diagnostic.level ?? "").toLowerCase());
    } catch {
      return /(?:^|\s|\[)(?:ERROR|FATAL)(?:\]|\s|:)/.test(line);
    }
  });
  if (failed) throw new GateError(`${scanner} reported an execution or analysis error`);
};

export const scanTrivy = (run, outFile) => {
  const code = exitCodeOf(run);
  rejectErrorDiagnostics(run, "trivy");
  if (code !== 0 && code !== 1) throw new GateError(`trivy exit ${code}`);
  const findings = curateTrivy(parseJson(readFileSync(outFile, "utf8"), "trivy evidence"));
  if ((code === 0) !== (findings.length === 0)) throw new GateError("trivy exit code contradicts evidence");
  return { status: code === 0 ? "PASS" : "FAIL", findings };
};

export const scanTruffleHog = (run) => {
  const code = exitCodeOf(run);
  rejectErrorDiagnostics(run, "trufflehog");
  if (code !== 0 && code !== 183) throw new GateError(`trufflehog exit ${code}`);
  const findings = run.stdout
    .split("\n")
    .filter((line) => line.trim() !== "")
    .map((line) => curateTruffleHog(parseJson(line, "trufflehog output")));
  if ((code === 0) !== (findings.length === 0)) throw new GateError("trufflehog exit code contradicts output");
  return { status: code === 0 ? "PASS" : "FAIL", findings };
};

const runTool = (exe, commandArgs, cwd, timeoutMs, workDir, name) =>
  runBounded({
    exe, commandArgs, cwd, env: minimalEnv(), timeoutMs, echo: false,
    stdoutFile: join(workDir, `${name}.out`), stderrFile: join(workDir, `${name}.err`),
  });

const runChecked = async (name, tool, args, cwd, workDir, timeoutMs, classify) => {
  const header = {
    name,
    version: PINS[name].version,
    observedVersion: tool.observed,
    argv: args.map((a) => a.split(workDir).join("<work>")),
  };
  let run;
  try {
    run = await runTool(tool.bin, args, cwd, timeoutMs, workDir, name);
    return { ...header, exitCode: run.code, signal: run.signal, timedOut: run.timedOut, durationMs: run.durationMs, ...classify(run) };
  } catch (e) {
    return {
      ...header,
      exitCode: run?.code ?? null,
      status: "ERROR",
      reason: e instanceof GateError ? e.message : "internal error",
      findings: [],
    };
  }
};

const verdict = (parts) => {
  if (parts.some((p) => p.status === "ERROR")) return "ERROR";
  return parts.some((p) => p.status === "FAIL") ? "FAIL" : "PASS";
};

export const runScan = async ({ repo, base, head, scanners, evidenceDir, timeouts = TIMEOUTS }) => {
  const top = git(repo, ["rev-parse", "--show-toplevel"]).trim();
  const refusals = refusalsFor(top);
  const prov = provenance(top, base, head);
  const trivyTool = scannerBin(scanners, "trivy");
  const truffleTool = scannerBin(scanners, "trufflehog");
  const workDir = mkdtempSync(join(tmpdir(), "dpes1100-gate-"));
  const parts = [];
  try {
    if (refusals.length === 0) {
      const trivyOut = join(workDir, "trivy.json");
      parts.push(await runChecked("trivy", trivyTool, trivyArgs(join(workDir, "trivy-cache"), trivyOut), top, workDir,
        timeouts.trivy, (run) => scanTrivy(run, trivyOut)));
      parts.push(await runChecked("trufflehog", truffleTool, truffleHogArgs(prov.mergeBase, prov.head), top, workDir,
        timeouts.trufflehog, scanTruffleHog));
    }
  } finally {
    rmSync(workDir, { recursive: true, force: true, maxRetries: 5 });
  }
  const status = verdict(refusals.length > 0 ? [...parts, { status: "FAIL" }] : parts);
  const report = {
    schema: SCHEMA,
    status,
    exitCode: EXIT[status],
    base: prov.base,
    head: prov.head,
    mergeBase: prov.mergeBase,
    pins: { trivy: PINS.trivy.version, trufflehog: PINS.trufflehog.version },
    refusals,
    waivers: "none: strict default, no waiver mechanism enabled",
    scanners: parts.map((p) => ({ ...p, findings: p.findings.slice(0, MAX_FINDINGS), findingCount: p.findings.length })),
  };
  mkdirSync(evidenceDir, { recursive: true });
  writeFileSync(join(evidenceDir, REPORT), `${JSON.stringify(report, null, 2)}\n`);
  return report;
};

const commitFiles = (repo, files, init) => {
  if (init) {
    mkdirSync(repo, { recursive: true });
    git(repo, ["init", "-q", "-b", "main"]);
  }
  for (const [path, content] of Object.entries(files)) {
    mkdirSync(join(repo, dirname(path)), { recursive: true });
    writeFileSync(join(repo, path), content);
  }
  git(repo, ["add", "-A"]);
  git(repo, ["-c", "user.name=gate-control", "-c", "user.email=gate-control@example.invalid",
    "-c", "commit.gpgsign=false", "commit", "-q", "-m", "control"]);
  return git(repo, ["rev-parse", "HEAD"]).trim();
};

// Real-scanner controls. The seeded token is synthetic, generated per run, never printed, never verified online.
export const runControls = async ({ scanners, evidenceDir }) => {
  const token = `ghp_${randomBytes(18).toString("hex")}`;
  const root = mkdtempSync(join(tmpdir(), "dpes1100-controls-"));
  const cases = [
    { name: "clean", files: { Dockerfile: 'FROM alpine:3.20\nUSER app\nCMD ["sh"]\n' },
      expect: { trivy: "PASS", trufflehog: "PASS" } },
    { name: "iac-misconfig", files: { Dockerfile: 'FROM alpine:3.20\nUSER root\nCMD ["sh"]\n' },
      expect: { trivy: "FAIL", trufflehog: "PASS" } },
    { name: "parser-error", files: { Dockerfile: 'FROM alpine:3.20\nUSER app\nCMD ["sh"]\n', "Dockerfile.dockerignore": "**\n" },
      expect: { trivy: "ERROR", trufflehog: "PASS" } },
    { name: "seeded-secret", files: { "README.md": "seed\n" }, second: { "config/seed.env": `SEED_TOKEN=${token}\n` },
      expect: { trivy: "FAIL", trufflehog: "FAIL" } },
  ];
  const results = [];
  try {
    for (const c of cases) {
      const repo = join(root, c.name);
      const base = commitFiles(repo, c.files, true);
      const head = c.second ? commitFiles(repo, c.second, false) : base;
      const outDir = join(evidenceDir, "controls", c.name);
      await runScan({ repo, base, head, scanners, evidenceDir: outDir });
      const text = readFileSync(join(outDir, REPORT), "utf8");
      const actual = {};
      for (const s of JSON.parse(text).scanners) actual[s.name] = s.status;
      const leaked = text.includes(token);
      results.push({
        case: c.name,
        expected: c.expect,
        actual,
        secretInEvidence: leaked,
        ok: !leaked && c.expect.trivy === actual.trivy && c.expect.trufflehog === actual.trufflehog,
      });
    }
  } finally {
    rmSync(root, { recursive: true, force: true, maxRetries: 5 });
  }
  mkdirSync(evidenceDir, { recursive: true });
  writeFileSync(join(evidenceDir, "controls.json"), `${JSON.stringify({ results }, null, 2)}\n`);
  return results.every((r) => r.ok) ? EXIT.PASS : EXIT.ERROR;
};

const flags = (argv) => {
  const out = {};
  for (let i = 0; i < argv.length; i += 2) {
    if (!argv[i]?.startsWith("--") || argv[i + 1] === undefined) throw new GateError("malformed arguments");
    out[argv[i].slice(2)] = argv[i + 1];
  }
  return out;
};

const requireFlag = (o, key) => {
  if (!o[key]) throw new GateError(`missing --${key}`);
  return o[key];
};

export const main = async (argv) => {
  const [cmd, ...rest] = argv;
  const o = flags(rest);
  if (cmd === "install") {
    const dest = resolve(requireFlag(o, "dest"));
    mkdirSync(dest, { recursive: true });
    for (const tool of ["trivy", "trufflehog"]) console.log(JSON.stringify(await install(tool, dest)));
    return EXIT.PASS;
  }
  if (cmd !== "scan" && cmd !== "controls") throw new GateError("usage: install | scan | controls");
  const scanners = resolve(requireFlag(o, "scanners"));
  const evidenceDir = resolve(requireFlag(o, "evidence-dir"));
  if (cmd === "controls") return runControls({ scanners, evidenceDir });
  let report;
  try {
    report = await runScan({
      repo: resolve(o.repo ?? "."), base: requireFlag(o, "base"), head: requireFlag(o, "head"), scanners, evidenceDir,
    });
  } catch (e) {
    report = { schema: SCHEMA, status: "ERROR", exitCode: EXIT.ERROR, reason: e instanceof GateError ? e.message : "internal error" };
    mkdirSync(evidenceDir, { recursive: true });
    writeFileSync(join(evidenceDir, REPORT), `${JSON.stringify(report, null, 2)}\n`);
  }
  const lines = (report.scanners ?? []).map((s) => `${s.name}=${s.status}(findings=${s.findings?.length ?? 0})`);
  console.log(`security gate ${report.status}${lines.length ? ` ${lines.join(" ")}` : ""}`);
  return EXIT[report.status];
};

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main(process.argv.slice(2)).then(
    (code) => { process.exitCode = code; },
    (e) => {
      console.error(`security gate error: ${e instanceof GateError ? e.message : "internal error"}`);
      process.exitCode = EXIT.ERROR;
    },
  );
}
