# M7-01 rendered contrast inventory

Measured with Chrome 157 on the local Vite fixture (`npm run dev -- --host 127.0.0.1 --port 5187 --strictPort`). `measure-contrast.js` inserts primitive and app.css fixtures in the running page, sets each theme, and reads actual `getComputedStyle` colors. Hover declarations are copied from the loaded rules with a fixture-only class; focus rows check `:focus-visible`. Meter stop rows use visible swatches of the resolved tokens; actual gradient interpolation is not sampled. Transparent element backgrounds are composited over the fixture surface. Raw values: `contrast-rendered.json`.

The inventory tests text at 4.5:1 and non-text at 3:1, including decorative borders to show their numerical ratios. Sixty decorative separator/border rows fall below 3:1; those lines are identified as decorative in the token contract and are not necessary to identify the component. Eight individual meter marker/ring-to-stop rows also fall below 3:1. The existing test checks the higher-contrast member of the two-color marker, not each member separately, and it passes for every stop. Therefore the literal request that every individual non-text pair reach 3:1 is **not met**. This table does not establish contrast for every gradient interpolation or every external consumer context.

The coverage column refers to `src/ui/theme/contrast.test.ts`: “yes” means the token pair is directly included (possibly at a stricter 4.5 threshold); “conditional” means the test takes the maximum of marker and ring; “no—decorative” means no 3:1 assertion exists. These uncovered decorative and individual marker pairs do not satisfy the requested universal assertion; adding an unconditional 3:1 test for them would fail on the current palette.

| Element and state | Theme | Foreground token | Background token | Resolved foreground | Resolved background | Ratio | Threshold | Result | Test coverage |
|---|---|---|---|---|---|---:|---:|---|---|
| Disclosure closed | light | `--ink` | `--paper` | `rgb(17, 17, 17)` | `rgb(255, 255, 255)` | 18.88 | 4.5 | PASS | yes |
| Disclosure closed | light | `--ink` | `--panel` | `rgb(17, 17, 17)` | `rgb(255, 255, 255)` | 18.88 | 4.5 | PASS | yes |
| Disclosure closed | light | `--ink` | `--soft` | `rgb(17, 17, 17)` | `rgb(237, 244, 251)` | 17.03 | 4.5 | PASS | yes |
| Disclosure closed | light | `--ink` | `--soft-strong` | `rgb(17, 17, 17)` | `rgb(228, 239, 251)` | 16.22 | 4.5 | PASS | yes |
| Disclosure closed | light | `--ink` | `--control-surface` | `rgb(17, 17, 17)` | `rgb(242, 247, 252)` | 17.52 | 4.5 | PASS | yes |
| Disclosure open | light | `--ink` | `--soft-strong` | `rgb(17, 17, 17)` | `rgb(228, 239, 251)` | 16.22 | 4.5 | PASS | yes |
| Disclosure arrow closed | light | `--blue` | `--paper` | `rgb(0, 87, 168)` | `rgb(255, 255, 255)` | 7.17 | 3.0 | PASS | yes |
| Disclosure arrow closed | light | `--blue` | `--panel` | `rgb(0, 87, 168)` | `rgb(255, 255, 255)` | 7.17 | 3.0 | PASS | yes |
| Disclosure arrow closed | light | `--blue` | `--soft` | `rgb(0, 87, 168)` | `rgb(237, 244, 251)` | 6.47 | 3.0 | PASS | yes |
| Disclosure arrow closed | light | `--blue` | `--soft-strong` | `rgb(0, 87, 168)` | `rgb(228, 239, 251)` | 6.16 | 3.0 | PASS | yes |
| Disclosure arrow closed | light | `--blue` | `--control-surface` | `rgb(0, 87, 168)` | `rgb(242, 247, 252)` | 6.65 | 3.0 | PASS | yes |
| Disclosure arrow open | light | `--blue` | `--soft-strong` | `rgb(0, 87, 168)` | `rgb(228, 239, 251)` | 6.16 | 3.0 | PASS | yes |
| ui-pill neutral | light | `--ink` | `--soft` | `rgb(17, 17, 17)` | `rgb(237, 244, 251)` | 17.03 | 4.5 | PASS | yes |
| ui-badge neutral | light | `--ink` | `--soft` | `rgb(17, 17, 17)` | `rgb(237, 244, 251)` | 17.03 | 4.5 | PASS | yes |
| ui-pill info | light | `--blue` | `--blue-bg` | `rgb(0, 87, 168)` | `rgb(237, 245, 253)` | 6.51 | 4.5 | PASS | yes |
| ui-badge info | light | `--blue` | `--blue-bg` | `rgb(0, 87, 168)` | `rgb(237, 245, 253)` | 6.51 | 4.5 | PASS | yes |
| ui-pill ok | light | `--success` | `--success-bg` | `rgb(0, 87, 168)` | `rgb(237, 245, 253)` | 6.51 | 4.5 | PASS | yes |
| ui-badge ok | light | `--success` | `--success-bg` | `rgb(0, 87, 168)` | `rgb(237, 245, 253)` | 6.51 | 4.5 | PASS | yes |
| ui-pill warn | light | `--amber` | `--amber-bg` | `rgb(118, 80, 0)` | `rgb(255, 245, 219)` | 6.63 | 4.5 | PASS | yes |
| ui-badge warn | light | `--amber` | `--amber-bg` | `rgb(118, 80, 0)` | `rgb(255, 245, 219)` | 6.63 | 4.5 | PASS | yes |
| ui-pill placeholder | light | `--amber` | `--amber-bg` | `rgb(118, 80, 0)` | `rgb(255, 245, 219)` | 6.63 | 4.5 | PASS | yes |
| ui-badge placeholder | light | `--amber` | `--amber-bg` | `rgb(118, 80, 0)` | `rgb(255, 245, 219)` | 6.63 | 4.5 | PASS | yes |
| ui-pill bad | light | `--red` | `--red-bg` | `rgb(163, 45, 52)` | `rgb(255, 240, 240)` | 6.36 | 4.5 | PASS | yes |
| ui-badge bad | light | `--red` | `--red-bg` | `rgb(163, 45, 52)` | `rgb(255, 240, 240)` | 6.36 | 4.5 | PASS | yes |
| Button ui-btn | light | `--ink` | `--panel` | `rgb(17, 17, 17)` | `rgb(255, 255, 255)` | 18.88 | 4.5 | PASS | yes |
| Button ui-btn ui-btn-primary | light | `--on-accent` | `--accent` | `rgb(255, 255, 255)` | `rgb(0, 87, 168)` | 7.17 | 4.5 | PASS | yes |
| Button ui-btn ui-btn-danger | light | `--red` | `--red-bg` | `rgb(163, 45, 52)` | `rgb(255, 240, 240)` | 6.36 | 4.5 | PASS | yes |
| Button quiet | light | `--blue` | `--paper` | `rgb(0, 87, 168)` | `rgb(255, 255, 255)` | 7.17 | 4.5 | PASS | yes |
| Button disabled | light | `--subtle` | `--paper` | `rgb(52, 70, 90)` | `rgb(255, 255, 255)` | 9.68 | 4.5 | PASS | yes |
| Button quiet | light | `--blue` | `--panel` | `rgb(0, 87, 168)` | `rgb(255, 255, 255)` | 7.17 | 4.5 | PASS | yes |
| Button disabled | light | `--subtle` | `--panel` | `rgb(52, 70, 90)` | `rgb(255, 255, 255)` | 9.68 | 4.5 | PASS | yes |
| Button quiet | light | `--blue` | `--soft` | `rgb(0, 87, 168)` | `rgb(237, 244, 251)` | 6.47 | 4.5 | PASS | yes |
| Button disabled | light | `--subtle` | `--soft` | `rgb(52, 70, 90)` | `rgb(237, 244, 251)` | 8.73 | 4.5 | PASS | yes |
| Button quiet | light | `--blue` | `--soft-strong` | `rgb(0, 87, 168)` | `rgb(228, 239, 251)` | 6.16 | 4.5 | PASS | yes |
| Button disabled | light | `--subtle` | `--soft-strong` | `rgb(52, 70, 90)` | `rgb(228, 239, 251)` | 8.31 | 4.5 | PASS | yes |
| Button quiet | light | `--blue` | `--control-surface` | `rgb(0, 87, 168)` | `rgb(242, 247, 252)` | 6.65 | 4.5 | PASS | yes |
| Button disabled | light | `--subtle` | `--control-surface` | `rgb(52, 70, 90)` | `rgb(242, 247, 252)` | 8.98 | 4.5 | PASS | yes |
| Panel title | light | `--ink` | `--panel` | `rgb(17, 17, 17)` | `rgb(255, 255, 255)` | 18.88 | 4.5 | PASS | yes |
| Bubble ui-bubble | light | `--ink` | `--soft` | `rgb(17, 17, 17)` | `rgb(237, 244, 251)` | 17.03 | 4.5 | PASS | yes |
| Bubble metadata ui-bubble | light | `--subtle` | `--soft` | `rgb(52, 70, 90)` | `rgb(237, 244, 251)` | 8.73 | 4.5 | PASS | yes |
| Bubble ui-bubble ui-bubble-human | light | `--ink` | `--panel` | `rgb(17, 17, 17)` | `rgb(255, 255, 255)` | 18.88 | 4.5 | PASS | yes |
| Bubble metadata ui-bubble ui-bubble-human | light | `--subtle` | `--panel` | `rgb(52, 70, 90)` | `rgb(255, 255, 255)` | 9.68 | 4.5 | PASS | yes |
| EmptyState title | light | `--ink` | `--paper` | `rgb(17, 17, 17)` | `rgb(255, 255, 255)` | 18.88 | 4.5 | PASS | yes |
| EmptyState title | light | `--ink` | `--panel` | `rgb(17, 17, 17)` | `rgb(255, 255, 255)` | 18.88 | 4.5 | PASS | yes |
| EmptyState title | light | `--ink` | `--soft` | `rgb(17, 17, 17)` | `rgb(237, 244, 251)` | 17.03 | 4.5 | PASS | yes |
| EmptyState title | light | `--ink` | `--soft-strong` | `rgb(17, 17, 17)` | `rgb(228, 239, 251)` | 16.22 | 4.5 | PASS | yes |
| EmptyState title | light | `--ink` | `--control-surface` | `rgb(17, 17, 17)` | `rgb(242, 247, 252)` | 17.52 | 4.5 | PASS | yes |
| EmptyState description | light | `--subtle` | `--paper` | `rgb(52, 70, 90)` | `rgb(255, 255, 255)` | 9.68 | 4.5 | PASS | yes |
| EmptyState description | light | `--subtle` | `--panel` | `rgb(52, 70, 90)` | `rgb(255, 255, 255)` | 9.68 | 4.5 | PASS | yes |
| EmptyState description | light | `--subtle` | `--soft` | `rgb(52, 70, 90)` | `rgb(237, 244, 251)` | 8.73 | 4.5 | PASS | yes |
| EmptyState description | light | `--subtle` | `--soft-strong` | `rgb(52, 70, 90)` | `rgb(228, 239, 251)` | 8.31 | 4.5 | PASS | yes |
| EmptyState description | light | `--subtle` | `--control-surface` | `rgb(52, 70, 90)` | `rgb(242, 247, 252)` | 8.98 | 4.5 | PASS | yes |
| Meter label | light | `--ink` | `--paper` | `rgb(17, 17, 17)` | `rgb(255, 255, 255)` | 18.88 | 4.5 | PASS | yes |
| Meter label | light | `--ink` | `--panel` | `rgb(17, 17, 17)` | `rgb(255, 255, 255)` | 18.88 | 4.5 | PASS | yes |
| Meter label | light | `--ink` | `--soft` | `rgb(17, 17, 17)` | `rgb(237, 244, 251)` | 17.03 | 4.5 | PASS | yes |
| Meter label | light | `--ink` | `--soft-strong` | `rgb(17, 17, 17)` | `rgb(228, 239, 251)` | 16.22 | 4.5 | PASS | yes |
| Meter label | light | `--ink` | `--control-surface` | `rgb(17, 17, 17)` | `rgb(242, 247, 252)` | 17.52 | 4.5 | PASS | yes |
| Meter value | light | `--ink` | `--paper` | `rgb(17, 17, 17)` | `rgb(255, 255, 255)` | 18.88 | 4.5 | PASS | yes |
| Meter value | light | `--ink` | `--panel` | `rgb(17, 17, 17)` | `rgb(255, 255, 255)` | 18.88 | 4.5 | PASS | yes |
| Meter value | light | `--ink` | `--soft` | `rgb(17, 17, 17)` | `rgb(237, 244, 251)` | 17.03 | 4.5 | PASS | yes |
| Meter value | light | `--ink` | `--soft-strong` | `rgb(17, 17, 17)` | `rgb(228, 239, 251)` | 16.22 | 4.5 | PASS | yes |
| Meter value | light | `--ink` | `--control-surface` | `rgb(17, 17, 17)` | `rgb(242, 247, 252)` | 17.52 | 4.5 | PASS | yes |
| Theme toggle | light | `--ink` | `--panel` | `rgb(17, 17, 17)` | `rgb(255, 255, 255)` | 18.88 | 4.5 | PASS | yes |
| Theme toggle state | light | `--subtle` | `--panel` | `rgb(52, 70, 90)` | `rgb(255, 255, 255)` | 9.68 | 4.5 | PASS | yes |
| app.css pill | light | `--ink` | `--soft` | `rgb(17, 17, 17)` | `rgb(237, 244, 251)` | 17.03 | 4.5 | PASS | yes |
| app.css pill pill-connected | light | `--success` | `--success-bg` | `rgb(0, 87, 168)` | `rgb(237, 245, 253)` | 6.51 | 4.5 | PASS | yes |
| app.css pill pill-reconnecting | light | `--amber` | `--amber-bg` | `rgb(118, 80, 0)` | `rgb(255, 245, 219)` | 6.63 | 4.5 | PASS | yes |
| app.css pill pill-disconnected | light | `--red` | `--red-bg` | `rgb(163, 45, 52)` | `rgb(255, 240, 240)` | 6.36 | 4.5 | PASS | yes |
| app.css notice | light | `--ink` | `--amber-bg` | `rgb(17, 17, 17)` | `rgb(255, 245, 219)` | 17.39 | 4.5 | PASS | yes |
| app.css notice notice-error | light | `--ink` | `--red-bg` | `rgb(17, 17, 17)` | `rgb(255, 240, 240)` | 17.06 | 4.5 | PASS | yes |
| app.css placeholder | light | `--ink` | `--amber-bg` | `rgb(17, 17, 17)` | `rgb(255, 245, 219)` | 17.39 | 4.5 | PASS | yes |
| app.css badge | light | `--amber` | `--amber-bg` | `rgb(118, 80, 0)` | `rgb(255, 245, 219)` | 6.63 | 4.5 | PASS | yes |
| app.css placeholder-tag | light | `--amber` | `--amber-bg` | `rgb(118, 80, 0)` | `rgb(255, 245, 219)` | 6.63 | 4.5 | PASS | yes |
| app.css label | light | `--component-muted` | `--paper` | `rgb(51, 51, 51)` | `rgb(255, 255, 255)` | 12.63 | 4.5 | PASS | yes |
| app.css label | light | `--component-muted` | `--panel` | `rgb(51, 51, 51)` | `rgb(255, 255, 255)` | 12.63 | 4.5 | PASS | yes |
| app.css label | light | `--component-muted` | `--soft` | `rgb(51, 51, 51)` | `rgb(237, 244, 251)` | 11.39 | 4.5 | PASS | yes |
| app.css label | light | `--component-muted` | `--soft-strong` | `rgb(51, 51, 51)` | `rgb(228, 239, 251)` | 10.85 | 4.5 | PASS | yes |
| app.css label | light | `--component-muted` | `--control-surface` | `rgb(51, 51, 51)` | `rgb(242, 247, 252)` | 11.72 | 4.5 | PASS | yes |
| app.css muted | light | `--component-muted` | `--paper` | `rgb(51, 51, 51)` | `rgb(255, 255, 255)` | 12.63 | 4.5 | PASS | yes |
| app.css muted | light | `--component-muted` | `--panel` | `rgb(51, 51, 51)` | `rgb(255, 255, 255)` | 12.63 | 4.5 | PASS | yes |
| app.css muted | light | `--component-muted` | `--soft` | `rgb(51, 51, 51)` | `rgb(237, 244, 251)` | 11.39 | 4.5 | PASS | yes |
| app.css muted | light | `--component-muted` | `--soft-strong` | `rgb(51, 51, 51)` | `rgb(228, 239, 251)` | 10.85 | 4.5 | PASS | yes |
| app.css muted | light | `--component-muted` | `--control-surface` | `rgb(51, 51, 51)` | `rgb(242, 247, 252)` | 11.72 | 4.5 | PASS | yes |
| app.css input text | light | `--ink` | `--panel` | `rgb(17, 17, 17)` | `rgb(255, 255, 255)` | 18.88 | 4.5 | PASS | yes |
| app.css button | light | `--ink` | `--panel` | `rgb(17, 17, 17)` | `rgb(255, 255, 255)` | 18.88 | 4.5 | PASS | yes |
| app.css disabled button | light | `--subtle` | `--panel` | `rgb(52, 70, 90)` | `rgb(255, 255, 255)` | 9.68 | 4.5 | PASS | yes |
| app.css pre | light | `--ink` | `--soft` | `rgb(17, 17, 17)` | `rgb(237, 244, 251)` | 17.03 | 4.5 | PASS | yes |
| app.css strip | light | `--ink` | `--soft` | `rgb(17, 17, 17)` | `rgb(237, 244, 251)` | 17.03 | 4.5 | PASS | yes |
| Button boundary | light | `--control-border` | `--paper` | `rgb(107, 127, 148)` | `rgb(255, 255, 255)` | 4.13 | 3.0 | PASS | yes |
| Button boundary | light | `--control-border` | `--panel` | `rgb(107, 127, 148)` | `rgb(255, 255, 255)` | 4.13 | 3.0 | PASS | yes |
| Button boundary | light | `--control-border` | `--soft` | `rgb(107, 127, 148)` | `rgb(237, 244, 251)` | 3.72 | 3.0 | PASS | yes |
| Button boundary | light | `--control-border` | `--soft-strong` | `rgb(107, 127, 148)` | `rgb(228, 239, 251)` | 3.54 | 3.0 | PASS | yes |
| Button boundary | light | `--control-border` | `--control-surface` | `rgb(107, 127, 148)` | `rgb(242, 247, 252)` | 3.83 | 3.0 | PASS | yes |
| EmptyState boundary | light | `--control-border` | `--paper` | `rgb(107, 127, 148)` | `rgb(255, 255, 255)` | 4.13 | 3.0 | PASS | yes |
| EmptyState boundary | light | `--control-border` | `--panel` | `rgb(107, 127, 148)` | `rgb(255, 255, 255)` | 4.13 | 3.0 | PASS | yes |
| EmptyState boundary | light | `--control-border` | `--soft` | `rgb(107, 127, 148)` | `rgb(237, 244, 251)` | 3.72 | 3.0 | PASS | yes |
| EmptyState boundary | light | `--control-border` | `--soft-strong` | `rgb(107, 127, 148)` | `rgb(228, 239, 251)` | 3.54 | 3.0 | PASS | yes |
| EmptyState boundary | light | `--control-border` | `--control-surface` | `rgb(107, 127, 148)` | `rgb(242, 247, 252)` | 3.83 | 3.0 | PASS | yes |
| Meter boundary | light | `--control-border` | `--paper` | `rgb(107, 127, 148)` | `rgb(255, 255, 255)` | 4.13 | 3.0 | PASS | yes |
| Meter boundary | light | `--control-border` | `--panel` | `rgb(107, 127, 148)` | `rgb(255, 255, 255)` | 4.13 | 3.0 | PASS | yes |
| Meter boundary | light | `--control-border` | `--soft` | `rgb(107, 127, 148)` | `rgb(237, 244, 251)` | 3.72 | 3.0 | PASS | yes |
| Meter boundary | light | `--control-border` | `--soft-strong` | `rgb(107, 127, 148)` | `rgb(228, 239, 251)` | 3.54 | 3.0 | PASS | yes |
| Meter boundary | light | `--control-border` | `--control-surface` | `rgb(107, 127, 148)` | `rgb(242, 247, 252)` | 3.83 | 3.0 | PASS | yes |
| input boundary | light | `--control-border` | `--paper` | `rgb(107, 127, 148)` | `rgb(255, 255, 255)` | 4.13 | 3.0 | PASS | yes |
| input boundary | light | `--control-border` | `--panel` | `rgb(107, 127, 148)` | `rgb(255, 255, 255)` | 4.13 | 3.0 | PASS | yes |
| input boundary | light | `--control-border` | `--soft` | `rgb(107, 127, 148)` | `rgb(237, 244, 251)` | 3.72 | 3.0 | PASS | yes |
| input boundary | light | `--control-border` | `--soft-strong` | `rgb(107, 127, 148)` | `rgb(228, 239, 251)` | 3.54 | 3.0 | PASS | yes |
| input boundary | light | `--control-border` | `--control-surface` | `rgb(107, 127, 148)` | `rgb(242, 247, 252)` | 3.83 | 3.0 | PASS | yes |
| focus ring | light | `--focus` | `--paper` | `rgb(0, 87, 168)` | `rgb(255, 255, 255)` | 7.17 | 3.0 | PASS | yes |
| focus ring | light | `--focus` | `--panel` | `rgb(0, 87, 168)` | `rgb(255, 255, 255)` | 7.17 | 3.0 | PASS | yes |
| focus ring | light | `--focus` | `--soft` | `rgb(0, 87, 168)` | `rgb(237, 244, 251)` | 6.47 | 3.0 | PASS | yes |
| focus ring | light | `--focus` | `--soft-strong` | `rgb(0, 87, 168)` | `rgb(228, 239, 251)` | 6.16 | 3.0 | PASS | yes |
| focus ring | light | `--focus` | `--control-surface` | `rgb(0, 87, 168)` | `rgb(242, 247, 252)` | 6.65 | 3.0 | PASS | yes |
| Button primary boundary | light | `--accent` | `--paper` | `rgb(0, 87, 168)` | `rgb(255, 255, 255)` | 7.17 | 3.0 | PASS | yes |
| Button primary boundary | light | `--accent` | `--panel` | `rgb(0, 87, 168)` | `rgb(255, 255, 255)` | 7.17 | 3.0 | PASS | yes |
| Button primary boundary | light | `--accent` | `--soft` | `rgb(0, 87, 168)` | `rgb(237, 244, 251)` | 6.47 | 3.0 | PASS | yes |
| Button primary boundary | light | `--accent` | `--soft-strong` | `rgb(0, 87, 168)` | `rgb(228, 239, 251)` | 6.16 | 3.0 | PASS | yes |
| Button primary boundary | light | `--accent` | `--control-surface` | `rgb(0, 87, 168)` | `rgb(242, 247, 252)` | 6.65 | 3.0 | PASS | yes |
| Button danger boundary | light | `--red` | `--paper` | `rgb(163, 45, 52)` | `rgb(255, 255, 255)` | 7.04 | 3.0 | PASS | yes |
| Button danger boundary | light | `--red` | `--panel` | `rgb(163, 45, 52)` | `rgb(255, 255, 255)` | 7.04 | 3.0 | PASS | yes |
| Button danger boundary | light | `--red` | `--soft` | `rgb(163, 45, 52)` | `rgb(237, 244, 251)` | 6.35 | 3.0 | PASS | yes |
| Button danger boundary | light | `--red` | `--soft-strong` | `rgb(163, 45, 52)` | `rgb(228, 239, 251)` | 6.05 | 3.0 | PASS | yes |
| Button danger boundary | light | `--red` | `--control-surface` | `rgb(163, 45, 52)` | `rgb(242, 247, 252)` | 6.53 | 3.0 | PASS | yes |
| Pill info boundary | light | `--blue` | `--paper` | `rgb(0, 87, 168)` | `rgb(255, 255, 255)` | 7.17 | 3.0 | PASS | yes |
| Pill info boundary | light | `--blue` | `--panel` | `rgb(0, 87, 168)` | `rgb(255, 255, 255)` | 7.17 | 3.0 | PASS | yes |
| Pill info boundary | light | `--blue` | `--soft` | `rgb(0, 87, 168)` | `rgb(237, 244, 251)` | 6.47 | 3.0 | PASS | yes |
| Pill info boundary | light | `--blue` | `--soft-strong` | `rgb(0, 87, 168)` | `rgb(228, 239, 251)` | 6.16 | 3.0 | PASS | yes |
| Pill info boundary | light | `--blue` | `--control-surface` | `rgb(0, 87, 168)` | `rgb(242, 247, 252)` | 6.65 | 3.0 | PASS | yes |
| Pill ok boundary | light | `--success` | `--paper` | `rgb(0, 87, 168)` | `rgb(255, 255, 255)` | 7.17 | 3.0 | PASS | yes |
| Pill ok boundary | light | `--success` | `--panel` | `rgb(0, 87, 168)` | `rgb(255, 255, 255)` | 7.17 | 3.0 | PASS | yes |
| Pill ok boundary | light | `--success` | `--soft` | `rgb(0, 87, 168)` | `rgb(237, 244, 251)` | 6.47 | 3.0 | PASS | yes |
| Pill ok boundary | light | `--success` | `--soft-strong` | `rgb(0, 87, 168)` | `rgb(228, 239, 251)` | 6.16 | 3.0 | PASS | yes |
| Pill ok boundary | light | `--success` | `--control-surface` | `rgb(0, 87, 168)` | `rgb(242, 247, 252)` | 6.65 | 3.0 | PASS | yes |
| Pill warn boundary | light | `--amber` | `--paper` | `rgb(118, 80, 0)` | `rgb(255, 255, 255)` | 7.20 | 3.0 | PASS | yes |
| Pill warn boundary | light | `--amber` | `--panel` | `rgb(118, 80, 0)` | `rgb(255, 255, 255)` | 7.20 | 3.0 | PASS | yes |
| Pill warn boundary | light | `--amber` | `--soft` | `rgb(118, 80, 0)` | `rgb(237, 244, 251)` | 6.49 | 3.0 | PASS | yes |
| Pill warn boundary | light | `--amber` | `--soft-strong` | `rgb(118, 80, 0)` | `rgb(228, 239, 251)` | 6.18 | 3.0 | PASS | yes |
| Pill warn boundary | light | `--amber` | `--control-surface` | `rgb(118, 80, 0)` | `rgb(242, 247, 252)` | 6.68 | 3.0 | PASS | yes |
| Pill bad boundary | light | `--red` | `--paper` | `rgb(163, 45, 52)` | `rgb(255, 255, 255)` | 7.04 | 3.0 | PASS | yes |
| Pill bad boundary | light | `--red` | `--panel` | `rgb(163, 45, 52)` | `rgb(255, 255, 255)` | 7.04 | 3.0 | PASS | yes |
| Pill bad boundary | light | `--red` | `--soft` | `rgb(163, 45, 52)` | `rgb(237, 244, 251)` | 6.35 | 3.0 | PASS | yes |
| Pill bad boundary | light | `--red` | `--soft-strong` | `rgb(163, 45, 52)` | `rgb(228, 239, 251)` | 6.05 | 3.0 | PASS | yes |
| Pill bad boundary | light | `--red` | `--control-surface` | `rgb(163, 45, 52)` | `rgb(242, 247, 252)` | 6.53 | 3.0 | PASS | yes |
| app.css notice warning stripe | light | `--amber` | `--paper` | `rgb(118, 80, 0)` | `rgb(255, 255, 255)` | 7.20 | 3.0 | PASS | yes |
| app.css notice warning stripe | light | `--amber` | `--panel` | `rgb(118, 80, 0)` | `rgb(255, 255, 255)` | 7.20 | 3.0 | PASS | yes |
| app.css notice warning stripe | light | `--amber` | `--soft` | `rgb(118, 80, 0)` | `rgb(237, 244, 251)` | 6.49 | 3.0 | PASS | yes |
| app.css notice warning stripe | light | `--amber` | `--soft-strong` | `rgb(118, 80, 0)` | `rgb(228, 239, 251)` | 6.18 | 3.0 | PASS | yes |
| app.css notice warning stripe | light | `--amber` | `--control-surface` | `rgb(118, 80, 0)` | `rgb(242, 247, 252)` | 6.68 | 3.0 | PASS | yes |
| app.css notice error stripe | light | `--red` | `--paper` | `rgb(163, 45, 52)` | `rgb(255, 255, 255)` | 7.04 | 3.0 | PASS | yes |
| app.css notice error stripe | light | `--red` | `--panel` | `rgb(163, 45, 52)` | `rgb(255, 255, 255)` | 7.04 | 3.0 | PASS | yes |
| app.css notice error stripe | light | `--red` | `--soft` | `rgb(163, 45, 52)` | `rgb(237, 244, 251)` | 6.35 | 3.0 | PASS | yes |
| app.css notice error stripe | light | `--red` | `--soft-strong` | `rgb(163, 45, 52)` | `rgb(228, 239, 251)` | 6.05 | 3.0 | PASS | yes |
| app.css notice error stripe | light | `--red` | `--control-surface` | `rgb(163, 45, 52)` | `rgb(242, 247, 252)` | 6.53 | 3.0 | PASS | yes |
| app.css placeholder boundary | light | `--amber` | `--paper` | `rgb(118, 80, 0)` | `rgb(255, 255, 255)` | 7.20 | 3.0 | PASS | yes |
| app.css placeholder boundary | light | `--amber` | `--panel` | `rgb(118, 80, 0)` | `rgb(255, 255, 255)` | 7.20 | 3.0 | PASS | yes |
| app.css placeholder boundary | light | `--amber` | `--soft` | `rgb(118, 80, 0)` | `rgb(237, 244, 251)` | 6.49 | 3.0 | PASS | yes |
| app.css placeholder boundary | light | `--amber` | `--soft-strong` | `rgb(118, 80, 0)` | `rgb(228, 239, 251)` | 6.18 | 3.0 | PASS | yes |
| app.css placeholder boundary | light | `--amber` | `--control-surface` | `rgb(118, 80, 0)` | `rgb(242, 247, 252)` | 6.68 | 3.0 | PASS | yes |
| Meter meter-marker on meter-marker-ring | light | `--meter-marker` | `--meter-marker-ring` | `rgb(17, 17, 17)` | `rgb(255, 255, 255)` | 18.88 | 3.0 | PASS | yes |
| Meter meter-marker on meter-track | light | `--meter-marker` | `--meter-track` | `rgb(17, 17, 17)` | `rgb(216, 225, 235)` | 14.29 | 3.0 | PASS | yes |
| Meter meter-fill-pattern on meter-stop-1 | light | `--meter-fill-pattern` | `--meter-stop-1` | `rgb(17, 17, 17)` | `rgb(53, 170, 101)` | 6.37 | 3.0 | PASS | yes |
| Meter meter-fill-pattern on meter-stop-2 | light | `--meter-fill-pattern` | `--meter-stop-2` | `rgb(17, 17, 17)` | `rgb(137, 191, 75)` | 8.65 | 3.0 | PASS | yes |
| Meter meter-fill-pattern on meter-stop-3 | light | `--meter-fill-pattern` | `--meter-stop-3` | `rgb(17, 17, 17)` | `rgb(230, 185, 67)` | 10.24 | 3.0 | PASS | yes |
| Meter meter-fill-pattern on meter-stop-4 | light | `--meter-fill-pattern` | `--meter-stop-4` | `rgb(17, 17, 17)` | `rgb(230, 125, 54)` | 6.61 | 3.0 | PASS | yes |
| Meter meter-fill-pattern on meter-stop-5 | light | `--meter-fill-pattern` | `--meter-stop-5` | `rgb(17, 17, 17)` | `rgb(216, 64, 64)` | 4.25 | 3.0 | PASS | yes |
| Meter meter-marker on meter-stop-1 | light | `--meter-marker` | `--meter-stop-1` | `rgb(17, 17, 17)` | `rgb(53, 170, 101)` | 6.37 | 3.0 | PASS | conditional |
| Meter meter-marker on meter-stop-2 | light | `--meter-marker` | `--meter-stop-2` | `rgb(17, 17, 17)` | `rgb(137, 191, 75)` | 8.65 | 3.0 | PASS | conditional |
| Meter meter-marker on meter-stop-3 | light | `--meter-marker` | `--meter-stop-3` | `rgb(17, 17, 17)` | `rgb(230, 185, 67)` | 10.24 | 3.0 | PASS | conditional |
| Meter meter-marker on meter-stop-4 | light | `--meter-marker` | `--meter-stop-4` | `rgb(17, 17, 17)` | `rgb(230, 125, 54)` | 6.61 | 3.0 | PASS | conditional |
| Meter meter-marker on meter-stop-5 | light | `--meter-marker` | `--meter-stop-5` | `rgb(17, 17, 17)` | `rgb(216, 64, 64)` | 4.25 | 3.0 | PASS | conditional |
| Meter meter-marker-ring on meter-stop-1 | light | `--meter-marker-ring` | `--meter-stop-1` | `rgb(255, 255, 255)` | `rgb(53, 170, 101)` | 2.96 | 3.0 | FAIL | conditional |
| Meter meter-marker-ring on meter-stop-2 | light | `--meter-marker-ring` | `--meter-stop-2` | `rgb(255, 255, 255)` | `rgb(137, 191, 75)` | 2.18 | 3.0 | FAIL | conditional |
| Meter meter-marker-ring on meter-stop-3 | light | `--meter-marker-ring` | `--meter-stop-3` | `rgb(255, 255, 255)` | `rgb(230, 185, 67)` | 1.84 | 3.0 | FAIL | conditional |
| Meter meter-marker-ring on meter-stop-4 | light | `--meter-marker-ring` | `--meter-stop-4` | `rgb(255, 255, 255)` | `rgb(230, 125, 54)` | 2.86 | 3.0 | FAIL | conditional |
| Meter meter-marker-ring on meter-stop-5 | light | `--meter-marker-ring` | `--meter-stop-5` | `rgb(255, 255, 255)` | `rgb(216, 64, 64)` | 4.44 | 3.0 | PASS | conditional |
| Disclosure hover | light | `--ink` | `--soft-strong` | `rgb(17, 17, 17)` | `rgb(228, 239, 251)` | 16.22 | 4.5 | PASS | yes |
| Disclosure arrow hover | light | `--blue` | `--soft-strong` | `rgb(0, 87, 168)` | `rgb(228, 239, 251)` | 6.16 | 3.0 | PASS | yes |
| Button hover ui-btn | light | `--ink` | `--soft-strong` | `rgb(17, 17, 17)` | `rgb(228, 239, 251)` | 16.22 | 4.5 | PASS | yes |
| Button hover ui-btn ui-btn-primary | light | `--on-accent` | `--accent` | `rgb(255, 255, 255)` | `rgb(0, 87, 168)` | 7.17 | 4.5 | PASS | yes |
| Button hover ui-btn ui-btn-danger | light | `--red` | `--soft-strong` | `rgb(163, 45, 52)` | `rgb(228, 239, 251)` | 6.05 | 4.5 | PASS | yes |
| Button hover ui-btn ui-btn-quiet | light | `--blue` | `--soft-strong` | `rgb(0, 87, 168)` | `rgb(228, 239, 251)` | 6.16 | 4.5 | PASS | yes |
| Theme toggle hover | light | `--ink` | `--soft` | `rgb(17, 17, 17)` | `rgb(237, 244, 251)` | 17.03 | 4.5 | PASS | yes |
| Disclosure decorative separator | light | `--line-soft` | `--paper` | `rgb(220, 231, 242)` | `rgb(255, 255, 255)` | 1.25 | 3.0 | FAIL | no—decorative |
| Disclosure decorative separator | light | `--line-soft` | `--panel` | `rgb(220, 231, 242)` | `rgb(255, 255, 255)` | 1.25 | 3.0 | FAIL | no—decorative |
| Disclosure decorative separator | light | `--line-soft` | `--soft` | `rgb(220, 231, 242)` | `rgb(237, 244, 251)` | 1.13 | 3.0 | FAIL | no—decorative |
| Disclosure decorative separator | light | `--line-soft` | `--soft-strong` | `rgb(220, 231, 242)` | `rgb(228, 239, 251)` | 1.08 | 3.0 | FAIL | no—decorative |
| Disclosure decorative separator | light | `--line-soft` | `--control-surface` | `rgb(220, 231, 242)` | `rgb(242, 247, 252)` | 1.16 | 3.0 | FAIL | no—decorative |
| Panel decorative border | light | `--line` | `--paper` | `rgb(200, 213, 228)` | `rgb(255, 255, 255)` | 1.49 | 3.0 | FAIL | no—decorative |
| Panel decorative border | light | `--line` | `--panel` | `rgb(200, 213, 228)` | `rgb(255, 255, 255)` | 1.49 | 3.0 | FAIL | no—decorative |
| Panel decorative border | light | `--line` | `--soft` | `rgb(200, 213, 228)` | `rgb(237, 244, 251)` | 1.34 | 3.0 | FAIL | no—decorative |
| Panel decorative border | light | `--line` | `--soft-strong` | `rgb(200, 213, 228)` | `rgb(228, 239, 251)` | 1.28 | 3.0 | FAIL | no—decorative |
| Panel decorative border | light | `--line` | `--control-surface` | `rgb(200, 213, 228)` | `rgb(242, 247, 252)` | 1.38 | 3.0 | FAIL | no—decorative |
| Bubble decorative border | light | `--line` | `--paper` | `rgb(200, 213, 228)` | `rgb(255, 255, 255)` | 1.49 | 3.0 | FAIL | no—decorative |
| Bubble decorative border | light | `--line` | `--panel` | `rgb(200, 213, 228)` | `rgb(255, 255, 255)` | 1.49 | 3.0 | FAIL | no—decorative |
| Bubble decorative border | light | `--line` | `--soft` | `rgb(200, 213, 228)` | `rgb(237, 244, 251)` | 1.34 | 3.0 | FAIL | no—decorative |
| Bubble decorative border | light | `--line` | `--soft-strong` | `rgb(200, 213, 228)` | `rgb(228, 239, 251)` | 1.28 | 3.0 | FAIL | no—decorative |
| Bubble decorative border | light | `--line` | `--control-surface` | `rgb(200, 213, 228)` | `rgb(242, 247, 252)` | 1.38 | 3.0 | FAIL | no—decorative |
| app.css pre decorative border | light | `--line` | `--paper` | `rgb(200, 213, 228)` | `rgb(255, 255, 255)` | 1.49 | 3.0 | FAIL | no—decorative |
| app.css pre decorative border | light | `--line` | `--panel` | `rgb(200, 213, 228)` | `rgb(255, 255, 255)` | 1.49 | 3.0 | FAIL | no—decorative |
| app.css pre decorative border | light | `--line` | `--soft` | `rgb(200, 213, 228)` | `rgb(237, 244, 251)` | 1.34 | 3.0 | FAIL | no—decorative |
| app.css pre decorative border | light | `--line` | `--soft-strong` | `rgb(200, 213, 228)` | `rgb(228, 239, 251)` | 1.28 | 3.0 | FAIL | no—decorative |
| app.css pre decorative border | light | `--line` | `--control-surface` | `rgb(200, 213, 228)` | `rgb(242, 247, 252)` | 1.38 | 3.0 | FAIL | no—decorative |
| app.css strip decorative separator | light | `--line` | `--paper` | `rgb(200, 213, 228)` | `rgb(255, 255, 255)` | 1.49 | 3.0 | FAIL | no—decorative |
| app.css strip decorative separator | light | `--line` | `--panel` | `rgb(200, 213, 228)` | `rgb(255, 255, 255)` | 1.49 | 3.0 | FAIL | no—decorative |
| app.css strip decorative separator | light | `--line` | `--soft` | `rgb(200, 213, 228)` | `rgb(237, 244, 251)` | 1.34 | 3.0 | FAIL | no—decorative |
| app.css strip decorative separator | light | `--line` | `--soft-strong` | `rgb(200, 213, 228)` | `rgb(228, 239, 251)` | 1.28 | 3.0 | FAIL | no—decorative |
| app.css strip decorative separator | light | `--line` | `--control-surface` | `rgb(200, 213, 228)` | `rgb(242, 247, 252)` | 1.38 | 3.0 | FAIL | no—decorative |
| app.css table decorative separator | light | `--line` | `--paper` | `rgb(200, 213, 228)` | `rgb(255, 255, 255)` | 1.49 | 3.0 | FAIL | no—decorative |
| app.css table decorative separator | light | `--line` | `--panel` | `rgb(200, 213, 228)` | `rgb(255, 255, 255)` | 1.49 | 3.0 | FAIL | no—decorative |
| app.css table decorative separator | light | `--line` | `--soft` | `rgb(200, 213, 228)` | `rgb(237, 244, 251)` | 1.34 | 3.0 | FAIL | no—decorative |
| app.css table decorative separator | light | `--line` | `--soft-strong` | `rgb(200, 213, 228)` | `rgb(228, 239, 251)` | 1.28 | 3.0 | FAIL | no—decorative |
| app.css table decorative separator | light | `--line` | `--control-surface` | `rgb(200, 213, 228)` | `rgb(242, 247, 252)` | 1.38 | 3.0 | FAIL | no—decorative |
| Disclosure closed | dark | `--ink` | `--paper` | `rgb(242, 246, 251)` | `rgb(16, 26, 37)` | 16.17 | 4.5 | PASS | yes |
| Disclosure closed | dark | `--ink` | `--panel` | `rgb(242, 246, 251)` | `rgb(23, 36, 51)` | 14.48 | 4.5 | PASS | yes |
| Disclosure closed | dark | `--ink` | `--soft` | `rgb(242, 246, 251)` | `rgb(32, 54, 74)` | 11.46 | 4.5 | PASS | yes |
| Disclosure closed | dark | `--ink` | `--soft-strong` | `rgb(242, 246, 251)` | `rgb(48, 74, 101)` | 8.44 | 4.5 | PASS | yes |
| Disclosure closed | dark | `--ink` | `--control-surface` | `rgb(242, 246, 251)` | `rgb(32, 54, 74)` | 11.46 | 4.5 | PASS | yes |
| Disclosure open | dark | `--ink` | `--soft-strong` | `rgb(242, 246, 251)` | `rgb(48, 74, 101)` | 8.44 | 4.5 | PASS | yes |
| Disclosure arrow closed | dark | `--blue` | `--paper` | `rgb(160, 206, 255)` | `rgb(16, 26, 37)` | 10.67 | 3.0 | PASS | yes |
| Disclosure arrow closed | dark | `--blue` | `--panel` | `rgb(160, 206, 255)` | `rgb(23, 36, 51)` | 9.55 | 3.0 | PASS | yes |
| Disclosure arrow closed | dark | `--blue` | `--soft` | `rgb(160, 206, 255)` | `rgb(32, 54, 74)` | 7.56 | 3.0 | PASS | yes |
| Disclosure arrow closed | dark | `--blue` | `--soft-strong` | `rgb(160, 206, 255)` | `rgb(48, 74, 101)` | 5.57 | 3.0 | PASS | yes |
| Disclosure arrow closed | dark | `--blue` | `--control-surface` | `rgb(160, 206, 255)` | `rgb(32, 54, 74)` | 7.56 | 3.0 | PASS | yes |
| Disclosure arrow open | dark | `--blue` | `--soft-strong` | `rgb(160, 206, 255)` | `rgb(48, 74, 101)` | 5.57 | 3.0 | PASS | yes |
| ui-pill neutral | dark | `--ink` | `--soft` | `rgb(242, 246, 251)` | `rgb(32, 54, 74)` | 11.46 | 4.5 | PASS | yes |
| ui-badge neutral | dark | `--ink` | `--soft` | `rgb(242, 246, 251)` | `rgb(32, 54, 74)` | 11.46 | 4.5 | PASS | yes |
| ui-pill info | dark | `--blue` | `--blue-bg` | `rgb(160, 206, 255)` | `rgb(34, 63, 91)` | 6.61 | 4.5 | PASS | yes |
| ui-badge info | dark | `--blue` | `--blue-bg` | `rgb(160, 206, 255)` | `rgb(34, 63, 91)` | 6.61 | 4.5 | PASS | yes |
| ui-pill ok | dark | `--success` | `--success-bg` | `rgb(160, 206, 255)` | `rgb(34, 63, 91)` | 6.61 | 4.5 | PASS | yes |
| ui-badge ok | dark | `--success` | `--success-bg` | `rgb(160, 206, 255)` | `rgb(34, 63, 91)` | 6.61 | 4.5 | PASS | yes |
| ui-pill warn | dark | `--amber` | `--amber-bg` | `rgb(255, 219, 149)` | `rgb(74, 58, 32)` | 8.25 | 4.5 | PASS | yes |
| ui-badge warn | dark | `--amber` | `--amber-bg` | `rgb(255, 219, 149)` | `rgb(74, 58, 32)` | 8.25 | 4.5 | PASS | yes |
| ui-pill placeholder | dark | `--amber` | `--amber-bg` | `rgb(255, 219, 149)` | `rgb(74, 58, 32)` | 8.25 | 4.5 | PASS | yes |
| ui-badge placeholder | dark | `--amber` | `--amber-bg` | `rgb(255, 219, 149)` | `rgb(74, 58, 32)` | 8.25 | 4.5 | PASS | yes |
| ui-pill bad | dark | `--red` | `--red-bg` | `rgb(255, 189, 194)` | `rgb(80, 49, 55)` | 7.25 | 4.5 | PASS | yes |
| ui-badge bad | dark | `--red` | `--red-bg` | `rgb(255, 189, 194)` | `rgb(80, 49, 55)` | 7.25 | 4.5 | PASS | yes |
| Button ui-btn | dark | `--ink` | `--panel` | `rgb(242, 246, 251)` | `rgb(23, 36, 51)` | 14.48 | 4.5 | PASS | yes |
| Button ui-btn ui-btn-primary | dark | `--on-accent` | `--accent` | `rgb(16, 26, 37)` | `rgb(160, 206, 255)` | 10.67 | 4.5 | PASS | yes |
| Button ui-btn ui-btn-danger | dark | `--red` | `--red-bg` | `rgb(255, 189, 194)` | `rgb(80, 49, 55)` | 7.25 | 4.5 | PASS | yes |
| Button quiet | dark | `--blue` | `--paper` | `rgb(160, 206, 255)` | `rgb(16, 26, 37)` | 10.67 | 4.5 | PASS | yes |
| Button disabled | dark | `--subtle` | `--paper` | `rgb(194, 210, 227)` | `rgb(16, 26, 37)` | 11.38 | 4.5 | PASS | yes |
| Button quiet | dark | `--blue` | `--panel` | `rgb(160, 206, 255)` | `rgb(23, 36, 51)` | 9.55 | 4.5 | PASS | yes |
| Button disabled | dark | `--subtle` | `--panel` | `rgb(194, 210, 227)` | `rgb(23, 36, 51)` | 10.19 | 4.5 | PASS | yes |
| Button quiet | dark | `--blue` | `--soft` | `rgb(160, 206, 255)` | `rgb(32, 54, 74)` | 7.56 | 4.5 | PASS | yes |
| Button disabled | dark | `--subtle` | `--soft` | `rgb(194, 210, 227)` | `rgb(32, 54, 74)` | 8.07 | 4.5 | PASS | yes |
| Button quiet | dark | `--blue` | `--soft-strong` | `rgb(160, 206, 255)` | `rgb(48, 74, 101)` | 5.57 | 4.5 | PASS | yes |
| Button disabled | dark | `--subtle` | `--soft-strong` | `rgb(194, 210, 227)` | `rgb(48, 74, 101)` | 5.94 | 4.5 | PASS | yes |
| Button quiet | dark | `--blue` | `--control-surface` | `rgb(160, 206, 255)` | `rgb(32, 54, 74)` | 7.56 | 4.5 | PASS | yes |
| Button disabled | dark | `--subtle` | `--control-surface` | `rgb(194, 210, 227)` | `rgb(32, 54, 74)` | 8.07 | 4.5 | PASS | yes |
| Panel title | dark | `--ink` | `--panel` | `rgb(242, 246, 251)` | `rgb(23, 36, 51)` | 14.48 | 4.5 | PASS | yes |
| Bubble ui-bubble | dark | `--ink` | `--soft` | `rgb(242, 246, 251)` | `rgb(32, 54, 74)` | 11.46 | 4.5 | PASS | yes |
| Bubble metadata ui-bubble | dark | `--subtle` | `--soft` | `rgb(194, 210, 227)` | `rgb(32, 54, 74)` | 8.07 | 4.5 | PASS | yes |
| Bubble ui-bubble ui-bubble-human | dark | `--ink` | `--panel` | `rgb(242, 246, 251)` | `rgb(23, 36, 51)` | 14.48 | 4.5 | PASS | yes |
| Bubble metadata ui-bubble ui-bubble-human | dark | `--subtle` | `--panel` | `rgb(194, 210, 227)` | `rgb(23, 36, 51)` | 10.19 | 4.5 | PASS | yes |
| EmptyState title | dark | `--ink` | `--paper` | `rgb(242, 246, 251)` | `rgb(16, 26, 37)` | 16.17 | 4.5 | PASS | yes |
| EmptyState title | dark | `--ink` | `--panel` | `rgb(242, 246, 251)` | `rgb(23, 36, 51)` | 14.48 | 4.5 | PASS | yes |
| EmptyState title | dark | `--ink` | `--soft` | `rgb(242, 246, 251)` | `rgb(32, 54, 74)` | 11.46 | 4.5 | PASS | yes |
| EmptyState title | dark | `--ink` | `--soft-strong` | `rgb(242, 246, 251)` | `rgb(48, 74, 101)` | 8.44 | 4.5 | PASS | yes |
| EmptyState title | dark | `--ink` | `--control-surface` | `rgb(242, 246, 251)` | `rgb(32, 54, 74)` | 11.46 | 4.5 | PASS | yes |
| EmptyState description | dark | `--subtle` | `--paper` | `rgb(194, 210, 227)` | `rgb(16, 26, 37)` | 11.38 | 4.5 | PASS | yes |
| EmptyState description | dark | `--subtle` | `--panel` | `rgb(194, 210, 227)` | `rgb(23, 36, 51)` | 10.19 | 4.5 | PASS | yes |
| EmptyState description | dark | `--subtle` | `--soft` | `rgb(194, 210, 227)` | `rgb(32, 54, 74)` | 8.07 | 4.5 | PASS | yes |
| EmptyState description | dark | `--subtle` | `--soft-strong` | `rgb(194, 210, 227)` | `rgb(48, 74, 101)` | 5.94 | 4.5 | PASS | yes |
| EmptyState description | dark | `--subtle` | `--control-surface` | `rgb(194, 210, 227)` | `rgb(32, 54, 74)` | 8.07 | 4.5 | PASS | yes |
| Meter label | dark | `--ink` | `--paper` | `rgb(242, 246, 251)` | `rgb(16, 26, 37)` | 16.17 | 4.5 | PASS | yes |
| Meter label | dark | `--ink` | `--panel` | `rgb(242, 246, 251)` | `rgb(23, 36, 51)` | 14.48 | 4.5 | PASS | yes |
| Meter label | dark | `--ink` | `--soft` | `rgb(242, 246, 251)` | `rgb(32, 54, 74)` | 11.46 | 4.5 | PASS | yes |
| Meter label | dark | `--ink` | `--soft-strong` | `rgb(242, 246, 251)` | `rgb(48, 74, 101)` | 8.44 | 4.5 | PASS | yes |
| Meter label | dark | `--ink` | `--control-surface` | `rgb(242, 246, 251)` | `rgb(32, 54, 74)` | 11.46 | 4.5 | PASS | yes |
| Meter value | dark | `--ink` | `--paper` | `rgb(242, 246, 251)` | `rgb(16, 26, 37)` | 16.17 | 4.5 | PASS | yes |
| Meter value | dark | `--ink` | `--panel` | `rgb(242, 246, 251)` | `rgb(23, 36, 51)` | 14.48 | 4.5 | PASS | yes |
| Meter value | dark | `--ink` | `--soft` | `rgb(242, 246, 251)` | `rgb(32, 54, 74)` | 11.46 | 4.5 | PASS | yes |
| Meter value | dark | `--ink` | `--soft-strong` | `rgb(242, 246, 251)` | `rgb(48, 74, 101)` | 8.44 | 4.5 | PASS | yes |
| Meter value | dark | `--ink` | `--control-surface` | `rgb(242, 246, 251)` | `rgb(32, 54, 74)` | 11.46 | 4.5 | PASS | yes |
| Theme toggle | dark | `--ink` | `--panel` | `rgb(242, 246, 251)` | `rgb(23, 36, 51)` | 14.48 | 4.5 | PASS | yes |
| Theme toggle state | dark | `--subtle` | `--panel` | `rgb(194, 210, 227)` | `rgb(23, 36, 51)` | 10.19 | 4.5 | PASS | yes |
| app.css pill | dark | `--ink` | `--soft` | `rgb(242, 246, 251)` | `rgb(32, 54, 74)` | 11.46 | 4.5 | PASS | yes |
| app.css pill pill-connected | dark | `--success` | `--success-bg` | `rgb(160, 206, 255)` | `rgb(34, 63, 91)` | 6.61 | 4.5 | PASS | yes |
| app.css pill pill-reconnecting | dark | `--amber` | `--amber-bg` | `rgb(255, 219, 149)` | `rgb(74, 58, 32)` | 8.25 | 4.5 | PASS | yes |
| app.css pill pill-disconnected | dark | `--red` | `--red-bg` | `rgb(255, 189, 194)` | `rgb(80, 49, 55)` | 7.25 | 4.5 | PASS | yes |
| app.css notice | dark | `--ink` | `--amber-bg` | `rgb(242, 246, 251)` | `rgb(74, 58, 32)` | 10.09 | 4.5 | PASS | yes |
| app.css notice notice-error | dark | `--ink` | `--red-bg` | `rgb(242, 246, 251)` | `rgb(80, 49, 55)` | 10.54 | 4.5 | PASS | yes |
| app.css placeholder | dark | `--ink` | `--amber-bg` | `rgb(242, 246, 251)` | `rgb(74, 58, 32)` | 10.09 | 4.5 | PASS | yes |
| app.css badge | dark | `--amber` | `--amber-bg` | `rgb(255, 219, 149)` | `rgb(74, 58, 32)` | 8.25 | 4.5 | PASS | yes |
| app.css placeholder-tag | dark | `--amber` | `--amber-bg` | `rgb(255, 219, 149)` | `rgb(74, 58, 32)` | 8.25 | 4.5 | PASS | yes |
| app.css label | dark | `--component-muted` | `--paper` | `rgb(194, 210, 227)` | `rgb(16, 26, 37)` | 11.38 | 4.5 | PASS | yes |
| app.css label | dark | `--component-muted` | `--panel` | `rgb(194, 210, 227)` | `rgb(23, 36, 51)` | 10.19 | 4.5 | PASS | yes |
| app.css label | dark | `--component-muted` | `--soft` | `rgb(194, 210, 227)` | `rgb(32, 54, 74)` | 8.07 | 4.5 | PASS | yes |
| app.css label | dark | `--component-muted` | `--soft-strong` | `rgb(194, 210, 227)` | `rgb(48, 74, 101)` | 5.94 | 4.5 | PASS | yes |
| app.css label | dark | `--component-muted` | `--control-surface` | `rgb(194, 210, 227)` | `rgb(32, 54, 74)` | 8.07 | 4.5 | PASS | yes |
| app.css muted | dark | `--component-muted` | `--paper` | `rgb(194, 210, 227)` | `rgb(16, 26, 37)` | 11.38 | 4.5 | PASS | yes |
| app.css muted | dark | `--component-muted` | `--panel` | `rgb(194, 210, 227)` | `rgb(23, 36, 51)` | 10.19 | 4.5 | PASS | yes |
| app.css muted | dark | `--component-muted` | `--soft` | `rgb(194, 210, 227)` | `rgb(32, 54, 74)` | 8.07 | 4.5 | PASS | yes |
| app.css muted | dark | `--component-muted` | `--soft-strong` | `rgb(194, 210, 227)` | `rgb(48, 74, 101)` | 5.94 | 4.5 | PASS | yes |
| app.css muted | dark | `--component-muted` | `--control-surface` | `rgb(194, 210, 227)` | `rgb(32, 54, 74)` | 8.07 | 4.5 | PASS | yes |
| app.css input text | dark | `--ink` | `--panel` | `rgb(242, 246, 251)` | `rgb(23, 36, 51)` | 14.48 | 4.5 | PASS | yes |
| app.css button | dark | `--ink` | `--panel` | `rgb(242, 246, 251)` | `rgb(23, 36, 51)` | 14.48 | 4.5 | PASS | yes |
| app.css disabled button | dark | `--subtle` | `--panel` | `rgb(194, 210, 227)` | `rgb(23, 36, 51)` | 10.19 | 4.5 | PASS | yes |
| app.css pre | dark | `--ink` | `--soft` | `rgb(242, 246, 251)` | `rgb(32, 54, 74)` | 11.46 | 4.5 | PASS | yes |
| app.css strip | dark | `--ink` | `--soft` | `rgb(242, 246, 251)` | `rgb(32, 54, 74)` | 11.46 | 4.5 | PASS | yes |
| Button boundary | dark | `--control-border` | `--paper` | `rgb(130, 154, 176)` | `rgb(16, 26, 37)` | 6.02 | 3.0 | PASS | yes |
| Button boundary | dark | `--control-border` | `--panel` | `rgb(130, 154, 176)` | `rgb(23, 36, 51)` | 5.39 | 3.0 | PASS | yes |
| Button boundary | dark | `--control-border` | `--soft` | `rgb(130, 154, 176)` | `rgb(32, 54, 74)` | 4.26 | 3.0 | PASS | yes |
| Button boundary | dark | `--control-border` | `--soft-strong` | `rgb(130, 154, 176)` | `rgb(48, 74, 101)` | 3.14 | 3.0 | PASS | yes |
| Button boundary | dark | `--control-border` | `--control-surface` | `rgb(130, 154, 176)` | `rgb(32, 54, 74)` | 4.26 | 3.0 | PASS | yes |
| EmptyState boundary | dark | `--control-border` | `--paper` | `rgb(130, 154, 176)` | `rgb(16, 26, 37)` | 6.02 | 3.0 | PASS | yes |
| EmptyState boundary | dark | `--control-border` | `--panel` | `rgb(130, 154, 176)` | `rgb(23, 36, 51)` | 5.39 | 3.0 | PASS | yes |
| EmptyState boundary | dark | `--control-border` | `--soft` | `rgb(130, 154, 176)` | `rgb(32, 54, 74)` | 4.26 | 3.0 | PASS | yes |
| EmptyState boundary | dark | `--control-border` | `--soft-strong` | `rgb(130, 154, 176)` | `rgb(48, 74, 101)` | 3.14 | 3.0 | PASS | yes |
| EmptyState boundary | dark | `--control-border` | `--control-surface` | `rgb(130, 154, 176)` | `rgb(32, 54, 74)` | 4.26 | 3.0 | PASS | yes |
| Meter boundary | dark | `--control-border` | `--paper` | `rgb(130, 154, 176)` | `rgb(16, 26, 37)` | 6.02 | 3.0 | PASS | yes |
| Meter boundary | dark | `--control-border` | `--panel` | `rgb(130, 154, 176)` | `rgb(23, 36, 51)` | 5.39 | 3.0 | PASS | yes |
| Meter boundary | dark | `--control-border` | `--soft` | `rgb(130, 154, 176)` | `rgb(32, 54, 74)` | 4.26 | 3.0 | PASS | yes |
| Meter boundary | dark | `--control-border` | `--soft-strong` | `rgb(130, 154, 176)` | `rgb(48, 74, 101)` | 3.14 | 3.0 | PASS | yes |
| Meter boundary | dark | `--control-border` | `--control-surface` | `rgb(130, 154, 176)` | `rgb(32, 54, 74)` | 4.26 | 3.0 | PASS | yes |
| input boundary | dark | `--control-border` | `--paper` | `rgb(130, 154, 176)` | `rgb(16, 26, 37)` | 6.02 | 3.0 | PASS | yes |
| input boundary | dark | `--control-border` | `--panel` | `rgb(130, 154, 176)` | `rgb(23, 36, 51)` | 5.39 | 3.0 | PASS | yes |
| input boundary | dark | `--control-border` | `--soft` | `rgb(130, 154, 176)` | `rgb(32, 54, 74)` | 4.26 | 3.0 | PASS | yes |
| input boundary | dark | `--control-border` | `--soft-strong` | `rgb(130, 154, 176)` | `rgb(48, 74, 101)` | 3.14 | 3.0 | PASS | yes |
| input boundary | dark | `--control-border` | `--control-surface` | `rgb(130, 154, 176)` | `rgb(32, 54, 74)` | 4.26 | 3.0 | PASS | yes |
| focus ring | dark | `--focus` | `--paper` | `rgb(160, 206, 255)` | `rgb(16, 26, 37)` | 10.67 | 3.0 | PASS | yes |
| focus ring | dark | `--focus` | `--panel` | `rgb(160, 206, 255)` | `rgb(23, 36, 51)` | 9.55 | 3.0 | PASS | yes |
| focus ring | dark | `--focus` | `--soft` | `rgb(160, 206, 255)` | `rgb(32, 54, 74)` | 7.56 | 3.0 | PASS | yes |
| focus ring | dark | `--focus` | `--soft-strong` | `rgb(160, 206, 255)` | `rgb(48, 74, 101)` | 5.57 | 3.0 | PASS | yes |
| focus ring | dark | `--focus` | `--control-surface` | `rgb(160, 206, 255)` | `rgb(32, 54, 74)` | 7.56 | 3.0 | PASS | yes |
| Button primary boundary | dark | `--accent` | `--paper` | `rgb(160, 206, 255)` | `rgb(16, 26, 37)` | 10.67 | 3.0 | PASS | yes |
| Button primary boundary | dark | `--accent` | `--panel` | `rgb(160, 206, 255)` | `rgb(23, 36, 51)` | 9.55 | 3.0 | PASS | yes |
| Button primary boundary | dark | `--accent` | `--soft` | `rgb(160, 206, 255)` | `rgb(32, 54, 74)` | 7.56 | 3.0 | PASS | yes |
| Button primary boundary | dark | `--accent` | `--soft-strong` | `rgb(160, 206, 255)` | `rgb(48, 74, 101)` | 5.57 | 3.0 | PASS | yes |
| Button primary boundary | dark | `--accent` | `--control-surface` | `rgb(160, 206, 255)` | `rgb(32, 54, 74)` | 7.56 | 3.0 | PASS | yes |
| Button danger boundary | dark | `--red` | `--paper` | `rgb(255, 189, 194)` | `rgb(16, 26, 37)` | 11.12 | 3.0 | PASS | yes |
| Button danger boundary | dark | `--red` | `--panel` | `rgb(255, 189, 194)` | `rgb(23, 36, 51)` | 9.96 | 3.0 | PASS | yes |
| Button danger boundary | dark | `--red` | `--soft` | `rgb(255, 189, 194)` | `rgb(32, 54, 74)` | 7.89 | 3.0 | PASS | yes |
| Button danger boundary | dark | `--red` | `--soft-strong` | `rgb(255, 189, 194)` | `rgb(48, 74, 101)` | 5.80 | 3.0 | PASS | yes |
| Button danger boundary | dark | `--red` | `--control-surface` | `rgb(255, 189, 194)` | `rgb(32, 54, 74)` | 7.89 | 3.0 | PASS | yes |
| Pill info boundary | dark | `--blue` | `--paper` | `rgb(160, 206, 255)` | `rgb(16, 26, 37)` | 10.67 | 3.0 | PASS | yes |
| Pill info boundary | dark | `--blue` | `--panel` | `rgb(160, 206, 255)` | `rgb(23, 36, 51)` | 9.55 | 3.0 | PASS | yes |
| Pill info boundary | dark | `--blue` | `--soft` | `rgb(160, 206, 255)` | `rgb(32, 54, 74)` | 7.56 | 3.0 | PASS | yes |
| Pill info boundary | dark | `--blue` | `--soft-strong` | `rgb(160, 206, 255)` | `rgb(48, 74, 101)` | 5.57 | 3.0 | PASS | yes |
| Pill info boundary | dark | `--blue` | `--control-surface` | `rgb(160, 206, 255)` | `rgb(32, 54, 74)` | 7.56 | 3.0 | PASS | yes |
| Pill ok boundary | dark | `--success` | `--paper` | `rgb(160, 206, 255)` | `rgb(16, 26, 37)` | 10.67 | 3.0 | PASS | yes |
| Pill ok boundary | dark | `--success` | `--panel` | `rgb(160, 206, 255)` | `rgb(23, 36, 51)` | 9.55 | 3.0 | PASS | yes |
| Pill ok boundary | dark | `--success` | `--soft` | `rgb(160, 206, 255)` | `rgb(32, 54, 74)` | 7.56 | 3.0 | PASS | yes |
| Pill ok boundary | dark | `--success` | `--soft-strong` | `rgb(160, 206, 255)` | `rgb(48, 74, 101)` | 5.57 | 3.0 | PASS | yes |
| Pill ok boundary | dark | `--success` | `--control-surface` | `rgb(160, 206, 255)` | `rgb(32, 54, 74)` | 7.56 | 3.0 | PASS | yes |
| Pill warn boundary | dark | `--amber` | `--paper` | `rgb(255, 219, 149)` | `rgb(16, 26, 37)` | 13.22 | 3.0 | PASS | yes |
| Pill warn boundary | dark | `--amber` | `--panel` | `rgb(255, 219, 149)` | `rgb(23, 36, 51)` | 11.84 | 3.0 | PASS | yes |
| Pill warn boundary | dark | `--amber` | `--soft` | `rgb(255, 219, 149)` | `rgb(32, 54, 74)` | 9.37 | 3.0 | PASS | yes |
| Pill warn boundary | dark | `--amber` | `--soft-strong` | `rgb(255, 219, 149)` | `rgb(48, 74, 101)` | 6.90 | 3.0 | PASS | yes |
| Pill warn boundary | dark | `--amber` | `--control-surface` | `rgb(255, 219, 149)` | `rgb(32, 54, 74)` | 9.37 | 3.0 | PASS | yes |
| Pill bad boundary | dark | `--red` | `--paper` | `rgb(255, 189, 194)` | `rgb(16, 26, 37)` | 11.12 | 3.0 | PASS | yes |
| Pill bad boundary | dark | `--red` | `--panel` | `rgb(255, 189, 194)` | `rgb(23, 36, 51)` | 9.96 | 3.0 | PASS | yes |
| Pill bad boundary | dark | `--red` | `--soft` | `rgb(255, 189, 194)` | `rgb(32, 54, 74)` | 7.89 | 3.0 | PASS | yes |
| Pill bad boundary | dark | `--red` | `--soft-strong` | `rgb(255, 189, 194)` | `rgb(48, 74, 101)` | 5.80 | 3.0 | PASS | yes |
| Pill bad boundary | dark | `--red` | `--control-surface` | `rgb(255, 189, 194)` | `rgb(32, 54, 74)` | 7.89 | 3.0 | PASS | yes |
| app.css notice warning stripe | dark | `--amber` | `--paper` | `rgb(255, 219, 149)` | `rgb(16, 26, 37)` | 13.22 | 3.0 | PASS | yes |
| app.css notice warning stripe | dark | `--amber` | `--panel` | `rgb(255, 219, 149)` | `rgb(23, 36, 51)` | 11.84 | 3.0 | PASS | yes |
| app.css notice warning stripe | dark | `--amber` | `--soft` | `rgb(255, 219, 149)` | `rgb(32, 54, 74)` | 9.37 | 3.0 | PASS | yes |
| app.css notice warning stripe | dark | `--amber` | `--soft-strong` | `rgb(255, 219, 149)` | `rgb(48, 74, 101)` | 6.90 | 3.0 | PASS | yes |
| app.css notice warning stripe | dark | `--amber` | `--control-surface` | `rgb(255, 219, 149)` | `rgb(32, 54, 74)` | 9.37 | 3.0 | PASS | yes |
| app.css notice error stripe | dark | `--red` | `--paper` | `rgb(255, 189, 194)` | `rgb(16, 26, 37)` | 11.12 | 3.0 | PASS | yes |
| app.css notice error stripe | dark | `--red` | `--panel` | `rgb(255, 189, 194)` | `rgb(23, 36, 51)` | 9.96 | 3.0 | PASS | yes |
| app.css notice error stripe | dark | `--red` | `--soft` | `rgb(255, 189, 194)` | `rgb(32, 54, 74)` | 7.89 | 3.0 | PASS | yes |
| app.css notice error stripe | dark | `--red` | `--soft-strong` | `rgb(255, 189, 194)` | `rgb(48, 74, 101)` | 5.80 | 3.0 | PASS | yes |
| app.css notice error stripe | dark | `--red` | `--control-surface` | `rgb(255, 189, 194)` | `rgb(32, 54, 74)` | 7.89 | 3.0 | PASS | yes |
| app.css placeholder boundary | dark | `--amber` | `--paper` | `rgb(255, 219, 149)` | `rgb(16, 26, 37)` | 13.22 | 3.0 | PASS | yes |
| app.css placeholder boundary | dark | `--amber` | `--panel` | `rgb(255, 219, 149)` | `rgb(23, 36, 51)` | 11.84 | 3.0 | PASS | yes |
| app.css placeholder boundary | dark | `--amber` | `--soft` | `rgb(255, 219, 149)` | `rgb(32, 54, 74)` | 9.37 | 3.0 | PASS | yes |
| app.css placeholder boundary | dark | `--amber` | `--soft-strong` | `rgb(255, 219, 149)` | `rgb(48, 74, 101)` | 6.90 | 3.0 | PASS | yes |
| app.css placeholder boundary | dark | `--amber` | `--control-surface` | `rgb(255, 219, 149)` | `rgb(32, 54, 74)` | 9.37 | 3.0 | PASS | yes |
| Meter meter-marker on meter-marker-ring | dark | `--meter-marker` | `--meter-marker-ring` | `rgb(242, 246, 251)` | `rgb(16, 26, 37)` | 16.17 | 3.0 | PASS | yes |
| Meter meter-marker on meter-track | dark | `--meter-marker` | `--meter-track` | `rgb(242, 246, 251)` | `rgb(64, 87, 108)` | 6.92 | 3.0 | PASS | yes |
| Meter meter-fill-pattern on meter-stop-1 | dark | `--meter-fill-pattern` | `--meter-stop-1` | `rgb(17, 17, 17)` | `rgb(53, 170, 101)` | 6.37 | 3.0 | PASS | yes |
| Meter meter-fill-pattern on meter-stop-2 | dark | `--meter-fill-pattern` | `--meter-stop-2` | `rgb(17, 17, 17)` | `rgb(137, 191, 75)` | 8.65 | 3.0 | PASS | yes |
| Meter meter-fill-pattern on meter-stop-3 | dark | `--meter-fill-pattern` | `--meter-stop-3` | `rgb(17, 17, 17)` | `rgb(230, 185, 67)` | 10.24 | 3.0 | PASS | yes |
| Meter meter-fill-pattern on meter-stop-4 | dark | `--meter-fill-pattern` | `--meter-stop-4` | `rgb(17, 17, 17)` | `rgb(230, 125, 54)` | 6.61 | 3.0 | PASS | yes |
| Meter meter-fill-pattern on meter-stop-5 | dark | `--meter-fill-pattern` | `--meter-stop-5` | `rgb(17, 17, 17)` | `rgb(216, 64, 64)` | 4.25 | 3.0 | PASS | yes |
| Meter meter-marker on meter-stop-1 | dark | `--meter-marker` | `--meter-stop-1` | `rgb(242, 246, 251)` | `rgb(53, 170, 101)` | 2.73 | 3.0 | FAIL | conditional |
| Meter meter-marker on meter-stop-2 | dark | `--meter-marker` | `--meter-stop-2` | `rgb(242, 246, 251)` | `rgb(137, 191, 75)` | 2.01 | 3.0 | FAIL | conditional |
| Meter meter-marker on meter-stop-3 | dark | `--meter-marker` | `--meter-stop-3` | `rgb(242, 246, 251)` | `rgb(230, 185, 67)` | 1.70 | 3.0 | FAIL | conditional |
| Meter meter-marker on meter-stop-4 | dark | `--meter-marker` | `--meter-stop-4` | `rgb(242, 246, 251)` | `rgb(230, 125, 54)` | 2.63 | 3.0 | FAIL | conditional |
| Meter meter-marker on meter-stop-5 | dark | `--meter-marker` | `--meter-stop-5` | `rgb(242, 246, 251)` | `rgb(216, 64, 64)` | 4.09 | 3.0 | PASS | conditional |
| Meter meter-marker-ring on meter-stop-1 | dark | `--meter-marker-ring` | `--meter-stop-1` | `rgb(16, 26, 37)` | `rgb(53, 170, 101)` | 5.92 | 3.0 | PASS | conditional |
| Meter meter-marker-ring on meter-stop-2 | dark | `--meter-marker-ring` | `--meter-stop-2` | `rgb(16, 26, 37)` | `rgb(137, 191, 75)` | 8.04 | 3.0 | PASS | conditional |
| Meter meter-marker-ring on meter-stop-3 | dark | `--meter-marker-ring` | `--meter-stop-3` | `rgb(16, 26, 37)` | `rgb(230, 185, 67)` | 9.52 | 3.0 | PASS | conditional |
| Meter meter-marker-ring on meter-stop-4 | dark | `--meter-marker-ring` | `--meter-stop-4` | `rgb(16, 26, 37)` | `rgb(230, 125, 54)` | 6.14 | 3.0 | PASS | conditional |
| Meter meter-marker-ring on meter-stop-5 | dark | `--meter-marker-ring` | `--meter-stop-5` | `rgb(16, 26, 37)` | `rgb(216, 64, 64)` | 3.95 | 3.0 | PASS | conditional |
| Disclosure hover | dark | `--ink` | `--soft-strong` | `rgb(242, 246, 251)` | `rgb(48, 74, 101)` | 8.44 | 4.5 | PASS | yes |
| Disclosure arrow hover | dark | `--blue` | `--soft-strong` | `rgb(160, 206, 255)` | `rgb(48, 74, 101)` | 5.57 | 3.0 | PASS | yes |
| Button hover ui-btn | dark | `--ink` | `--soft-strong` | `rgb(242, 246, 251)` | `rgb(48, 74, 101)` | 8.44 | 4.5 | PASS | yes |
| Button hover ui-btn ui-btn-primary | dark | `--on-accent` | `--accent` | `rgb(16, 26, 37)` | `rgb(160, 206, 255)` | 10.67 | 4.5 | PASS | yes |
| Button hover ui-btn ui-btn-danger | dark | `--red` | `--soft-strong` | `rgb(255, 189, 194)` | `rgb(48, 74, 101)` | 5.80 | 4.5 | PASS | yes |
| Button hover ui-btn ui-btn-quiet | dark | `--blue` | `--soft-strong` | `rgb(160, 206, 255)` | `rgb(48, 74, 101)` | 5.57 | 4.5 | PASS | yes |
| Theme toggle hover | dark | `--ink` | `--soft` | `rgb(242, 246, 251)` | `rgb(32, 54, 74)` | 11.46 | 4.5 | PASS | yes |
| Disclosure decorative separator | dark | `--line-soft` | `--paper` | `rgb(44, 65, 85)` | `rgb(16, 26, 37)` | 1.67 | 3.0 | FAIL | no—decorative |
| Disclosure decorative separator | dark | `--line-soft` | `--panel` | `rgb(44, 65, 85)` | `rgb(23, 36, 51)` | 1.49 | 3.0 | FAIL | no—decorative |
| Disclosure decorative separator | dark | `--line-soft` | `--soft` | `rgb(44, 65, 85)` | `rgb(32, 54, 74)` | 1.18 | 3.0 | FAIL | no—decorative |
| Disclosure decorative separator | dark | `--line-soft` | `--soft-strong` | `rgb(44, 65, 85)` | `rgb(48, 74, 101)` | 1.15 | 3.0 | FAIL | no—decorative |
| Disclosure decorative separator | dark | `--line-soft` | `--control-surface` | `rgb(44, 65, 85)` | `rgb(32, 54, 74)` | 1.18 | 3.0 | FAIL | no—decorative |
| Panel decorative border | dark | `--line` | `--paper` | `rgb(64, 87, 108)` | `rgb(16, 26, 37)` | 2.34 | 3.0 | FAIL | no—decorative |
| Panel decorative border | dark | `--line` | `--panel` | `rgb(64, 87, 108)` | `rgb(23, 36, 51)` | 2.09 | 3.0 | FAIL | no—decorative |
| Panel decorative border | dark | `--line` | `--soft` | `rgb(64, 87, 108)` | `rgb(32, 54, 74)` | 1.66 | 3.0 | FAIL | no—decorative |
| Panel decorative border | dark | `--line` | `--soft-strong` | `rgb(64, 87, 108)` | `rgb(48, 74, 101)` | 1.22 | 3.0 | FAIL | no—decorative |
| Panel decorative border | dark | `--line` | `--control-surface` | `rgb(64, 87, 108)` | `rgb(32, 54, 74)` | 1.66 | 3.0 | FAIL | no—decorative |
| Bubble decorative border | dark | `--line` | `--paper` | `rgb(64, 87, 108)` | `rgb(16, 26, 37)` | 2.34 | 3.0 | FAIL | no—decorative |
| Bubble decorative border | dark | `--line` | `--panel` | `rgb(64, 87, 108)` | `rgb(23, 36, 51)` | 2.09 | 3.0 | FAIL | no—decorative |
| Bubble decorative border | dark | `--line` | `--soft` | `rgb(64, 87, 108)` | `rgb(32, 54, 74)` | 1.66 | 3.0 | FAIL | no—decorative |
| Bubble decorative border | dark | `--line` | `--soft-strong` | `rgb(64, 87, 108)` | `rgb(48, 74, 101)` | 1.22 | 3.0 | FAIL | no—decorative |
| Bubble decorative border | dark | `--line` | `--control-surface` | `rgb(64, 87, 108)` | `rgb(32, 54, 74)` | 1.66 | 3.0 | FAIL | no—decorative |
| app.css pre decorative border | dark | `--line` | `--paper` | `rgb(64, 87, 108)` | `rgb(16, 26, 37)` | 2.34 | 3.0 | FAIL | no—decorative |
| app.css pre decorative border | dark | `--line` | `--panel` | `rgb(64, 87, 108)` | `rgb(23, 36, 51)` | 2.09 | 3.0 | FAIL | no—decorative |
| app.css pre decorative border | dark | `--line` | `--soft` | `rgb(64, 87, 108)` | `rgb(32, 54, 74)` | 1.66 | 3.0 | FAIL | no—decorative |
| app.css pre decorative border | dark | `--line` | `--soft-strong` | `rgb(64, 87, 108)` | `rgb(48, 74, 101)` | 1.22 | 3.0 | FAIL | no—decorative |
| app.css pre decorative border | dark | `--line` | `--control-surface` | `rgb(64, 87, 108)` | `rgb(32, 54, 74)` | 1.66 | 3.0 | FAIL | no—decorative |
| app.css strip decorative separator | dark | `--line` | `--paper` | `rgb(64, 87, 108)` | `rgb(16, 26, 37)` | 2.34 | 3.0 | FAIL | no—decorative |
| app.css strip decorative separator | dark | `--line` | `--panel` | `rgb(64, 87, 108)` | `rgb(23, 36, 51)` | 2.09 | 3.0 | FAIL | no—decorative |
| app.css strip decorative separator | dark | `--line` | `--soft` | `rgb(64, 87, 108)` | `rgb(32, 54, 74)` | 1.66 | 3.0 | FAIL | no—decorative |
| app.css strip decorative separator | dark | `--line` | `--soft-strong` | `rgb(64, 87, 108)` | `rgb(48, 74, 101)` | 1.22 | 3.0 | FAIL | no—decorative |
| app.css strip decorative separator | dark | `--line` | `--control-surface` | `rgb(64, 87, 108)` | `rgb(32, 54, 74)` | 1.66 | 3.0 | FAIL | no—decorative |
| app.css table decorative separator | dark | `--line` | `--paper` | `rgb(64, 87, 108)` | `rgb(16, 26, 37)` | 2.34 | 3.0 | FAIL | no—decorative |
| app.css table decorative separator | dark | `--line` | `--panel` | `rgb(64, 87, 108)` | `rgb(23, 36, 51)` | 2.09 | 3.0 | FAIL | no—decorative |
| app.css table decorative separator | dark | `--line` | `--soft` | `rgb(64, 87, 108)` | `rgb(32, 54, 74)` | 1.66 | 3.0 | FAIL | no—decorative |
| app.css table decorative separator | dark | `--line` | `--soft-strong` | `rgb(64, 87, 108)` | `rgb(48, 74, 101)` | 1.22 | 3.0 | FAIL | no—decorative |
| app.css table decorative separator | dark | `--line` | `--control-surface` | `rgb(64, 87, 108)` | `rgb(32, 54, 74)` | 1.66 | 3.0 | FAIL | no—decorative |
