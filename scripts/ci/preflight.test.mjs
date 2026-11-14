import assert from "node:assert/strict";
import { test } from "node:test";
import { win32 } from "node:path";
import { MAX_ROOT_LENGTH, checkPreflight } from "./preflight.mjs";

// A fake Windows file system: `real` maps the upper-cased given path to its real path (an alias is a path whose
// real path differs); everything listed in `exists` exists.
const fs = ({ real = {}, exists = [] }) => ({
  realpath: (p) => {
    const key = win32.resolve(p).toUpperCase();
    if (key in real) return real[key];
    return win32.resolve(p);
  },
  exists: (p) => {
    const key = win32.resolve(p).toUpperCase();
    return key in real || exists.map((e) => e.toUpperCase()).includes(key);
  },
});
const check = (roots, fake = {}) => checkPreflight({ roots, platform: "win32", ...fs(fake) });

test("a real short directory passes, existing or not yet created", () => {
  assert.deepEqual(check({ source: "C:\\t\\ab12cd34\\src" }, { exists: ["C:\\", "C:\\t", "C:\\t\\ab12cd34", "C:\\t\\ab12cd34\\src"] }), []);
  assert.deepEqual(check({ evidence: "C:\\t\\ab12cd34\\evidence" }, { exists: ["C:\\", "C:\\t", "C:\\t\\ab12cd34"] }), []);
});

test("a SUBST or mapped drive fails: the drive letter differs from the real path", () => {
  const problems = check(
    { source: "Z:\\run\\src" },
    { real: { "Z:\\": "C:\\Users\\someone\\work", "Z:\\RUN": "C:\\Users\\someone\\work\\run", "Z:\\RUN\\SRC": "C:\\Users\\someone\\work\\run\\src" } },
  );
  assert.equal(problems.length, 1);
  assert.match(problems[0], /source Z:\\run\\src is really C:\\Users\\someone\\work\\run\\src: a SUBST drive/);
});

test("a junction fails: the same drive, a different path", () => {
  const problems = check(
    { target: "C:\\t\\x\\target" },
    { real: { "C:\\T\\X": "D:\\elsewhere\\x", "C:\\T\\X\\TARGET": "D:\\elsewhere\\x\\target" } },
  );
  assert.match(problems[0], /is really D:\\elsewhere\\x\\target/);
});

test("a junction in the nearest existing ancestor is found for a path that does not exist yet", () => {
  const problems = check(
    { evidence: "C:\\t\\x\\evidence" },
    { real: { "C:\\T\\X": "D:\\elsewhere\\x" }, exists: ["C:\\", "C:\\t"] },
  );
  assert.match(problems[0], /evidence C:\\t\\x\\evidence is really D:\\elsewhere\\x\\evidence/);
});

test("letter case and an 8.3 short name are the same directory, not an alias", () => {
  assert.deepEqual(check({ tmp: "c:\\USERS\\RUNNER~1\\AppData\\tt" }, {
    real: { "C:\\USERS\\RUNNER~1\\APPDATA\\TT": "C:\\Users\\runneradmin\\AppData\\tt" },
  }), []);
  assert.deepEqual(check({ tmp: "c:\\t\\ab" }, { real: { "C:\\T\\AB": "C:\\T\\AB" } }), []);
});

test("a network or device path fails", () => {
  assert.match(check({ source: "\\\\server\\share\\src" })[0], /source is a network or device path/);
});

test("a path longer than the limit fails, one at the limit does not", () => {
  const at = `C:\\${"a".repeat(MAX_ROOT_LENGTH - 3)}`;
  assert.equal(at.length, MAX_ROOT_LENGTH);
  assert.deepEqual(check({ source: at }), []);
  const problems = check({ source: `${at}b` });
  assert.equal(problems.length, 1);
  assert.match(problems[0], new RegExp(`source path is ${MAX_ROOT_LENGTH + 1} characters \\(limit ${MAX_ROOT_LENGTH}\\)`));
});

test("every bad root is reported, each once per problem", () => {
  const problems = check({ source: "\\\\h\\s", target: `C:\\${"b".repeat(80)}` });
  assert.equal(problems.length, 2);
});

test("nothing is checked off Windows", () => {
  assert.deepEqual(checkPreflight({ roots: { source: "/a".repeat(200) }, platform: "darwin" }), []);
  assert.deepEqual(checkPreflight({ roots: { source: "/a" }, platform: "linux" }), []);
});

test("a path that cannot be resolved is reported", () => {
  const problems = checkPreflight({
    roots: { source: "C:\\t\\x" }, platform: "win32", exists: () => true, realpath: () => { throw new Error("access denied"); },
  });
  assert.match(problems[0], /could not be resolved: access denied/);
});
