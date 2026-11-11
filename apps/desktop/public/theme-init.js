/*
 * Applies the stored theme before first paint. Classic (blocking) script so an explicit
 * choice never flashes the other theme; it mirrors theme.ts initTheme and must keep the same
 * storage key. Storage may throw (blocked profile), in which case the system scheme applies.
 */
(function () {
  try {
    var value = window.localStorage.getItem("agent-orch.theme");
    if (value === "light" || value === "dark") document.documentElement.setAttribute("data-theme", value);
  } catch (error) {
    /* no stored preference */
  }
})();
