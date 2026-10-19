// The UI in the player's language: t("area.key", { name }) reads locales/*.json, English for anything missing.
// Plurals are { "one": …, "other": … } objects (any Intl.PluralRules category), picked by the `count` var.
// Unknown keys come back as they are, so text from the backend can go through the same calls.
import { prefs } from "./prefs.svelte";

type Dict = { [k: string]: string | Dict };
export type Vars = Record<string, string | number>;

const dicts = import.meta.glob<Dict>("./locales/*.json", { eager: true, import: "default" });

/** Each language by its own name, as the Settings menu lists them. */
export const LANGS: Record<string, string> = {
  en: "English",
  de: "Deutsch",
  es: "Español",
  fr: "Français",
  it: "Italiano",
  pl: "Polski",
  "pt-BR": "Português (Brasil)",
  ru: "Русский",
  ko: "한국어",
  "zh-CN": "简体中文",
};

/** The system's language if we have it (exact, then by language: pt-PT gets pt-BR), else English. */
export function systemLang() {
  const sys = navigator.language;
  return LANGS[sys] ? sys : (Object.keys(LANGS).find((l) => l.split("-")[0] === sys.split("-")[0]) ?? "en");
}

/** The UI language: the one picked in Settings, or the system's. */
export const lang = () => (LANGS[prefs.lang] ? prefs.lang : systemLang());

/** The UI language for Intl (lists, relative times): our English is British ("A, B and C", not "A, B, and C"). */
export const intlLang = () => (lang() === "en" ? "en-GB" : lang());

/** Locale for numbers and dates: the system's own (its region) when it speaks the UI language. */
export const numLocale = () => (navigator.language.split("-")[0] === lang().split("-")[0] ? navigator.language : intlLang());

const lookup = (d: Dict | undefined, key: string) => key.split(".").reduce<string | Dict | undefined>((o, k) => (typeof o === "object" ? o[k] : undefined), d);

export function t(key: string, vars: Vars = {}, to = lang()): string {
  let s = lookup(dicts[`./locales/${to}.json`], key) ?? lookup(dicts["./locales/en.json"], key) ?? key;
  if (typeof s === "object") s = (s[new Intl.PluralRules(to).select(Number(vars.count))] ?? s.other) as string;
  // a count reads in the UI language's style (12,345), the backend's (a string of digits) too
  const show = (k: string) => (k === "count" && /^\d+$/.test(String(vars[k])) ? Number(vars[k]).toLocaleString(numLocale()) : String(vars[k]));
  return s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? show(k) : m));
}

const esc = (v: string | number) => String(v).replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
/** `t` for text with markup (<strong>, <kbd>…) to render with {@html}: the vars are escaped, the locale files are ours. */
export const th = (key: string, vars: Vars = {}) => t(key, Object.fromEntries(Object.entries(vars).map(([k, v]) => [k, esc(v)])));

/**
 * A backend error as `t`'s arguments: a keyed one (`errors.<key>{values}: English`, src-tauri's `keyed!`) gives its key
 * and values, so `t(...keyedError(e))` shows it in the player's language; anything else (or a key we lack) its own text.
 */
export function keyedError(e: unknown): [string, Vars] {
  const s = String(e).replace(/^Error:\s*/, "");
  // a certificate PZMM can't trust, from any web call (sources::CERT_INTERCEPTED in the core)
  if (s.includes("is intercepting secure connections")) return ["errors.certIntercepted", {}];
  const m = s.match(/^(errors\.\w+)(\{[\s\S]*?\})?: /);
  if (!m) return [s, {}];
  try {
    if (lookup(dicts["./locales/en.json"], m[1]) !== undefined) return [m[1], m[2] ? JSON.parse(m[2]) : {}];
  } catch {}
  return [s.slice(m[0].length), {}];
}
