// Small DOM helpers shared by the views.
import { tick } from "svelte";
import { api } from "./api";

/** The key went to a text field, or to a select (anything but Esc, which a closed select doesn't use):
 * global shortcuts leave it alone. Checkboxes and radios aren't typed in. */
export const isTyping = (e: KeyboardEvent) =>
  e.target instanceof HTMLSelectElement ? e.key !== "Escape"
    : e.target instanceof HTMLTextAreaElement || (e.target instanceof HTMLInputElement && !["checkbox", "radio", "range", "color", "button", "submit"].includes(e.target.type));

const FOCUSABLE = "button:not(:disabled), a[href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])";

/**
 * `use:trap` on a dialog or a card that asks something: focus moves in (to the node itself with
 * `self`, else its first control; not with `focus` false, for a question nobody just asked for),
 * Tab and Shift+Tab stay inside (not with `cycle` false, for a strip that isn't modal), and when it
 * goes focus returns to what had it (or to `fallback`, a selector, when that's gone), unless focus has moved on.
 */
export function trap(node: HTMLElement, { self = false, fallback = "", focus = true, cycle = true } = {}) {
  const back = document.activeElement === document.body ? null : (document.activeElement as HTMLElement | null);
  const all = () => [...node.querySelectorAll<HTMLElement>(FOCUSABLE)];
  if (focus) (self ? node : all()[0])?.focus();
  function key(e: KeyboardEvent) {
    if (e.key !== "Tab" || !cycle) return;
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

/** Arrow keys (Left/Right, or Up/Down for a vertical list) step through a tab list or segmented control and move focus along. */
export function arrowPick(e: KeyboardEvent, ids: string[], current: string, set: (v: string) => void) {
  const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: ids.length - 1, ArrowUp: ids.length - 1 }[e.key];
  if (!step) return;
  e.preventDefault();
  const i = (ids.indexOf(current) + step) % ids.length;
  set(ids[i]);
  const buttons = (e.currentTarget as HTMLElement).querySelectorAll("button");
  tick().then(() => (buttons[i] as HTMLElement | undefined)?.focus());
}

/**
 * `use:wholeNumbers` on a form: a number field left empty or out of range goes back to a whole
 * number in its min/max (emptied: what it held before) ahead of any change handler, so nothing
 * saves null.
 */
export function wholeNumbers(node: HTMLElement) {
  let last = "";
  function fix(e: Event) {
    const el = e.target;
    if (!(el instanceof HTMLInputElement) || el.type !== "number") return;
    if (e.type === "focusin") return void (last = el.value);
    const n = el.valueAsNumber;
    const v = Number.isFinite(n) ? String(Math.min(Number(el.max || Infinity), Math.max(Number(el.min || 0), Math.round(n)))) : last || el.min || "0";
    if (v === el.value) return;
    el.value = v;
    el.dispatchEvent(new Event("input", { bubbles: true }));
  }
  node.addEventListener("focusin", fix);
  node.addEventListener("change", fix, true);
  return {
    destroy() {
      node.removeEventListener("focusin", fix);
      node.removeEventListener("change", fix, true);
    },
  };
}

/** `onclick` on rendered BBCode: its links open in the browser, never inside the app window. */
export function linkClick(e: MouseEvent) {
  const a = (e.target as HTMLElement).closest("a[data-href]");
  if (!a) return;
  e.preventDefault();
  api.openUrl(a.getAttribute("data-href")!);
}
