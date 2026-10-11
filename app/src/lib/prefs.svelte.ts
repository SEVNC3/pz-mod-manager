// Look and feel for whoever uses this computer, kept in localStorage (the backend never needs them).
import { lang } from "./i18n.svelte";

const read = (k: string, d: string) => {
  try {
    return localStorage.getItem(k) ?? d;
  } catch {
    return d;
  }
};
const DEFAULT_ACCENT = "#6e080c"; // the blood on the separator tape (static/blood.svg)

export const prefs = $state({
  theme: read("pzmm.theme", "system"),
  /** UI language ("" follows the system). */
  lang: read("pzmm.lang", ""),
  // people still on an earlier default (amber, then pink) move to the current one
  accent: [read("pzmm.accent", DEFAULT_ACCENT)].map((a) => (["#d39b3a", "#fc2684"].includes(a) ? DEFAULT_ACCENT : a))[0],
  compact: read("pzmm.compact", "0") === "1",
  showIds: read("pzmm.showIds", "1") === "1",
  workshopView: (read("pzmm.workshopView", "browse") === "home" ? "home" : "browse") as "home" | "browse",
  perPage: (read("pzmm.perPage", "30") === "50" ? 50 : 30) as 30 | 50,
  /** The Maps tab lists every Map-tagged Workshop item (the built-in index, then Steam), not only installed maps. */
  mapsWorkshop: read("pzmm.mapsWorkshop", "1") === "1",
  /** The Maps tab's tool panel (floating over the map): its open tab, its width in pixels, and whether it's minimised. */
  mapsTab: read("pzmm.mapsTab", "mods"),
  mapsPanel: Number(read("pzmm.mapsPanel", "520")) || 520,
  mapsMin: read("pzmm.mapsMin", "0") === "1",
  /** The Workshop tab leaves out what's already installed or subscribed. */
  hideInstalled: read("pzmm.hideInstalled", "0") === "1",
});

/** Text colour on an accent fill: dark on light accents, white on dark ones. */
const onAccent = (hex: string) => {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.45 ? "#1b1a17" : "#ffffff";
};

/** Apply and remember the preferences (run it in an effect: it reads every one). */
export function applyPrefs() {
  const root = document.documentElement;
  if (prefs.theme === "system") delete root.dataset.theme;
  else root.dataset.theme = prefs.theme;
  root.style.setProperty("--accent", prefs.accent);
  root.style.setProperty("--on-accent", onAccent(prefs.accent));
  root.classList.toggle("compact", prefs.compact);
  root.lang = lang();
  try {
    localStorage.setItem("pzmm.theme", prefs.theme);
    localStorage.setItem("pzmm.lang", prefs.lang);
    localStorage.setItem("pzmm.accent", prefs.accent);
    localStorage.setItem("pzmm.compact", prefs.compact ? "1" : "0");
    localStorage.setItem("pzmm.showIds", prefs.showIds ? "1" : "0");
    localStorage.setItem("pzmm.workshopView", prefs.workshopView);
    localStorage.setItem("pzmm.mapsWorkshop", prefs.mapsWorkshop ? "1" : "0");
    localStorage.setItem("pzmm.mapsTab", prefs.mapsTab);
    localStorage.setItem("pzmm.mapsPanel", String(prefs.mapsPanel));
    localStorage.setItem("pzmm.mapsMin", prefs.mapsMin ? "1" : "0");
    localStorage.setItem("pzmm.perPage", String(prefs.perPage));
    localStorage.setItem("pzmm.hideInstalled", prefs.hideInstalled ? "1" : "0");
  } catch {}
}
