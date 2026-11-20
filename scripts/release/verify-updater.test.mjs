import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";
import { updaterManifest } from "./publish.mjs";
import { verifyUpdater } from "./verify-updater.mjs";

const scratch = mkdtempSync(join(tmpdir(), "verify-updater-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));
const b64 = (text) => Buffer.from(text).toString("base64");
let count = 0;
const fresh = (name) => {
  const dir = join(scratch, `${name}-${count += 1}`);
  mkdirSync(dir, { recursive: true });
  return dir;
};

// Stand-in for the minisign binary, for hosts without it. It checks what verifyUpdater is responsible for passing:
// the decoded key and signature files and the payload. Signature file: "<key id>|<payload sha256>|<version>".
const FAKE = `
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
const arg = (flag) => process.argv[process.argv.indexOf(flag) + 1];
if (!["-V", "-Q"].every((flag) => process.argv.includes(flag))) process.exit(2);
const keyId = readFileSync(arg("-p"), "utf8").split("\\n")[1];
const [id, digest, version] = readFileSync(arg("-x"), "utf8").split("\\n")[1].split("|");
const actual = createHash("sha256").update(readFileSync(arg("-m"))).digest("hex");
if (id !== keyId || digest !== actual) { console.error("Signature verification failed"); process.exit(1); }
process.stdout.write("timestamp:1\\tfile:payload\\tversion:" + version + "\\n");
`;
const fakeScript = join(scratch, "fake-minisign.mjs");
writeFileSync(fakeScript, FAKE);

const fakeSigner = {
  command: [process.execPath, fakeScript],
  key: (id) => ({
    publicKey: b64(`untrusted comment: fake public key\n${id}\n`),
    sign: (payload, version) => {
      const digest = createHash("sha256").update(readFileSync(payload)).digest("hex");
      return b64(`untrusted comment: fake signature\n${id}|${digest}|${version}\n`);
    },
  }),
};

const minisignPresent = spawnSync("minisign", ["-v"], { encoding: "utf8" }).error === undefined;
const realSigner = {
  command: ["minisign"],
  key: (id) => {
    const dir = fresh(`key-${id}`);
    const run = (args) => {
      const result = spawnSync("minisign", args, { encoding: "utf8" });
      assert.equal(result.status, 0, result.stderr);
    };
    run(["-G", "-W", "-f", "-p", join(dir, "k.pub"), "-s", join(dir, "k.key")]);
    return {
      publicKey: b64(readFileSync(join(dir, "k.pub"), "utf8")),
      sign: (payload, version) => {
        run(["-S", "-s", join(dir, "k.key"), "-m", payload, "-x", `${payload}.minisig`, "-t", `timestamp:1\tfile:payload\tversion:${version}`]);
        return b64(readFileSync(`${payload}.minisig`, "utf8"));
      },
    };
  },
};

const MAC = "MCAO-0.1.0-macos-arm64-unsigned.app.tar.gz";
const WIN = "MCAO-0.1.0-windows-x64-unsigned-setup.exe";

// An assets directory whose payloads are signed with `key` as version `signedVersion`; `edit` may then break it.
function stage(signer, { key = signer.key("a"), signedVersion = "0.1.0", edit = () => {} } = {}) {
  const dir = fresh("assets");
  writeFileSync(join(dir, MAC), "mac payload bytes");
  writeFileSync(join(dir, WIN), "win payload bytes");
  const sigs = { [MAC]: key.sign(join(dir, MAC), signedVersion), [WIN]: key.sign(join(dir, WIN), signedVersion) };
  const write = () => {
    for (const [file, sig] of Object.entries(sigs)) writeFileSync(join(dir, `${file}.sig`), `${sig}\n`);
    const manifest = updaterManifest("0.1.0", [
      { platformKey: "darwin-aarch64", file: MAC, signature: sigs[MAC] },
      { platformKey: "windows-x86_64", file: WIN, signature: sigs[WIN] },
    ]);
    writeFileSync(join(dir, "latest.json"), JSON.stringify(manifest));
  };
  write();
  edit({ dir, sigs, write });
  return dir;
}

function scenarios(signer, label, skip) {
  test(`verifyUpdater accepts correctly signed payloads (${label})`, { skip: skip || false }, () => {
    const key = signer.key("a");
    assert.equal(verifyUpdater(stage(signer, { key }), key.publicKey, signer.command), "0.1.0");
  });

  test(`verifyUpdater rejects each kind of bad input (${label})`, { skip: skip || false }, () => {
    const key = signer.key("a");
    const other = signer.key("b");
    const check = (dir, publicKey, message, name) => assert.throws(() => verifyUpdater(dir, publicKey, signer.command), message, name);
    check(stage(signer, { key, edit: ({ dir }) => writeFileSync(join(dir, MAC), "mac payload byteZ") }), key.publicKey, /failed signature verification/, "a payload byte changed");
    check(stage(signer, { key, edit: ({ sigs, write }) => { sigs[MAC] = sigs[WIN]; write(); } }), key.publicKey, /failed signature verification/, "the other platform's signature");
    check(stage(signer, { key: other }), key.publicKey, /failed signature verification/, "signed with another key");
    check(stage(signer, { key, signedVersion: "0.1.1" }), key.publicKey, /signed for another version than 0\.1\.0/, "another version in the trusted comment");
    check(stage(signer, { key, edit: ({ dir }) => writeFileSync(join(dir, `${MAC}.sig`), "other\n") }), key.publicKey, /\.sig differs from the darwin-aarch64 signature/, "a .sig that differs from the manifest");
  });
}

// What fails before minisign runs needs no binary.
test("verifyUpdater refuses a malformed manifest, key or asset set before verifying any signature", () => {
  const key = fakeSigner.key("a");
  const check = (dir, publicKey, message, name) => assert.throws(() => verifyUpdater(dir, publicKey, fakeSigner.command), message, name);
  const manifestEdit = (change) => ({ dir }) => {
    const manifest = JSON.parse(readFileSync(join(dir, "latest.json"), "utf8"));
    change(manifest, dir);
    writeFileSync(join(dir, "latest.json"), JSON.stringify(manifest));
  };
  check(stage(fakeSigner, { key, edit: ({ dir }) => rmSync(join(dir, "latest.json")) }), key.publicKey, /latest\.json is missing/, "no manifest");
  check(stage(fakeSigner, { key, edit: ({ dir }) => writeFileSync(join(dir, "latest.json"), "{") }), key.publicKey, /not valid JSON/, "bad JSON");
  check(stage(fakeSigner, { key, edit: manifestEdit((m) => { m.version = "v0.1.0"; }) }), key.publicKey, /MAJOR\.MINOR\.PATCH/, "a v-prefixed version");
  check(stage(fakeSigner, { key, edit: manifestEdit((m) => { m.platforms["linux-x86_64"] = m.platforms["darwin-aarch64"]; }) }), key.publicKey, /platforms must be exactly/, "an extra platform");
  check(stage(fakeSigner, { key, edit: manifestEdit((m) => { m.platforms["darwin-aarch64"].url = `https://example.com/${MAC}`; }) }), key.publicKey, /url is not https:\/\/github\.com\/mc-innersource\/motif-mcao\/releases\/download\/v0\.1\.0\//, "a foreign URL");
  check(stage(fakeSigner, { key, edit: manifestEdit((m) => { m.platforms["darwin-aarch64"].url = m.platforms["darwin-aarch64"].url.replace(MAC, `../${MAC}`); }) }), key.publicKey, /url is not/, "a path escape");
  check(stage(fakeSigner, { key, edit: ({ dir }) => rmSync(join(dir, `${WIN}.sig`)) }), key.publicKey, /setup\.exe\.sig is missing/, "a missing .sig");
  check(stage(fakeSigner, { key, edit: ({ dir }) => rmSync(join(dir, MAC)) }), key.publicKey, /payload .* is missing/, "a missing payload");
  check(stage(fakeSigner, { key }), "not base64!", /public key is not base64/, "a key that is not base64");
  check(stage(fakeSigner, { key }), b64("hello"), /does not decode to a minisign file/, "a key that is not a minisign file");
});

scenarios(fakeSigner, "fake minisign", false);
scenarios(realSigner, "real minisign", minisignPresent ? false : "the minisign binary is not on PATH; the fake-minisign cases ran instead");
