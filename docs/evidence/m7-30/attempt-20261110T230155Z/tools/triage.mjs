// Triage sweep results: hard = a target still violating after being scrolled to centre; scroll-dependent = the flagged
// target cleared once scrolled but a different one was under the sticky strip at the new scroll offset.
// usage: node triage.mjs <results.json> [<results.json>...]
import { readFileSync } from "node:fs";

for (const file of process.argv.slice(2)) {
  const results = JSON.parse(readFileSync(file, "utf8"));
  const out = { states: results.length, clean: 0, restObscured: 0, scrollDependent: 0, hard: [], docY: 0, hscroll: 0, overlap: 0, consoleErrors: 0 };
  for (const r of results) {
    if (r.metrics.docOverflowY > 1) out.docY++;
    if (r.metrics.hScroll) out.hscroll++;
    if (r.metrics.activityOverlapsMain) out.overlap++;
    out.consoleErrors += r.consoleErrors.length;
    if (r.violations.length === 0) {
      out.clean++;
      continue;
    }
    const before = new Set(r.violations.flatMap((v) => v.nodes.map((n) => `${v.id}|${n.target[0]}`)));
    const after = (r.afterScroll ?? []).flatMap((v) => v.nodes.map((n) => `${v.id}|${n.target[0]}`));
    const persisting = after.filter((k) => before.has(k));
    if (after.length === 0) out.restObscured++;
    else if (persisting.length === 0 && r.violations.every((v) => v.id === "target-size")) out.scrollDependent++;
    else out.hard.push({ name: r.name, persisting, after });
  }
  console.log(file, JSON.stringify(out));
}
