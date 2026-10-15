// Self-check for the locale files and hard-coded UI text. Run: node scripts/i18n-check.ts
// Every locale has exactly en.json's keys with the same {placeholders}; every t("key") exists; no .svelte
// markup text, title/aria-label/placeholder/alt, or capitalised script phrase skips t(). Mark a deliberate
// exception with "i18n-ignore" on its line or the line above.
import { readFileSync, readdirSync } from "node:fs";

const dir = new URL("../src/lib/locales/", import.meta.url);
type Dict = { [k: string]: string | Dict };
const PLURAL = new Set(["zero", "one", "two", "few", "many", "other"]);
const flat = (d: Dict, pre = "", out: Record<string, string | Dict> = {}) => {
  for (const [k, v] of Object.entries(d)) {
    // a plural leaf has only plural categories (a group that merely has an "other" key isn't one)
    const isPlural = typeof v === "object" && "other" in v && Object.entries(v).every(([c, s]) => PLURAL.has(c) && typeof s === "string");
    if (typeof v === "string" || isPlural) out[pre + k] = v;
    else flat(v, `${pre}${k}.`, out);
  }
  return out;
};
const load = (f: string) => flat(JSON.parse(readFileSync(new URL(f, dir), "utf8")));
const holes = (s: string) => new Set(s.match(/\{\w+\}/g) ?? []);
const forms = (v: string | Dict) => (typeof v === "string" ? [v] : Object.values(v as Record<string, string>));
const union = (v: string | Dict) => [...new Set(forms(v).flatMap((s) => [...holes(s)]))].sort().join(" ");
const tags = (v: string | Dict) => (forms(v)[0].match(/<[^>]+>/g) ?? []).sort().join("");

const errors: string[] = [];
const en = load("en.json");
const files = readdirSync(dir).filter((f) => f.endsWith(".json"));
for (const f of files.filter((f) => f !== "en.json")) {
  const loc = load(f);
  for (const k of Object.keys(en)) {
    const v = loc[k];
    if (v === undefined) errors.push(`${f}: missing ${k}`);
    else if (typeof v !== typeof en[k] || (typeof v === "object" && typeof v.other !== "string")) errors.push(`${f}: ${k} must be ${typeof en[k] === "string" ? "a string" : "a plural object with \"other\""}`);
    else if (union(v) !== union(en[k]) || forms(v).some((s) => [...holes(s)].some((h) => !union(en[k]).includes(h))))
      errors.push(`${f}: ${k} placeholders ${union(v) || "(none)"} differ from English ${union(en[k]) || "(none)"}`);
    else if (tags(v) !== tags(en[k])) errors.push(`${f}: ${k} markup ${tags(v) || "(none)"} differs from English ${tags(en[k]) || "(none)"}`);
  }
  for (const k of Object.keys(loc)) if (!(k in en)) errors.push(`${f}: extra key ${k}`);
}

// sources: t("key") keys exist, and nothing user-facing is hard-coded
const src = new URL("../src/", import.meta.url);
const sources = readdirSync(src, { recursive: true, encoding: "utf8" })
  .filter((f) => /\.(svelte|ts)$/.test(f) && !f.includes("i18n.svelte.ts"))
  .map((f) => f.replaceAll("\\", "/"));
/** Same length, same line breaks: blank out a match so line numbers stay put. */
const blank = (m: string) => m.replace(/[^\n]/g, " ");
const IGNORE = "i18n-ignore";
const KEYS = /^(Escape|Enter|Delete|Backspace|Tab|Home|End|Arrow[A-Z]\w+|Page[A-Z]\w+)$/; // e.key names
let code = ""; // every source, for the unused-key check below

for (const f of sources) {
  const text = readFileSync(new URL(f, src), "utf8");
  code += text;
  const lines = text.split("\n");
  const flag = (i: number, what: string) => {
    if (!lines[i]?.includes(IGNORE) && !lines[i - 1]?.includes(IGNORE)) errors.push(`${f}:${i + 1}: ${what}`);
  };
  const lineOf = (at: number) => text.slice(0, at).split("\n").length - 1;

  for (const m of text.matchAll(/\bth?\(\s*"([\w.]+)"/g)) if (!(m[1] in en)) flag(lineOf(m.index), `unknown key ${m[1]}`);

  const isSvelte = f.endsWith(".svelte");
  const scripts = isSvelte ? [...text.matchAll(/<script[^>]*>([\s\S]*?)<\/script>/g)].map((m) => ({ code: m[1], at: m.index + m[0].indexOf(m[1]), markup: false })) : [{ code: text, at: 0, markup: false }];
  // markup: what's left once scripts, styles, comments and {expressions} are gone
  if (isSvelte) {
    let mk = text.replace(/<(script|style)[^>]*>[\s\S]*?<\/\1>/g, blank).replace(/<!--[\s\S]*?-->/g, blank);
    // {expressions}: script code in the markup too (handlers, ternaries) gets the script check below
    for (let prev = ""; prev !== mk; prev = mk, mk = mk.replace(/\{[^{}]*\}/g, blank))
      for (const m of mk.matchAll(/\{[^{}]*\}/g)) scripts.push({ code: m[0], at: m.index, markup: true });
    for (const m of mk.matchAll(/\s(title|aria-label|placeholder|alt|label)="([^"]*[A-Za-z]{2}[^"]*)"/g)) flag(lineOf(m.index), `${m[1]}="${m[2]}"`);
    mk = mk.replace(/<[^>]*>/g, blank).replace(/&\w+;/g, blank);
    mk.split("\n").forEach((l, i) => /[A-Za-z]{2}/.test(l) && flag(i, `text "${l.trim()}"`));
  }
  // script: capitalised phrases in string literals ("Couldn't save", `Installing ${n} mods`)
  // and in markup {expressions}, single capitalised words too ({on ? "Disable" : "Enable"})
  for (const { code, at, markup } of scripts) {
    const clean = code.replace(/\/\*[\s\S]*?\*\//g, blank).replace(/(^|[^:"'`])\/\/[^\n]*/g, (m, p) => p + blank(m.slice(p.length)));
    for (const m of clean.matchAll(/(["'`])((?:\\.|(?!\1)[^\\\n])*)\1/g)) {
      const s = m[2].replace(/\$\{[^}]*\}/g, " ");
      if (/^\s*[A-Z][a-z’']+[,:]?\s+[a-z]/.test(s) || /^\s*[a-z]+ [a-z]+ [a-z]+/.test(s) || (markup && /^[A-Z][a-z]{2,}…?$/.test(s) && !KEYS.test(s))) flag(lineOf(at + m.index), `string ${m[0].slice(0, 60)}`);
    }
  }
}

// the backend's own wording: say!("key", "English") lines are backend.<key>, keyed!("key", "English") errors errors.<key>, Check setup findings doctor.<key>, mod checks authoring.check.<key>;
// en.json has the same English (an "error: " prefix only marks an error line), so the CLI and the app agree
const rustDirs = ["../../crates/core/src/", "../src-tauri/src/"].map((d) => new URL(d, import.meta.url));
const lit = String.raw`"((?:\\.|[^"\\])*)"`;
const used = new Set<string>(); // keys the backend sends
// the in-game manager's strings: scripts/ingame-i18n.ts takes them to the game
const prefixes: string[] = ["ingame."];
const english = (k: string, text: string, where: string) => {
  used.add(k);
  if (!(k in en)) errors.push(`${where}: en.json has no ${k} ("${text}")`);
  else {
    // a plural object (by {count}) has the backend's English as its "other", with "item(s)" as "items"
    const [ours, theirs] = typeof en[k] === "string" ? [en[k], text] : [(en[k] as Dict).other, text.replaceAll("(s)", "s")];
    if (ours !== theirs) errors.push(`${where}: en.json's ${k} "${ours}" isn't the backend's "${theirs}"`);
  }
};
for (const dir of rustDirs)
  for (const f of readdirSync(dir, { recursive: true, encoding: "utf8" }).filter((f) => f.endsWith(".rs"))) {
    const text = readFileSync(new URL(f.replaceAll("\\", "/"), dir), "utf8");
    for (const m of text.matchAll(/format!\("([\w.]+\.)\{/g)) prefixes.push(m[1]); // format!("maps.err.{}", ..)
    for (const m of text.matchAll(new RegExp(String.raw`say!\(\s*"(\w+)",\s*` + lit, "g"))) english(`backend.${m[1]}`, JSON.parse(`"${m[2]}"`).replace(/^error: /, ""), f);
    for (const m of text.matchAll(new RegExp(String.raw`keyed!\(\s*"(\w+)",\s*` + lit, "g"))) english(`errors.${m[1]}`, JSON.parse(`"${m[2]}"`), f);
    for (const m of text.matchAll(new RegExp(String.raw`\b(?:err|warn|note)\([^,()]+,\s*"(\w+)",\s*` + lit, "g"))) english(`doctor.${m[1]}`, JSON.parse(`"${m[2]}"`), f);
    for (const m of text.matchAll(new RegExp(String.raw`\b(?:bad|iffy)\(.*?,\s*"(\w+)",\s*` + lit, "g"))) english(`authoring.check.${m[1]}`, JSON.parse(`"${m[2]}"`), f);
  }

// every en.json key is used: quoted whole somewhere, under a dynamic prefix (`kinds.${k}`, format!("maps.err.{}")),
// or a quoted key with a dynamic suffix (`${key}For`)
prefixes.push(...[...code.matchAll(/`([\w.]+)\$\{/g)].map((m) => m[1]));
const suffixes = [...new Set([...code.matchAll(/\}(\w+)`/g)].map((m) => m[1]))];
const quoted = (k: string) => ['"', "'", "`"].some((q) => code.includes(q + k + q));
for (const k of Object.keys(en))
  if (!used.has(k) && !quoted(k) && !prefixes.some((p) => k.startsWith(p)) && !suffixes.some((s) => k.endsWith(s) && quoted(k.slice(0, -s.length)))) errors.push(`en.json: ${k} is never used`);

// What's new pops up only when its notes were written for the version being built: say so when they weren't
const version = readFileSync(new URL("../../Cargo.toml", import.meta.url), "utf8").match(/^version = "(.+)"/m)?.[1];
const notesFor = readFileSync(new URL("../src/lib/WhatsNew.svelte", import.meta.url), "utf8").match(/NOTES_FOR = "(.+)"/)?.[1];
if (version !== notesFor) console.warn(`i18n-check: warning: whatsNew.notes are for ${notesFor}, the app is ${version}: no What's new shows after this update (update the notes in every locale and NOTES_FOR)`);

if (errors.length) {
  console.error(errors.join("\n"));
  console.error(`i18n-check: ${errors.length} problem(s)`);
  process.exit(1);
}
console.log(`i18n-check: ok (${Object.keys(en).length} keys, ${files.length} languages)`);
