# M7-01 wireframe custom-property roles

The wireframe SHA-256 is `24e819fd145304d73582a78bc6a6848641d560ccd2ccb7ddf2cbd68139735cbf`. Run `node docs/evidence/m7-01/extract-tokens.mjs [wireframe-path]` after `npm ci` in `apps/desktop` to enumerate all style and inline declarations, including overridden rules. Values below are the final `:root` and explicit `:root[data-theme="dark"]` declarations; earlier dark media declarations are superseded by the later `:root` rule in the wireframe. The app also supports system dark via its own media rule. Matching hex colors are normalized to six digits.

| Wireframe role | Wireframe light | Wireframe dark | App role | App light | App dark | Verdict |
|---|---|---|---|---|---|---|
| `--amber` | `#765000` | `#ffdb95` | `--amber` | `#765000` | `#ffdb95` | Match |
| `--amber-bg` | `#fff5db` | `#4a3a20` | `--amber-bg` | `#fff5db` | `#4a3a20` | Match |
| `--blue` | `#0057a8` | `#a0ceff` | `--blue` | `#0057a8` | `#a0ceff` | Match |
| `--blue-bg` | `#edf5fd` | `#223f5b` | `--blue-bg` | `#edf5fd` | `#223f5b` | Match |
| `--green` | `#0057a8` | `#a0ceff` | `--success` | `#0057a8` | `#a0ceff` | Renamed: positive state uses success to preserve separate semantic green. |
| `--green-bg` | `#edf5fd` | `#223f5b` | `--success-bg` | `#edf5fd` | `#223f5b` | Renamed: positive state uses success to preserve separate semantic green. |
| `--ink` | `#111` | `#f2f6fb` | `--ink` | `#111111` | `#f2f6fb` | Match |
| `--level` | inline 27–71% | same | `--level` | runtime percentage | runtime percentage | Match in purpose: per-meter geometry, set by `primitives.tsx`, not a theme colour. |
| `--line` | `#c8d5e4` | `#40576c` | `--line` | `#c8d5e4` | `#40576c` | Match |
| `--mono` | `"SFMono-Regular",Consolas,monospace` | `"SFMono-Regular",Consolas,monospace` | `--font-mono` | `"SFMono-Regular", ui-monospace, Consolas, monospace` | `"SFMono-Regular", ui-monospace, Consolas, monospace` | Intentional deviation: renamed; app adds platform monospace fallbacks (`ui-monospace`). |
| `--muted` | `#111` | `#c2d2e3` | `--muted` | `#111111` | `#c2d2e3` | Match |
| `--panel` | `#fff` | `#172433` | `--panel` | `#ffffff` | `#172433` | Match |
| `--paper` | `#fff` | `#101a25` | `--paper` | `#ffffff` | `#101a25` | Match |
| `--red` | `#a32d34` | `#ffbdc2` | `--red` | `#a32d34` | `#ffbdc2` | Match |
| `--red-bg` | `#fff0f0` | `#503137` | `--red-bg` | `#fff0f0` | `#503137` | Match |
| `--soft` | `#edf4fb` | `#20364a` | `--soft` | `#edf4fb` | `#20364a` | Match |
| `--usage-footer-height` | `0px` | `0px` | — | — | — | Intentional deviation: wireframe-only sticky-footer layout variable; not a colour or shared theme token. |

The extracted earlier root media palette and inline `--level` occurrences are retained in the JSON output, rather than silently treated as the final explicit-theme palette. `--usage-footer-height` is a layout implementation detail; `--level` is represented in `src/ui/theme/primitives.tsx:190`.
