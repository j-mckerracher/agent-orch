// Decides, from environment variable NAMES only, whether a release build is signed.
// Pure: reads names and emptiness, never prints or returns a secret value.
//
// A completely absent configuration is a clean skip (unsigned build, clear message) for a manual
// run, but an error when signing is required: REQUIRE_SIGNING=true, or a `v*` tag build
// (GITHUB_REF_TYPE=tag) unless ALLOW_UNSIGNED_RELEASE=true opts in explicitly. A partial
// configuration is always an error: a half-configured release must not quietly ship unsigned.

const PLATFORMS = {
  macos: {
    sign: ["APPLE_CERTIFICATE", "APPLE_CERTIFICATE_PASSWORD", "APPLE_SIGNING_IDENTITY"],
    notarize: ["APPLE_ID", "APPLE_PASSWORD", "APPLE_TEAM_ID"],
  },
  windows: {
    sign: ["WINDOWS_CERTIFICATE", "WINDOWS_CERTIFICATE_PASSWORD"],
    notarize: [],
  },
};

const present = (env, name) => typeof env[name] === "string" && env[name].trim() !== "";

export function signingDecision(platform, env) {
  const spec = PLATFORMS[platform];
  if (!spec) throw new Error(`unknown platform ${platform}`);
  const tagBuild = env.GITHUB_REF_TYPE === "tag" && env.ALLOW_UNSIGNED_RELEASE !== "true";
  const required = env.REQUIRE_SIGNING === "true" || tagBuild;
  const why = env.REQUIRE_SIGNING === "true"
    ? "REQUIRE_SIGNING=true"
    : "this is a tag build; set the repository variable ALLOW_UNSIGNED_RELEASE=true to allow an unsigned tag build";
  const names = [...spec.sign, ...spec.notarize];
  const have = names.filter((n) => present(env, n));
  const missingSign = spec.sign.filter((n) => !present(env, n));
  const missingNotarize = spec.notarize.filter((n) => !present(env, n));
  const failure = (message) => ({ mode: "error", sign: false, notarize: false, message });

  if (have.length === 0) {
    return required
      ? failure(`Signing is required (${why}) but no ${platform} signing secret is set (${names.join(", ")}).`)
      : {
          mode: "unsigned", sign: false, notarize: false,
          message: `No ${platform} signing secrets are set (${names.join(", ")}); building UNSIGNED. This build must not be distributed.`,
        };
  }
  if (missingSign.length > 0) {
    return failure(`Incomplete ${platform} signing configuration; missing: ${missingSign.join(", ")}.`);
  }
  const partialNotarize = spec.notarize.length > 0 && missingNotarize.length > 0 && missingNotarize.length < spec.notarize.length;
  if (partialNotarize) {
    return failure(`Incomplete ${platform} notarization configuration; missing: ${missingNotarize.join(", ")}.`);
  }
  const notarize = spec.notarize.length > 0 && missingNotarize.length === 0;
  if (spec.notarize.length > 0 && !notarize && required) {
    return failure(`Signing is required (${why}) but ${platform} notarization secrets are not set (${spec.notarize.join(", ")}).`);
  }
  return {
    mode: "signed", sign: true, notarize,
    message: notarize || spec.notarize.length === 0
      ? `Signing${notarize ? " and notarizing" : ""} the ${platform} build.`
      : `Signing the ${platform} build; notarization secrets are absent (${spec.notarize.join(", ")}), so it is NOT notarized.`,
  };
}

// Names a packaging child process may inherit. Empty values are dropped by the caller: Tauri
// treats a variable that is set but empty as configured.
export const signingNames = (platform) => [...PLATFORMS[platform].sign, ...PLATFORMS[platform].notarize];
