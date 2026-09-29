// Runs before first paint (no white flash for dark users). Kept as a separate file, not an inline
// <script>, so the desktop CSP can use script-src 'self' without 'unsafe-inline'.
// Must match src/theme.ts: key "delta-theme" (legacy "openwork-theme" read and migrated once),
// absent/invalid = auto = follow the OS.
try {
  var t = localStorage.getItem("delta-theme");
  if (t !== "light" && t !== "dark") {
    var legacy = localStorage.getItem("openwork-theme");
    if (legacy === "light" || legacy === "dark") {
      localStorage.setItem("delta-theme", legacy);
      localStorage.removeItem("openwork-theme");
      t = legacy;
    }
  }
  var dark = t === "dark" || (t !== "light" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
} catch (e) {}
