// Display helpers shared by the views.

export const fmtSize = (b: number) =>
  b >= 1 << 30 ? `${(b / (1 << 30)).toFixed(1)} GB` : b >= 1 << 20 ? `${(b / (1 << 20)).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1024))} KB`;

const DATES: Record<"short" | "long", Intl.DateTimeFormatOptions> = {
  short: { year: "2-digit", month: "short", day: "numeric" },
  long: { year: "numeric", month: "short", day: "numeric" },
};
/** A unix time as a date ("" for 0, which the backend sends for "unknown"). */
export const fmtDate = (t: number, style: "short" | "long") => (t ? new Date(t * 1000).toLocaleDateString(undefined, DATES[style]) : "");

/** 1.2K, 3.4M: counts on Workshop cards. */
export const compact = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 });

export const workshopUrl = (id: string) => `https://steamcommunity.com/sharedfiles/filedetails/?id=${id}`;

/** A card's blurb: no BBCode, images or bare links. */
export const plain = (s: string) =>
  s.replace(/\[img\][\s\S]*?\[\/img\]/gi, "").replace(/https?:\/\/\S+/g, "").replace(/\[\/?[a-z0-9*]+(=[^\]]*)?\]/gi, "").replace(/\s+/g, " ").trim();
