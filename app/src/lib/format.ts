// Display helpers shared by the views.

import { lang, numLocale, t } from "./i18n.svelte";

const one = (n: number) => n.toLocaleString(numLocale(), { minimumFractionDigits: 1, maximumFractionDigits: 1 });
export const fmtSize = (b: number) =>
  b >= 1 << 30 ? t("format.gb", { n: one(b / (1 << 30)) }) : b >= 1 << 20 ? t("format.mb", { n: one(b / (1 << 20)) }) : t("format.kb", { n: Math.max(1, Math.round(b / 1024)) });

/** 12,345: a whole number in the UI language's style. */
export const fmtNum = (n: number) => n.toLocaleString(numLocale());

const DATES: Record<"short" | "long", Intl.DateTimeFormatOptions> = {
  short: { year: "2-digit", month: "short", day: "numeric" },
  long: { year: "numeric", month: "short", day: "numeric" },
};
/** A unix time as a date ("" for 0, which the backend sends for "unknown"). */
export const fmtDate = (time: number, style: "short" | "long") => (time ? new Date(time * 1000).toLocaleDateString(numLocale(), DATES[style]) : "");

/** A unix time as a date and time. */
export const fmtDateTime = (time: number) => new Date(time * 1000).toLocaleString(numLocale(), { dateStyle: "medium", timeStyle: "short" });

const AGO: [Intl.RelativeTimeFormatUnit, number][] = [["year", 31536000], ["month", 2592000], ["week", 604800], ["day", 86400], ["hour", 3600], ["minute", 60]];
/** "3 days ago": a unix time from now, in the UI language ("" for 0, "unknown"). */
export function fmtAgo(time: number) {
  if (!time) return "";
  const s = time - Date.now() / 1000;
  const [unit, n] = AGO.find(([, n]) => Math.abs(s) >= n) ?? ["minute", 60];
  return new Intl.RelativeTimeFormat(lang(), { numeric: "auto" }).format(Math.round(s / n), unit);
}

/** 1.2K, 3.4M: counts on Workshop cards. */
export const compact = { format: (n: number) => n.toLocaleString(numLocale(), { notation: "compact", maximumFractionDigits: 1 }) };

export const workshopUrl = (id: string) => `https://steamcommunity.com/sharedfiles/filedetails/?id=${id}`;

/** A card's blurb: no BBCode, images or bare links. */
export const plain = (s: string) =>
  s.replace(/\[img\][\s\S]*?\[\/img\]/gi, "").replace(/https?:\/\/\S+/g, "").replace(/\[\/?[a-z0-9*]+(=[^\]]*)?\]/gi, "").replace(/\s+/g, " ").trim();
