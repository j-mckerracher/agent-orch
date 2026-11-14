// Release packaging for one platform, parameterized by secrets.
//
//   node scripts/release/package.mjs <macos|windows> --out DIR
//
// Environment: CARGO_TARGET_DIR (required, absolute, outside the checkout), signing secrets named
// in signing.mjs, REQUIRE_SIGNING=true to fail instead of skipping when none are set (a `v*` tag
// build requires signing unless ALLOW_UNSIGNED_RELEASE=true), WINDOWS_TIMESTAMP_URL (optional,
// not secret). Runs the same `npm run package:mac|win` as the CI gate, then collects the
// artifact, a SHA256SUMS file and release-info.json into --out.
// Secrets are never logged. The Windows certificate and password are removed from this process's
// environment before anything is spawned and reach only the PowerShell step that imports the
// certificate; the build child gets the certificate thumbprint through TAURI_CONFIG instead.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  appendFileSync, copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { buildChildEnv, resolveTargetDir, takeWindowsSecrets } from "./env.mjs";
import { signingDecision } from "./signing.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "..", "..");
const desktop = join(repo, "apps", "desktop");
const win = process.platform === "win32";

const [platform, flag, outArg] = process.argv.slice(2);
if (!["macos", "windows"].includes(platform) || flag !== "--out" || !outArg) {
  console.error("usage: node scripts/release/package.mjs <macos|windows> --out DIR");
  process.exit(2);
}
if ((platform === "macos") !== (process.platform === "darwin") || (platform === "windows") !== win) {
  console.error(`::error::cannot package ${platform} on ${process.platform}`);
  process.exit(2);
}
let targetDir;
try {
  targetDir = resolveTargetDir(process.env.CARGO_TARGET_DIR);
} catch (error) {
  console.error(`::error::${error.message}`);
  process.exit(2);
}
const out = resolve(outArg);
if (existsSync(out)) {
  console.error(`::error::output directory exists, refusing to reuse it: ${out}`);
  process.exit(2);
}

const decision = signingDecision(platform, process.env);
if (decision.mode === "error") {
  console.error(`::error title=Signing configuration::${decision.message}`);
  process.exit(1);
}
console.log(`::${decision.mode === "signed" ? "notice" : "warning"} title=Signing ${platform}::${decision.message}`);

const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, { stdio: "inherit", shell: win && /^np[mx]$/.test(command), ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} exited ${result.status}`);
};
// `extraEnv` is the only place a secret may be passed, and only for the one call that needs it.
const powershell = (script, extraEnv) => {
  const result = spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], {
    encoding: "utf8", env: { ...childEnv, ...extraEnv },
  });
  if (result.status !== 0) throw new Error(`powershell failed: ${result.stderr.trim()}`);
  return result.stdout.trim();
};

// From here on nothing this process spawns inherits the Windows certificate or password.
let windowsSecrets = platform === "windows" ? takeWindowsSecrets(process.env) : {};
const childEnv = { ...buildChildEnv(platform, process.env, decision), CARGO_TARGET_DIR: targetDir };

const version = JSON.parse(readFileSync(join(desktop, "src-tauri", "tauri.conf.json"), "utf8")).version;
if (process.env.GITHUB_REF_TYPE === "tag" && process.env.GITHUB_REF_NAME !== `v${version}`) {
  console.error(`::error::tag ${process.env.GITHUB_REF_NAME} does not match app version v${version}`);
  process.exit(1);
}
const sha256 = (file) => createHash("sha256").update(readFileSync(file)).digest("hex");
mkdirSync(out, { recursive: true });
const label = decision.sign ? "signed" : "unsigned";
const artifacts = [];
let certThumbprint = null;

try {
  if (platform === "windows" && decision.sign) {
    const pfx = join(targetDir, "release-signing.pfx");
    mkdirSync(targetDir, { recursive: true });
    writeFileSync(pfx, Buffer.from(windowsSecrets.WINDOWS_CERTIFICATE, "base64"), { mode: 0o600 });
    try {
      certThumbprint = powershell(
        "$pw = ConvertTo-SecureString $env:WINDOWS_CERTIFICATE_PASSWORD -AsPlainText -Force; " +
          "(Import-PfxCertificate -FilePath $env:PFX_PATH -CertStoreLocation Cert:\\CurrentUser\\My -Password $pw).Thumbprint",
        { PFX_PATH: pfx, WINDOWS_CERTIFICATE_PASSWORD: windowsSecrets.WINDOWS_CERTIFICATE_PASSWORD },
      );
    } finally {
      rmSync(pfx, { force: true });
      windowsSecrets = {};
    }
    if (!/^[0-9A-Fa-f]{40}$/.test(certThumbprint)) throw new Error("certificate import did not return a thumbprint");
    // The Tauri CLI merges TAURI_CONFIG into the config; the thumbprint is not a secret.
    childEnv.TAURI_CONFIG = JSON.stringify({
      bundle: { windows: {
        certificateThumbprint: certThumbprint,
        digestAlgorithm: "sha256",
        timestampUrl: process.env.WINDOWS_TIMESTAMP_URL || "http://timestamp.digicert.com",
      } },
    });
  }

  run("npm", ["run", platform === "macos" ? "package:mac" : "package:win"], { cwd: desktop, env: childEnv });

  if (platform === "macos") {
    const app = join(targetDir, "release", "bundle", "macos", "Agent Orchestrator.app");
    if (!existsSync(app)) throw new Error(`bundle missing: ${app}`);
    if (decision.sign) {
      run("codesign", ["--verify", "--deep", "--strict", "--verbose=2", app]);
      if (decision.notarize) {
        run("xcrun", ["stapler", "validate", app]);
        run("spctl", ["--assess", "--type", "execute", "--verbose=2", app]);
      }
    }
    const zip = join(out, `Agent-Orchestrator-${version}-macos-${process.arch}-${label}.zip`);
    run("ditto", ["-c", "-k", "--keepParent", app, zip]);
    artifacts.push(zip);
  } else {
    const installer = join(targetDir, "release", "bundle", "nsis", `Agent Orchestrator_${version}_x64-setup.exe`);
    if (!existsSync(installer)) throw new Error(`installer missing: ${installer}`);
    if (decision.sign) {
      const status = powershell("(Get-AuthenticodeSignature -LiteralPath $env:ARTIFACT).Status", { ARTIFACT: installer });
      if (status !== "Valid") throw new Error(`installer Authenticode status is ${status}, expected Valid`);
    }
    const copy = join(out, `Agent-Orchestrator-${version}-windows-x64-${label}-setup.exe`);
    copyFileSync(installer, copy);
    artifacts.push(copy);
  }
} finally {
  if (certThumbprint && /^[0-9A-Fa-f]{40}$/.test(certThumbprint)) {
    powershell("Remove-Item -LiteralPath (Join-Path Cert:\\CurrentUser\\My $env:THUMB) -ErrorAction SilentlyContinue", { THUMB: certThumbprint });
  }
}

const git = spawnSync("git", ["rev-parse", "HEAD"], { cwd: repo, encoding: "utf8" });
const info = {
  version, platform, signed: decision.sign, notarized: decision.notarize,
  commit: process.env.GITHUB_SHA || git.stdout.trim(),
  artifacts: artifacts.map((a) => ({ file: a.slice(out.length + 1), sha256: sha256(a) })),
};
writeFileSync(join(out, "release-info.json"), `${JSON.stringify(info, null, 2)}\n`);
writeFileSync(join(out, "SHA256SUMS"), info.artifacts.map((a) => `${a.sha256}  ${a.file}`).join("\n") + "\n");
if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `signed=${decision.sign}\nnotarized=${decision.notarize}\n`);
if (process.env.GITHUB_STEP_SUMMARY) {
  appendFileSync(process.env.GITHUB_STEP_SUMMARY,
    `### Release build: ${platform}\n\n- ${decision.message}\n${info.artifacts.map((a) => `- \`${a.file}\` sha256 \`${a.sha256}\``).join("\n")}\n`);
}
console.log(JSON.stringify(info, null, 2));
