# Codex app-server fixtures (DPES-1183)

Frames for `tests/codex_app_server_adapter.rs`. Provenance per fixture, from the DPES-1183 spec:

| File | Provenance |
|------|------------|
| `initialize_result.json` | Field names from spec 5.2 (observed in spike). Values are illustrative. |
| `thread_start_result.json` | Fields from spec 5.4. Thread id observed (`01a1ffc6-...`). Other values illustrative. |
| `approval_command_request.json` | Schema-derived from Codex CLI 0.171.0 (`ServerRequest`). Not observed live. |
| `approval_filechange_request.json` | Schema-derived from Codex CLI 0.171.0 (`ServerRequest`). Not observed live. |
| `turn_completed_ok.json` | Schema-derived. Not observed live. |
| `turn_completed_failed.json` | Shape observed in spike (spec 5.10). |
| `error_usage_limit.json` | Shape observed in spike (spec 5.10). The message text is illustrative; the spec does not quote it. |
| `item_completed_plan.json` | **SYNTHETIC.** Built from the schema (spec 5.9), never recorded. The live plan turn never ran. Replace it with a recorded frame once the live run (DPES-1183 Q17) succeeds. |

Each fixture is pretty-printed JSON. The fake child strips newlines before it emits a frame.
