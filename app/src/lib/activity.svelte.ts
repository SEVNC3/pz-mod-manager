import { untrack } from "svelte";
import type { Vars } from "./i18n.svelte";
import type { IconName } from "./Icon.svelte";

// Jobs a tab runs in its own view (server tools, saves, authoring, the feed): the activity bar shows them too.
export const background = $state({ jobs: 0 });

/** In a component's `$effect`: counts as running until the effect re-runs or the component goes.
 * Untracked: reading the count there would make the effect re-run on every change it makes. */
export function track() {
  untrack(() => background.jobs++);
  return () => untrack(() => void background.jobs--);
}

/** What an activity line is about: its icon, the expanded view's filters and what it offers. */
export type LogKind = "install" | "update" | "remove" | "error" | "info";

/** An activity line. `key` (a locale key, or null for text as is) and `vars` say what it was, so
 * icons and filters never guess from the words; `items` are the Workshop items it's about. */
export type LogEntry = { text: string; key: string | null; vars: Vars; kind: LogKind; items: string[]; time: number; url?: string };

export const KIND_ICONS: Record<LogKind, IconName> = { install: "download", update: "refresh", remove: "trash", error: "error", info: "info" };

const KINDS: Record<string, LogKind> = {
  "backend.installed": "install",
  "backend.subscribed": "install",
  "backend.updated": "update",
  "backend.removed": "remove",
  "backend.unsubscribed": "remove",
};

/** A line's kind and Workshop items, from its key and vars. */
export function classify(key: string | null, vars: Vars, error: boolean): Pick<LogEntry, "kind" | "items"> {
  const kind = error ? "error" : (key && KINDS[key]) || "info";
  const item = String(vars.source ?? "").match(/^(?:workshop|steam):(\d+)$/)?.[1] ?? (key?.endsWith("subscribed") ? String(vars.id ?? "") : "");
  return { kind, items: item ? [item] : [] };
}
