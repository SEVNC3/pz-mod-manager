// The one way PZMM asks before acting: a strip above the tabs (drawn by the main page), so any
// view can ask without the browser's own confirm box (which ignores the theme).
import type { IconName } from "./Icon.svelte";

export type Choice = { label: string; icon: IconName };
type Ask = { title: string; lines: string[]; choices: Choice[]; background: boolean; done: (i: number) => void };

export const asking = $state<{ now: Ask | null }>({ now: null });

/** Show `lines` with a button per choice (the first is the main one) and Cancel; resolves to the choice, -1 for Cancel.
 * `background`: nobody just asked for it (a scheduled job), so focus stays where it is (a key meant elsewhere doesn't answer it). */
export function ask(title: string, lines: string[], choices: Choice[], background = false): Promise<number> {
  asking.now?.done(-1);
  return new Promise((done) => (asking.now = { title, lines, choices, background, done }));
}

export function answer(i: number) {
  const a = asking.now;
  asking.now = null;
  a?.done(i);
}

/** A yes-or-cancel question: true when `yes` is picked. */
export const confirmed = async (question: string, yes: Choice, lines: string[] = [], background = false) => (await ask(question, lines, [yes], background)) === 0;
