import assert from "node:assert/strict";
import { test } from "node:test";
import { findPrivate, redactText } from "./redact.mjs";

const identity = { user: "alice", domain: "CORPDOM", host: "WS-0042", dnsDomain: "corp.example.edu", profile: "C:\\Users\\alice" };

test("the domain account, profile, host, DNS domain, user and e-mail are all replaced", () => {
  const text = [
    "C:\\ao\\p12\\acl-data CORPDOM\\alice:(OI)(CI)(F)",
    "C:\\Users\\alice\\.cargo and alice@example.edu on WS-0042.corp.example.edu",
    "owner CorpDom\\Alice, machine ws-0042",
  ].join("\r\n");
  const out = redactText(text, identity);
  assert.equal(out, [
    "C:\\ao\\p12\\acl-data <user>:(OI)(CI)(F)",
    "<profile>\\.cargo and <email> on <host>.<dnsdomain>",
    "owner <user>, machine <host>",
  ].join("\r\n"));
  assert.deepEqual(findPrivate(out, { identity }), []);
});

test("a domain in front of an account is removed even when the environment does not name it", () => {
  assert.equal(redactText("OTHERDOM\\alice:(F)", { ...identity, domain: "" }), "<user>:(F)");
});

test("names are matched whole-word, and short names are left alone", () => {
  assert.equal(redactText("alicewonder malice alice", identity), "alicewonder malice <user>");
  assert.equal(redactText("al is here", { ...identity, user: "al" }), "al is here");
  assert.equal(redactText("BUILTIN\\Users NT AUTHORITY\\SYSTEM C:\\Windows\\Temp", identity), "BUILTIN\\Users NT AUTHORITY\\SYSTEM C:\\Windows\\Temp");
});

test("findPrivate reports the line and kind, never the value", () => {
  const text = "fine\nmail bob@example.edu\nCORPDOM\\carol\nhost WS-0042 user alice\nBUILTIN\\Administrators\n";
  const found = findPrivate(text, { identity });
  assert.deepEqual(found.map((f) => `${f.line}:${f.kind}`).sort(), ["2:email", "3:domain", "3:domain-account", "4:host", "4:user"]);
  assert.ok(!JSON.stringify(found).match(/bob|carol|alice|WS-0042|CORPDOM/));
});

test("raw tool output may name the profile but not the rest of the identity", () => {
  assert.deepEqual(findPrivate("error in C:\\Users\\alice\\.cargo\\x.rs", { identity, includeUser: false }), []);
  assert.deepEqual(findPrivate("on WS-0042", { identity, includeUser: false }).map((f) => f.kind), ["host"]);
});
