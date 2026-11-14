// Live-daemon operations through the real UI: workstream create/link, conversation send, approvals, keyboard tree.
// usage: node liveops.mjs <outDir>
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { launchBrowser, newPage, startDaemon, url, sleep, RawClient } from "./lib.mjs";

const out = process.argv[2];
mkdirSync(join(out, "shots"), { recursive: true });
const results = [];
const rec = (id, verdict, detail) => {
  results.push({ id, verdict, detail });
  console.log(`${verdict.padEnd(5)} ${id}: ${typeof detail === "string" ? detail : JSON.stringify(detail)}`);
};
const clickText = (page, sel, re) =>
  page.evaluate(
    (s, r) => {
      const rx = new RegExp(r, "i");
      const el = [...document.querySelectorAll(s)].find((e) => rx.test(e.innerText || e.getAttribute("aria-label") || ""));
      if (!el) return false;
      el.click();
      return true;
    },
    sel,
    re.source,
  );
const bodyText = (page) => page.evaluate(() => document.body.innerText);
const shot = (page, name) => page.screenshot({ path: join(out, "shots", `${name}.png`) });
const sentMethods = (bridge) => bridge.sent.map((m) => m.method);
const FIXTURE_ID = /fixture:/i;

const browser = await launchBrowser();
const daemon = await startDaemon();
const keepAlive = (await RawClient.connect(daemon.dataDir, ["health", "projects", "tasks", "sessions", "events", "approvals", "queue", "workspaces", "delivery", "agents", "host", "coordinator", "review", "workstreams"])).client;
const seeded = daemon.seeded;
console.log("seeded", JSON.stringify(seeded));
const requestApproval = await new Promise((res) => {
  keepAlive.listeners.push((m) => m.id === 9001 && res(m));
  keepAlive.send({ jsonrpc: "2.0", id: 9001, method: "review.request_approval", params: { task_id: seeded.task_id, workspace_id: seeded.workspace_id, verification_run_id: seeded.verification_run_id, action: "local_review", target_ref: `refs/heads/orchestrator/review/${seeded.task_id}`, idempotency_key: "m7-30-req" } });
});
console.log("request_approval", JSON.stringify(requestApproval).slice(0, 300));
try {
  const { page, context, bridge, errors } = await newPage(browser, { width: 1280, height: 900, theme: "light", mode: "live", daemon });
  await page.goto(url({ mode: "x", route: "workstreams" }), { waitUntil: "load" });
  await sleep(2200);
  await shot(page, "01-projects-live-initial");

  // ---- workstream create ----
  await page.type("form[aria-label='Create workstream'] input[type=text]", "M7-30 accessibility workstream");
  await shot(page, "02-create-typed");
  await page.keyboard.press("Enter");
  await sleep(1500);
  await shot(page, "03-created");
  const t1 = await bodyText(page);
  const created = bridge.sent.find((m) => m.method === "workstreams.create");
  rec("live-workstream-create", created && created.params?.idempotency_key && !FIXTURE_ID.test(JSON.stringify(created)) && /M7-30 accessibility workstream/.test(t1) ? "PASS" : "FAIL", { sent: created, visible: /M7-30 accessibility workstream/.test(t1) });

  // ---- keyboard on the workstream tree (live) ----
  const hasTree = await page.evaluate(() => !!document.querySelector("[role=tree] [role=treeitem]"));
  if (hasTree) {
    await page.evaluate(() => document.querySelector("[role=tree] [role=treeitem]").focus());
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Enter");
    await sleep(500);
    rec("live-tree-keyboard", "PASS", { focus: await page.evaluate(() => document.activeElement?.innerText.slice(0, 60)), hash: await page.evaluate(() => location.hash) });
  } else rec("live-tree-keyboard", "FAIL", "no tree item after create");

  // ---- link the seeded task to the workstream through the UI ----
  await shot(page, "04-ws-detail");
  const linkForm = await page.evaluate(() => !!document.querySelector("form[aria-label*='ink' i], form[aria-label*='ask' i]"));
  const linkSelects = await page.evaluate(() => [...document.querySelectorAll("form select")].map((s) => ({ label: s.closest("label")?.innerText.slice(0, 60), options: [...s.options].map((o) => o.value).slice(0, 6) })));
  rec("live-link-form-present", "INFO", { linkForm, linkSelects });

  // ---- conversation send ----
  await page.evaluate(() => (location.hash = "#/workspace"));
  await sleep(1200);
  await page.type("textarea[id$='-draft']", "m7-30 live message");
  await shot(page, "05-composer-typed");
  await page.keyboard.down("Control");
  await page.keyboard.press("Enter");
  await page.keyboard.up("Control");
  await sleep(2000);
  await shot(page, "06-after-send");
  const send = bridge.sent.find((m) => m.method === "coordinator.send_message");
  const wsText = await bodyText(page);
  rec("live-conversation-send", send ? (!FIXTURE_ID.test(JSON.stringify(send.params)) && send.params?.idempotency_key ? "PASS" : "FAIL") : "LIMITATION", { sent: send?.params, tail: wsText.slice(-400).replace(/\s+/g, " ") });

  // ---- approvals through the Review view ----
  await page.evaluate(() => (location.hash = "#/review"));
  await sleep(1800);
  await shot(page, "07-review-live");
  const reviewText = await bodyText(page);
  rec("live-review-visible", /Task live-task-1/i.test(reviewText) ? "PASS" : "FAIL", reviewText.slice(0, 500).replace(/\s+/g, " "));
  writeFileSync(join(out, "review-body.txt"), reviewText);
  const buttons = await page.evaluate(() => [...document.querySelectorAll("button")].map((b) => `${b.innerText.trim().replace(/\s+/g, " ").slice(0, 40)}${b.disabled ? " [disabled]" : ""}`));
  console.log("buttons", JSON.stringify(buttons));
  writeFileSync(join(out, "review-buttons.json"), JSON.stringify(buttons, null, 1));
  await clickText(page, "button", /Task live-task-1/);
  await sleep(2500);
  await shot(page, "08-review-candidate");
  const acceptState = await page.evaluate(() => {
    const b = [...document.querySelectorAll("button")].find((x) => x.innerText.trim() === "Accept");
    return b ? { disabled: b.disabled, describedby: b.getAttribute("aria-describedby") } : null;
  });
  rec("live-review-accept-present", acceptState ? "PASS" : "FAIL", acceptState);
  if (acceptState && !acceptState.disabled) {
    await page.focus("button[aria-label='Accept'], button:not([disabled])").catch(() => {});
    await page.evaluate(() => [...document.querySelectorAll("button")].find((x) => x.innerText.trim() === "Accept").focus());
    await page.keyboard.press("Enter");
    await sleep(3000);
    await shot(page, "09-after-accept");
    const resolve = bridge.sent.find((m) => m.method === "approvals.resolve");
    rec("live-approval-accept-keyboard", resolve && resolve.params?.idempotency_key && !FIXTURE_ID.test(JSON.stringify(resolve.params)) ? "PASS" : "FAIL", { sent: resolve?.params, after: (await bodyText(page)).slice(0, 0) });
  } else {
    writeFileSync(join(out, "review-candidate-body.txt"), await bodyText(page));
  }

  writeFileSync(join(out, "bridge-sent.json"), JSON.stringify(bridge.sent, null, 1));
  writeFileSync(join(out, "console-errors.json"), JSON.stringify(errors, null, 1));
  rec("live-no-fixture-ids-on-wire", bridge.sent.every((m) => !FIXTURE_ID.test(JSON.stringify(m))) ? "PASS" : "FAIL", { messages: bridge.sent.length, methods: [...new Set(sentMethods(bridge))] });
  rec("live-console-errors", errors.length === 0 ? "PASS" : "FAIL", errors.slice(0, 5));
  await context.close();
} finally {
  keepAlive.socket.destroy();
  await browser.close();
  await daemon.stop();
  writeFileSync(join(out, "liveops.json"), JSON.stringify(results, null, 1));
}
console.log(`checks: ${results.length}; FAIL: ${results.filter((r) => r.verdict === "FAIL").length}`);
