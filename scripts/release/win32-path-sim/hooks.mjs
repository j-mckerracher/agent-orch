// Module resolution hook: install-lib.mjs and its test get the Windows `node:path`; nothing else does, because
// every other module touches the real file system, which a Windows path cannot address on this host.
const PURE_MODULES = /[\\/]install-lib(\.test)?\.mjs$/;

export async function resolve(specifier, context, nextResolve) {
  if ((specifier === "node:path" || specifier === "path") && PURE_MODULES.test(context.parentURL ?? "")) {
    return { url: new URL("./path-win32.mjs", import.meta.url).href, shortCircuit: true };
  }
  return nextResolve(specifier, context);
}
