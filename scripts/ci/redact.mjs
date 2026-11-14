// Keeps who and where a run happened out of evidence that is later committed: the account name, the
// Windows domain (`DOMAIN\user`), the machine name, the DNS domain, the profile path and any e-mail address.
//
// `redactText` replaces them in text the harness writes itself. `findPrivate` reads text and reports
// where something private remains as file-independent `{ line, kind }` records. It NEVER returns the
// matched value, so a report of a leak is not itself a leak.
//
// Names are matched whole-word, case-insensitively, and only when they are at least MIN_NAME characters
// long, so a two-letter account name cannot corrupt unrelated words. The same rules are in
// p12-windows-checks.ps1 (Protect-Text).
import os from "node:os";

const MIN_NAME = 3;
const EMAIL = /[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/g;
// An upper-case NetBIOS-style domain, a backslash, an account. The look-behind keeps path segments such as
// `C:\ao\P12\x` out. Well-known Windows authorities are not an operator's domain.
const DOMAIN_ACCOUNT = /(?<![A-Za-z0-9._\\/:-])([A-Z][A-Z0-9._-]{2,14})\\[A-Za-z0-9$._-]+/g;
const WELL_KNOWN = new Set(["BUILTIN", "AUTHORITY", "SERVICE", "MANAGER", "DRIVER", "HOST", "SYSTEM"]);

const escapeRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const wholeWord = (name) => new RegExp(`(?<![A-Za-z0-9])${escapeRe(name)}(?![A-Za-z0-9])`, "gi");

export const identityFromEnv = (env = process.env) => {
  const win = process.platform === "win32";
  const host = (env.COMPUTERNAME ?? os.hostname()).split(".")[0];
  let user = env.USERNAME ?? env.USER ?? "";
  let profile = env.USERPROFILE ?? "";
  try {
    user ||= os.userInfo().username;
    profile ||= os.homedir();
  } catch { /* an account without a passwd entry */ }
  return {
    user,
    domain: win ? (env.USERDOMAIN ?? "") : "",
    host,
    dnsDomain: env.USERDNSDOMAIN ?? "",
    profile,
  };
};

const usable = (s) => typeof s === "string" && s.length >= MIN_NAME;

export const redactText = (text, identity = identityFromEnv()) => {
  const { user, domain, host, dnsDomain, profile } = identity;
  const steps = [];
  if (usable(profile)) steps.push([new RegExp(escapeRe(profile), "gi"), "<profile>"]);
  if (usable(domain) && usable(user)) steps.push([new RegExp(`${escapeRe(domain)}\\\\${escapeRe(user)}`, "gi"), "<user>"]);
  if (usable(dnsDomain)) steps.push([wholeWord(dnsDomain), "<dnsdomain>"]);
  if (usable(domain)) steps.push([wholeWord(domain), "<domain>"]);
  if (usable(host)) steps.push([wholeWord(host), "<host>"]);
  if (usable(user)) steps.push([wholeWord(user), "<user>"]);
  let out = String(text).replace(EMAIL, "<email>");
  for (const [pattern, replacement] of steps) out = out.replace(pattern, replacement);
  return out.replace(/(?<![A-Za-z0-9._\\/:-])[A-Z][A-Z0-9._-]{2,14}\\<user>/g, "<user>");
};

// `includeUser` is false for raw tool output (cargo, npm), which may legitimately print the profile
// path; the rest of the identity is never expected there.
export const findPrivate = (text, { identity = identityFromEnv(), includeUser = true } = {}) => {
  const found = [];
  const named = [["domain", identity.domain], ["host", identity.host], ["dnsdomain", identity.dnsDomain]];
  if (includeUser) named.push(["user", identity.user]);
  String(text).split(/\r?\n/).forEach((line, index) => {
    const kinds = new Set();
    if (new RegExp(EMAIL.source).test(line)) kinds.add("email");
    for (const m of line.matchAll(DOMAIN_ACCOUNT)) if (!WELL_KNOWN.has(m[1])) kinds.add("domain-account");
    for (const [kind, name] of named) if (usable(name) && wholeWord(name).test(line)) kinds.add(kind);
    for (const kind of kinds) found.push({ line: index + 1, kind });
  });
  return found;
};
