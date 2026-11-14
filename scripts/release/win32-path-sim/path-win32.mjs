// Stands in for `node:path` while win32-path-sim is registered: every function is the Windows one, so
// join("/w", "x") is "\w\x" even on macOS or Linux.
import { win32 } from "node:path";

export const { basename, delimiter, dirname, extname, format, isAbsolute, join, normalize, parse, relative, resolve, sep, toNamespacedPath } = win32;
export const posix = win32.posix;
export { win32 };
export default win32;
