// The 14 NoteCraft themes (light/dark x 7 accents) — accent hex values are a direct
// port of crates/ui-egui/src/theme.rs's color math, matching the app exactly. See
// AGENTS.md for how to regenerate this if theme.rs changes.
const ACCENT_ORDER = ["default", "red", "green", "yellow", "blue", "gray", "purple"];
const ACCENT_LABEL = { default: "Default", red: "Red", green: "Green", yellow: "Yellow", blue: "Blue", gray: "Gray", purple: "Purple" };
const THEME_ACCENTS = {
  "light-default": "#7a45b8", "light-red": "#c0392b", "light-green": "#1e8a44",
  "light-yellow": "#a87400", "light-blue": "#1a5fd0", "light-gray": "#5b6472", "light-purple": "#7a45b8",
  "dark-default": "#b085e8", "dark-red": "#ff8a80", "dark-green": "#6cd08a",
  "dark-yellow": "#f2c94c", "dark-blue": "#8fb8ff", "dark-gray": "#b4bcc8", "dark-purple": "#b085e8",
};
const STORAGE_KEY = "notecraft-theme";
const DEFAULT_THEME = "dark-default";

function currentTheme() {
  return document.documentElement.getAttribute("data-theme") || DEFAULT_THEME;
}

function setTheme(id) {
  document.documentElement.setAttribute("data-theme", id);
  try { localStorage.setItem(STORAGE_KEY, id); } catch (e) { /* private browsing, etc. */ }
  document.querySelectorAll(".theme-swatch").forEach((btn) => {
    btn.setAttribute("aria-pressed", String(btn.dataset.theme === id));
  });
  const dot = document.querySelector(".theme-toggle .swatch");
  if (dot) dot.style.background = THEME_ACCENTS[id];
}

function buildPicker(popover) {
  for (const mode of ["dark", "light"]) {
    const label = document.createElement("div");
    label.className = "row-label";
    label.textContent = mode === "dark" ? "Dark" : "Light";
    popover.appendChild(label);

    const row = document.createElement("div");
    row.className = "theme-row";
    for (const accent of ACCENT_ORDER) {
      const id = `${mode}-${accent}`;
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "theme-swatch";
      btn.dataset.theme = id;
      btn.style.background = THEME_ACCENTS[id];
      btn.title = `${mode === "dark" ? "Dark" : "Light"} ${ACCENT_LABEL[accent]}`;
      btn.setAttribute("aria-label", btn.title);
      btn.addEventListener("click", () => setTheme(id));
      row.appendChild(btn);
    }
    popover.appendChild(row);
  }
}

document.addEventListener("DOMContentLoaded", () => {
  const toggle = document.querySelector(".theme-toggle");
  const popover = document.querySelector(".theme-popover");
  if (!toggle || !popover) return;

  buildPicker(popover);
  setTheme(currentTheme());

  toggle.addEventListener("click", (e) => {
    e.stopPropagation();
    popover.classList.toggle("open");
  });
  document.addEventListener("click", (e) => {
    if (!popover.contains(e.target) && e.target !== toggle) popover.classList.remove("open");
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") popover.classList.remove("open");
  });
});
