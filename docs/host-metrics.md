# Host metrics (`host.metrics`)

Read-only daemon method that reports host CPU, memory, disk and GPU meters for
the operator console. Board item M7-04; plan:
`/Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md`.

Code: crate `crates/orchestrator-host-metrics` (sampling, cache, wire types),
`crates/orchestrator-daemon/src/api_host.rs` (method), and the `host`
capability in `crates/orchestrator-ipc`.

## Contract

| | |
|---|---|
| Method | `host.metrics` |
| Capability | `host` (negotiated at handshake; without it the call fails with the typed unsupported-capability error, code -32004) |
| Auth | Same token handshake as every method |
| Params | none: `null` or `{}`. Anything else is `InvalidParams` (-32602) |
| Client | `Client::host_metrics()` |

Result (`schema_version` 1):

```json
{
  "schema_version": 1,
  "sample_interval_ms": 2000,
  "meters": [
    {
      "meter": "cpu | memory | disk | gpu",
      "state": "ok | unavailable | unsupported",
      "kind": "utilization | capacity",
      "unit": "percent | bytes",
      "scope": "label of the device/volume/mount, or null",
      "value": 12.5,
      "total": null,
      "observed_at": "2026-11-10T18:38:53.359Z",
      "sample_interval_ms": 2000,
      "reason": null
    }
  ]
}
```

- Every field is always present; absent data is `null`.
- `cpu`: `utilization`, `percent` (0-100, all logical CPUs). The first sample
  after start has no baseline and is `unavailable`.
- `memory`: `capacity`, `bytes`; `value` is used, `total` is installed
  physical memory.
- `disk`: `capacity`, `bytes`; the volume that holds the daemon data
  directory. `value` = total - available (as `df` reports it); `total` is the
  volume size; `scope` is the mount point.
- `gpu`: `utilization`, `percent`, one meter per device, `scope` = device name.
- `state != ok` means `value`, `total` and `observed_at` are `null` and `reason`
  says why. A meter is never `0` because data is missing. `unsupported` means
  the platform has no vetted source; `unavailable` means a source exists but
  gave no usable reading (no sample yet, read failed, inconsistent numbers, or
  the sample is stale).
- All four resources are always listed, in the order cpu, memory, disk, gpu
  (gpu can have several entries).

## Sampling

A background thread (`host-metrics`) samples every 2 s (floor 500 ms) outside
any lock and stores the result. `host.metrics` copies that cache, so it never
runs a probe, never waits on one and cannot block another RPC. Before the first
sample every meter is `unavailable` ("no sample has completed yet"). A sample
older than 5 intervals is reported `unavailable` ("stale") so a wedged probe is
visible rather than silently serving old numbers. Dropping the service stops
the thread without joining it.

## Library choice: sysinfo 0.40.1

- Licence MIT. MSRV 1.95 (workspace toolchain 1.95+).
- `default-features = false`, features `system` and `disk` only (no
  network/component/user crates). On macOS also `gpu`.
- Platform sources are sysinfo's own native APIs (no shelling out). On macOS
  the GPU meter comes from IOKit "Device Utilization %"; sysinfo's Windows GPU
  path (PDH "GPU Engine" counters) exists but is not used.
- GPU policy: the macOS source was compared with `ioreg` (below) and is
  vetted. sysinfo's Windows GPU path (PDH "GPU Engine" counters) has not been
  compared with Task Manager, so Windows and every other target report GPU as
  `unsupported` rather than guessing.

## Manual comparison

Run `cargo run -p orchestrator-host-metrics --example dump -- <data-dir>` and
compare with the OS tool at the same moment.

Tolerance: CPU within 15 percentage points under fluctuating load (the OS tools
average different windows), within 5 on a steady load; memory within 5% of
installed memory; disk within 1% of volume size; GPU within 10 percentage
points.

macOS (M3 Pro, 36 GiB), 2026-11-10:

| Meter | host.metrics | Reference |
|---|---|---|
| CPU | 98.3 / 50.4 / 53.3 % (three runs) | `top`: ~94 / ~55 / ~55 % over the same seconds |
| Memory | 28.24 GB used of 38.65 GB | `vm_stat`: anonymous 13.45 + wired 3.31 + compressor 11.90 = 28.66 GB (Activity Monitor "Memory Used" formula), 1.1% of installed apart |
| Disk | 450.7 GB used of 494.4 GB on `/` | `df -k`: 494.4 GB total, 42.8 GB available (same container; `df` "used" excludes other APFS volumes) |
| GPU | 47-54 % ("Apple M3 Pro") | `ioreg -r -d 1 -c IOAccelerator`: "Device Utilization %" 47-48 |

On APFS the data directory resolves to the `/` mount because the firmlinked
Data volume is not a path prefix; both share one container, so total and
available match `df` for the Data volume.

Windows Task Manager comparison: not yet done (see the implementation report).
