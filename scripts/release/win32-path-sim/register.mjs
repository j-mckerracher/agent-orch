// Simulates Windows path separators for the pure install-smoke code on any host:
//
//   node --import ./scripts/release/win32-path-sim/register.mjs --test scripts/release/install-lib.test.mjs
//
// It proves only that the tests and install-lib.mjs agree on how paths are built (path.join, path.resolve) when
// the separator is a backslash. It does not prove anything about a real Windows machine: drive letters, the
// real file system, CRLF files and PowerShell stay UNPROVEN until the Windows gates run.
import { register } from "node:module";

register("./hooks.mjs", import.meta.url);
