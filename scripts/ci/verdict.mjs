// The overall verdict of a run, from the verdicts of its gates and harness checks.
//
//   FAIL          at least one item failed (or has a verdict this module does not know).
//   INCONCLUSIVE  nothing failed, but at least one gate was SKIPPED (not measured) or none ran.
//   PASS          every item passed.
//
// A skipped gate is a measurement that is missing, so it can never contribute to a PASS. FAIL wins over
// INCONCLUSIVE: a run with a failure and a skip is a failure.
export const OVERALL = { PASS: "PASS", FAIL: "FAIL", INCONCLUSIVE: "INCONCLUSIVE" };

// 0 is the only success; the other two are distinct so a caller can tell a failure from a missing measurement.
export const EXIT_CODES = { PASS: 0, FAIL: 1, INCONCLUSIVE: 2 };

export const overallVerdict = (gates, checks = []) => {
  const all = [...gates, ...checks];
  const verdicts = all.map((r) => r.verdict);
  if (verdicts.some((v) => v !== "PASS" && v !== "SKIPPED")) return OVERALL.FAIL;
  if (gates.length === 0 || verdicts.includes("SKIPPED")) return OVERALL.INCONCLUSIVE;
  return OVERALL.PASS;
};

export const countVerdicts = (items) => {
  const counts = { PASS: 0, FAIL: 0, SKIPPED: 0 };
  for (const { verdict } of items) counts[verdict] = (counts[verdict] ?? 0) + 1;
  return counts;
};
