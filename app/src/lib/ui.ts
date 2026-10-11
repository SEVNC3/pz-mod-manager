// Small DOM helpers shared by the views.
import { tick } from "svelte";

/** The key went to a text field or a select: global shortcuts leave it alone. */
export const isTyping = (e: KeyboardEvent) =>
  e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement;

const FOCUSABLE = "button:not(:disabled), a[href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])";

/**
 * `use:trap` on a dialog or a card that asks something: focus moves in (to the node itself with
 * `self`, else its first control), Tab and Shift+Tab stay inside, and when it goes focus returns
 * to what had it (or to `fallback`, a selector, when that's gone), unless focus has moved on.
 */
export function trap(node: HTMLElement, { self = false, fallback = "" } = {}) {
  const back = document.activeElement === document.body ? null : (document.activeElement as HTMLElement | null);
  const all = () => [...node.querySelectorAll<HTMLElement>(FOCUSABLE)];
  (self ? node : all()[0])?.focus();
  function key(e: KeyboardEvent) {
    if (e.key !== "Tab") return;
    const f = all();
    if (!f.length) return;
    const [first, last] = [f[0], f[f.length - 1]];
    if (e.shiftKey && (document.activeElement === first || document.activeElement === node)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }
  node.addEventListener("keydown", key);
  return {
    destroy() {
      node.removeEventListener("keydown", key);
      // after the DOM settles (what replaces the card is drawn); only if focus was dropped with it
      setTimeout(() => {
        if (document.activeElement && document.activeElement !== document.body) return;
        (back?.isConnected ? back : fallback ? document.querySelector<HTMLElement>(fallback) : null)?.focus();
      });
    },
  };
}

/**
 * Arrow keys for a popup menu: Up/Down move between its enabled items (wrapping), Esc and Tab
 * close it (Esc puts focus back on the opener; Tab lets it move on).
 */
export function menuKeys(e: KeyboardEvent, menu: HTMLElement | undefined, close: (refocus: boolean) => void) {
  const items = [...(menu?.querySelectorAll<HTMLElement>("[role^=menuitem]:not(:disabled)") ?? [])];
  const i = items.indexOf(document.activeElement as HTMLElement);
  if (e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    close(true);
  } else if (e.key === "Tab") {
    close(false);
  } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    items[(i + (e.key === "ArrowDown" ? 1 : items.length - 1)) % items.length]?.focus();
  }
}

/** Left/Right arrows step through a tab list or segmented control and move focus along. */
export function arrowPick(e: KeyboardEvent, ids: string[], current: string, set: (v: string) => void) {
  if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
  e.preventDefault();
  const i = (ids.indexOf(current) + (e.key === "ArrowRight" ? 1 : ids.length - 1)) % ids.length;
  set(ids[i]);
  const buttons = (e.currentTarget as HTMLElement).querySelectorAll("button");
  tick().then(() => (buttons[i] as HTMLElement | undefined)?.focus());
}
