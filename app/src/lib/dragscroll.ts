/** How close to the scroll box's top or bottom edge (px) a dragged row starts it scrolling. */
const EDGE = 56;
/** The fastest it scrolls (px per frame), with the pointer right at the edge. */
const MAX = 18;

/** How far to scroll this frame with the pointer at `y`, in a box spanning `top`..`bottom` (negative: up). */
export function edgeSpeed(y: number, top: number, bottom: number): number {
  const near = y - top < EDGE ? -(EDGE - (y - top)) : bottom - y < EDGE ? EDGE - (bottom - y) : 0;
  return Math.sign(near) * Math.ceil(MAX * Math.min(1, Math.abs(near) / EDGE));
}

/** The element that scrolls `el`: the nearest ancestor with overflowing auto/scroll content, else the page. */
function scroller(el: HTMLElement | null): HTMLElement {
  for (; el; el = el.parentElement) {
    const o = getComputedStyle(el).overflowY;
    if ((o === "auto" || o === "scroll") && el.scrollHeight > el.clientHeight) return el;
  }
  return document.scrollingElement as HTMLElement;
}

/**
 * Edge auto-scroll while one of `node`'s rows is dragged (native drag and drop). WebView2's drag loop swallows the
 * mouse wheel, so without it a row can't reach a far group. Driven by dragover, which keeps firing while the mouse is
 * still, and a frame loop; it stops on dragend, drop, or the pointer leaving the scroll box.
 */
export function dragScroll(node: HTMLElement) {
  let box: HTMLElement | null = null;
  let speed = 0;
  let frame = 0;
  const step = () => {
    if (!speed || !box) return void (frame = 0);
    box.scrollTop += speed;
    frame = requestAnimationFrame(step);
  };
  const over = (e: DragEvent) => {
    if (!box) return;
    const page = box === document.scrollingElement;
    const r = page ? { top: 0, bottom: innerHeight } : box.getBoundingClientRect();
    speed = edgeSpeed(e.clientY, Math.max(r.top, 0), Math.min(r.bottom, innerHeight));
    if (speed && !frame) frame = requestAnimationFrame(step);
  };
  const leave = (e: DragEvent) => {
    if (!(e.relatedTarget instanceof Node && box?.contains(e.relatedTarget))) speed = 0;
  };
  const target = () => (box === document.scrollingElement ? document : box!);
  const stop = () => {
    if (box) {
      target().removeEventListener("dragover", over as EventListener);
      target().removeEventListener("dragleave", leave as EventListener);
      target().removeEventListener("drop", stop);
    }
    box = null;
    speed = 0;
    cancelAnimationFrame(frame);
    frame = 0;
  };
  const start = () => {
    stop();
    box = scroller(node.parentElement);
    target().addEventListener("dragover", over as EventListener);
    target().addEventListener("dragleave", leave as EventListener);
    target().addEventListener("drop", stop);
  };
  node.addEventListener("dragstart", start);
  node.addEventListener("dragend", stop);
  return {
    destroy() {
      stop();
      node.removeEventListener("dragstart", start);
      node.removeEventListener("dragend", stop);
    },
  };
}
