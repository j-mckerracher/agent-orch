// Behavioural matrix checks (non-axe): theme, reduced motion, navigation, keyboard, connection states,
// approvals, data honesty. usage: node behave.mjs <outDir> [--only a,b]
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { launchBrowser, newPage, startDaemon, url, sleep, RawClient, APP } from "./lib.mjs";

const out = process.argv[2];
const only = process.argv.includes("--only") ? process.argv[process.argv.indexOf("--only") + 1].split(",") : null;
mkdirSync(join(out, "shots"), { recursive: true });
const results = [];
const rec = (id, verdict, detail) => {
  results.push({ id, verdict, detail });
  console.log(`${verdict.padEnd(5)} ${id}: ${typeof detail === "string" ? detail : JSON.stringify(detail)}`);
};
const want = (g) => !only || only.includes(g);
const text = (page) => page.evaluate(() => document.body.innerText);
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
const active = (page) =>
  page.evaluate(() => {
    const e = document.activeElement;
    return e ? `${e.tagName.toLowerCase()}${e.id ? "#" + e.id : ""}[${(e.getAttribute("aria-label") || e.innerText || "").trim().replace(/\s+/g, " ").slice(0, 40)}]` : null;
  });
const shot = (page, name) => page.screenshot({ path: join(out, "shots", `${name}.png`) });
const ILLUSTRATIVE = /\b20\s*\/\s*24\b|\$\s?26\.00|CPU\s*(?:41|44)\b/i;

const browser = await launchBrowser();
let daemon = null;
let keepAlive = null;
try {
  // ---------- theme ----------
  if (want("theme")) {
    for (const mode of ["fixture", "nobridge"]) {
      const { page, context } = await newPage(browser, { width: 1280, height: 800, mode, scheme: "light" });
      await page.goto(url({ mode: mode === "fixture" ? "fixture" : "x", route: "workspace" }), { waitUntil: "load" });
      await sleep(500);
      const before = await page.evaluate(() => document.documentElement.getAttribute("data-theme"));
      await clickText(page, "button", /dark mode/);
      await sleep(200);
      const toggled = await page.evaluate(() => ({ t: document.documentElement.getAttribute("data-theme"), p: [...document.querySelectorAll("button")].find((b) => /dark mode/i.test(b.innerText))?.getAttribute("aria-pressed"), ls: localStorage.getItem("agent-orch.theme") }));
      await page.reload({ waitUntil: "load" });
      await sleep(500);
      const after = await page.evaluate(() => document.documentElement.getAttribute("data-theme"));
      rec(`theme-toggle-persist-${mode}`, (before === null || before === "light") && toggled.t === "dark" && toggled.p === "true" && toggled.ls === "dark" && after === "dark" ? "PASS" : "FAIL", { dataThemeBeforeToggle: before, toggled, afterReload: after });
      await context.close();
    }
    for (const scheme of ["dark", "light"]) {
      const { page, context } = await newPage(browser, { width: 1280, height: 800, mode: "fixture", scheme });
      await page.goto(url({ mode: "fixture", route: "workspace" }), { waitUntil: "load" });
      await sleep(400);
      const t = await page.evaluate(() => ({ attr: document.documentElement.getAttribute("data-theme"), pressed: [...document.querySelectorAll("button")].find((b) => /dark mode/i.test(b.innerText))?.getAttribute("aria-pressed"), bg: getComputedStyle(document.body).backgroundColor }));
      rec(`theme-os-preference-${scheme}`, t.pressed === String(scheme === "dark") ? "PASS" : "FAIL", t);
      await context.close();
    }
  }

  // ---------- reduced motion ----------
  if (want("motion")) {
    for (const reduce of [true, false]) {
      const { page, context } = await newPage(browser, { width: 1280, height: 800, mode: "fixture", reduceMotion: reduce });
      await page.evaluateOnNewDocument(() => {
        window.__raf = 0;
        const orig = window.requestAnimationFrame.bind(window);
        window.requestAnimationFrame = (cb) => {
          window.__raf++;
          return orig(cb);
        };
      });
      await page.goto(url({ mode: "fixture", route: "workspace" }), { waitUntil: "load" });
      await sleep(500);
      const bgOffByDefault = await page.evaluate(() => [...document.querySelectorAll("button")].find((b) => /background/i.test(b.innerText.trim()))?.getAttribute("aria-pressed"));
      const longMotion = await page.evaluate(() => {
        const ms = (v) => Math.max(...String(v).split(",").map((x) => (x.trim().endsWith("ms") ? parseFloat(x) : parseFloat(x) * 1000)));
        let n = 0;
        const bad = [];
        for (const e of document.querySelectorAll("*")) {
          const cs = getComputedStyle(e);
          const a = cs.animationName !== "none" ? ms(cs.animationDuration) : 0;
          const t = ms(cs.transitionDuration);
          if (a > 1 || t > 1) bad.push(`${e.tagName.toLowerCase()}.${String(e.className).slice(0, 30)} a=${a} t=${t}`);
          n++;
        }
        return { elements: n, bad: bad.slice(0, 8), badCount: bad.length };
      });
      await clickText(page, "button", /background/);
      await sleep(400);
      const bgOn = await page.evaluate(() => ({ pressed: [...document.querySelectorAll("button")].find((b) => /background/i.test(b.innerText.trim()))?.getAttribute("aria-pressed"), host: !!document.querySelector("[data-testid=ambient-background]"), canvas: document.querySelectorAll("[data-testid=ambient-background] canvas").length }));
      const reducedAttr = await page.evaluate(() => [...document.querySelectorAll("[data-testid=ambient-background], [data-testid=ambient-background] *")].map((e) => e.getAttributeNames().filter((a) => /motion|static|reduced/i.test(a)).map((a) => `${a}=${e.getAttribute(a)}`)).flat());
      const rafStart = await page.evaluate(() => window.__raf);
      await sleep(1000);
      const rafPerSecondAmbientOn = (await page.evaluate(() => window.__raf)) - rafStart;
      rec(`reduced-motion-${reduce ? "reduce" : "no-preference"}`, bgOffByDefault === "false" && (reduce ? longMotion.badCount === 0 && rafPerSecondAmbientOn <= 2 : true) ? "PASS" : "FAIL", { ambientOffByDefault: bgOffByDefault === "false", motionLongerThan1ms: longMotion, ambientAfterToggle: bgOn, rafPerSecondAmbientOn, ambientReducedAttrs: reducedAttr });
      await context.close();
    }
  }

  // ---------- navigation ----------
  if (want("nav")) {
    const { page, context } = await newPage(browser, { width: 1280, height: 800, mode: "fixture" });
    await page.goto(url({ mode: "fixture", route: "workspace" }), { waitUntil: "load" });
    await sleep(500);
    const heads = {};
    for (const [label, route] of [["Orchestrator Workspace", "workspace"], ["Agents & Sessions", "agents"], ["Review & Decisions", "review"], ["Projects", "workstreams"]]) {
      await clickText(page, "nav[aria-label='Primary views'] a, nav[aria-label='Primary views'] button", new RegExp(label.replace(/[&]/g, "\\&")));
      await sleep(300);
      heads[route] = await page.evaluate(() => ({ hash: location.hash, h1: document.querySelector("h1")?.innerText, title: document.title, focus: (() => { const e = document.activeElement; return e ? `${e.tagName.toLowerCase()}${e.id ? "#" + e.id : ""}` : null; })(), current: document.querySelector("nav[aria-label='Primary views'] [aria-current]")?.innerText.trim().replace(/\s+/g, " ") }));
    }
    rec("nav-four-routes", Object.values(heads).every((h) => h.h1 && h.current) ? "PASS" : "FAIL", heads);
    await page.goBack();
    await sleep(300);
    const back = await page.evaluate(() => location.hash);
    await page.goForward();
    await sleep(300);
    const fwd = await page.evaluate(() => location.hash);
    rec("nav-back-forward", back !== fwd ? "PASS" : "FAIL", { back, forward: fwd });
    for (const h of ["#/project/legacy-id", "#/project/legacy-id/tasks/t1", "#/does-not-exist", "#/", "#/workstreams/nope"]) {
      await page.evaluate((x) => (location.hash = x), h);
      await sleep(400);
      const s = await page.evaluate(() => ({ hash: location.hash, h1: document.querySelector("h1")?.innerText ?? null, blank: document.body.innerText.trim().length < 20 }));
      rec(`nav-deeplink ${h}`, !s.blank && s.h1 ? "PASS" : "FAIL", s);
    }
    await context.close();
  }

  // ---------- keyboard ----------
  if (want("keyboard")) {
    for (const mode of ["fixture", "live"]) {
      if (mode === "live" && !daemon) {
        daemon = await startDaemon();
        keepAlive = (await RawClient.connect(daemon.dataDir, ["health"])).client;
      }
      const { page, context } = await newPage(browser, { width: 1280, height: 800, mode, daemon });
      await page.goto(url({ mode: mode === "fixture" ? "fixture" : "x", route: "workspace" }), { waitUntil: "load" });
      await sleep(mode === "live" ? 1800 : 600);
      const first = [];
      for (let i = 0; i < 4; i++) {
        await page.keyboard.press("Tab");
        first.push(await active(page));
      }
      rec(`kbd-first-stops-${mode}`, first.every(Boolean) ? "PASS" : "FAIL", first);
      // Enter on a nav link navigates; the target view takes focus or exposes an h1.
      await page.evaluate(() => document.querySelector("nav[aria-label='Primary views'] a, nav[aria-label='Primary views'] button")?.focus());
      for (let i = 0; i < 2; i++) await page.keyboard.press("Tab");
      const before = await page.evaluate(() => location.hash);
      await page.keyboard.press("Enter");
      await sleep(400);
      const after = await page.evaluate(() => location.hash);
      rec(`kbd-nav-enter-${mode}`, before !== after ? "PASS" : "FAIL", { before, after, focus: await active(page) });
      // Workstream tree: arrows move, Enter activates.
      await page.evaluate(() => (location.hash = "#/workspace"));
      await sleep(400);
      const hasTree = await page.evaluate(() => !!document.querySelector("[role=tree] [role=treeitem]"));
      if (hasTree) {
        await page.evaluate(() => document.querySelector("[role=tree] [role=treeitem]")?.focus());
        const a0 = await active(page);
        await page.keyboard.press("ArrowDown");
        const a1 = await active(page);
        await page.keyboard.press("ArrowDown");
        const a2 = await active(page);
        await page.keyboard.press("ArrowUp");
        const a3 = await active(page);
        await page.keyboard.press("Home");
        const a4 = await active(page);
        await page.keyboard.press("End");
        const a5 = await active(page);
        rec(`kbd-tree-arrows-${mode}`, a0 !== a1 || a1 !== a2 ? "PASS" : "FAIL", { a0, a1, a2, up: a3, home: a4, end: a5 });
      } else rec(`kbd-tree-arrows-${mode}`, "LIMITATION", "no tree items in this mode (no workstreams to navigate)");
      // Popovers: model menu / target picker open with Enter/Space and close with Escape, returning focus.
      for (const [label, re] of [["target-picker", /talking to|target|conversation/i], ["model-menu", /model/i]]) {
        const found = await page.evaluate((r) => { const rx = new RegExp(r, "i"); const b = [...document.querySelectorAll("button[aria-haspopup], button[aria-expanded]")].find((x) => rx.test(x.innerText + (x.getAttribute("aria-label") || ""))); if (!b) return null; b.focus(); return b.innerText.trim().slice(0, 30); }, re.source);
        if (!found) {
          rec(`kbd-${label}-${mode}`, "LIMITATION", "control not present in this mode");
          continue;
        }
        await page.keyboard.press("Enter");
        await sleep(250);
        const open = await page.evaluate(() => document.activeElement?.closest("[role=listbox],[role=menu],[role=radiogroup],[role=dialog]") !== null || !!document.querySelector("[role=listbox],[role=menu],[role=radiogroup]:not([aria-label='Task filters'])"));
        const inside = await active(page);
        await page.keyboard.press("Escape");
        await sleep(250);
        const back = await active(page);
        rec(`kbd-${label}-${mode}`, open ? "PASS" : "FAIL", { trigger: found, openedWithEnter: open, focusInside: inside, afterEscape: back });
      }
      // Activity disclosures toggle with Enter and Space.
      const disc = await page.evaluate(() => { const s = document.querySelector("details.activity-group > summary, .shell-activity details > summary"); if (!s) return null; s.focus(); return s.parentElement.open; });
      if (disc !== null) {
        await page.keyboard.press("Enter");
        const o1 = await page.evaluate(() => document.activeElement.parentElement.open);
        await page.keyboard.press("Space");
        const o2 = await page.evaluate(() => document.activeElement.parentElement.open);
        rec(`kbd-disclosure-${mode}`, o1 !== disc && o2 !== o1 ? "PASS" : "FAIL", { start: disc, afterEnter: o1, afterSpace: o2 });
      } else rec(`kbd-disclosure-${mode}`, "LIMITATION", "no disclosure present");
      await context.close();
    }
  }

  // ---------- connection states ----------
  if (want("conn")) {
    if (!daemon) {
      daemon = await startDaemon();
      keepAlive = (await RawClient.connect(daemon.dataDir, ["health"])).client;
    }
    const { page, context, bridge } = await newPage(browser, { width: 1280, height: 800, mode: "live", daemon });
    const states = [];
    const snap = async (name) => {
      const s = await page.evaluate(() => ({ conn: document.querySelector("[data-testid='shell-connection']")?.innerText.replace(/\s+/g, " ").slice(0, 160) ?? null, fixtureBanner: !!document.querySelector("[data-testid=fixture-banner]"), stripVisible: !!document.querySelector(".shell-resources")?.getBoundingClientRect().height, body: document.body.innerText }));
      states.push({ name, ...s, body: undefined, illustrative: ILLUSTRATIVE.test(s.body) });
      await shot(page, `conn-${name}`);
      return s;
    };
    await page.goto(url({ mode: "x", route: "workspace" }), { waitUntil: "load" });
    await sleep(2000);
    const s1 = await snap("1-connected");
    rec("conn-connected", /Connected/.test(s1.conn ?? "") && !s1.fixtureBanner ? "PASS" : "FAIL", states.at(-1));
    // sever the connection, as a crashed daemon would
    bridge.severAll();
    await sleep(1500);
    const s2 = await snap("2-severed");
    rec("conn-severed-shows-degraded", !/^● Connected/.test(s2.conn ?? "") && !s2.fixtureBanner && !ILLUSTRATIVE.test(s2.body) ? "PASS" : "FAIL", states.at(-1));
    await sleep(6000);
    const s3 = await snap("3-after-auto-retry");
    rec("conn-auto-recovery-after-sever", /Connected/.test(s3.conn ?? "") ? "PASS" : "LIMITATION", { ...states.at(-1), note: "daemon stayed up; connection re-established by the client's reconnect logic if PASS" });
    // daemon halted
    await daemon.halt();
    bridge.severAll();
    await sleep(2500);
    const s4 = await snap("4-daemon-halted");
    rec("conn-daemon-halted", !/^● Connected/.test(s4.conn ?? "") && !s4.fixtureBanner && !ILLUSTRATIVE.test(s4.body) && /not running|unreachable|Reconnecting|Offline|unavailable/i.test(s4.body) ? "PASS" : "FAIL", states.at(-1));
    for (const route of ["agents", "review", "workstreams", "workspace"]) {
      await page.evaluate((r) => (location.hash = `#/${r}`), route);
      await sleep(400);
      const vis = await page.evaluate(() => ({ strip: !!document.querySelector(".shell-resources")?.getBoundingClientRect().height, conn: !!document.querySelector("[data-testid='shell-connection']"), fixture: !!document.querySelector("[data-testid=fixture-banner]") }));
      rec(`conn-halted-strip-visible-${route}`, vis.strip && vis.conn && !vis.fixture ? "PASS" : "FAIL", vis);
    }
    // restart
    await daemon.start();
    keepAlive = (await RawClient.connect(daemon.dataDir, ["health"])).client;
    let recovered = false;
    for (let i = 0; i < 20 && !recovered; i++) {
      await sleep(1000);
      recovered = await page.evaluate(() => /Connected/.test(document.querySelector("[data-testid='shell-connection']")?.innerText ?? ""));
      if (!recovered && i === 6) await clickText(page, "button", /refresh \/ reconnect/);
    }
    const s5 = await snap("5-restarted");
    rec("conn-daemon-restart-recovery", recovered ? "PASS" : "FAIL", states.at(-1));
    writeFileSync(join(out, "conn-states.json"), JSON.stringify(states, null, 1));
    await context.close();
  }

  // ---------- data honesty ----------
  if (want("honesty")) {
    if (!daemon || (await (async () => false)())) {
      daemon = daemon ?? (await startDaemon());
      keepAlive = keepAlive ?? (await RawClient.connect(daemon.dataDir, ["health"])).client;
    }
    const rows = [];
    for (const mode of ["live", "fixture", "nobridge"]) {
      for (const route of ["workspace", "workstreams", "agents", "review"]) {
        const { page, context } = await newPage(browser, { width: 1280, height: 900, mode: mode === "live" ? "live" : mode, daemon });
        await page.goto(url({ mode: mode === "fixture" ? "fixture" : "x", route }), { waitUntil: "load" });
        await sleep(mode === "live" ? 2200 : 700);
        // open every disclosure so hidden text is scanned too
        await page.evaluate(() => document.querySelectorAll("details").forEach((d) => (d.open = true)));
        await sleep(200);
        const s = await page.evaluate(() => {
          const body = document.body.innerText;
          const strip = document.querySelector(".shell-resources")?.innerText.replace(/\s+/g, " ") ?? "";
          const placeholders = [...document.querySelectorAll("*")].filter((e) => e.children.length === 0 && /PLACEHOLDER/i.test(e.textContent)).length;
          return { body, strip: strip.slice(0, 300), placeholders };
        });
        const hit = s.body.match(ILLUSTRATIVE);
        rows.push({ mode, route, illustrativeHit: hit?.[0] ?? null, placeholderLabels: s.placeholders, strip: s.strip });
        if (route === "workspace") await shot(page, `honesty-${mode}-${route}`);
        await context.close();
      }
    }
    writeFileSync(join(out, "honesty.json"), JSON.stringify(rows, null, 1));
    const liveHits = rows.filter((r) => r.mode !== "fixture" && r.illustrativeHit);
    rec("honesty-no-illustrative-numbers-live-and-nobridge", liveHits.length === 0 ? "PASS" : "FAIL", liveHits.length ? liveHits : { scanned: rows.filter((r) => r.mode !== "fixture").length });
    const fx = rows.filter((r) => r.mode === "fixture");
    rec("honesty-fixture-illustrative-numbers-labelled", fx.every((r) => !r.illustrativeHit || r.placeholderLabels > 0) ? "PASS" : "FAIL", fx.map((r) => ({ route: r.route, hit: r.illustrativeHit, placeholderLabels: r.placeholderLabels })));
    rec("honesty-live-strip-source", "INFO", rows.find((r) => r.mode === "live" && r.route === "workspace")?.strip);
  }
} finally {
  keepAlive?.socket.destroy();
  await browser.close();
  if (daemon) await daemon.stop();
  writeFileSync(join(out, "behave.json"), JSON.stringify(results, null, 1));
}
console.log(`checks: ${results.length}; FAIL: ${results.filter((r) => r.verdict === "FAIL").length}`);
