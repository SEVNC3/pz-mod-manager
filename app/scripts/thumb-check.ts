// Self-check for src/lib/thumb.ts. Run: node scripts/thumb-check.ts
import assert from "node:assert/strict";
import { failedPicture, thumb } from "../src/lib/thumb.ts";

/** Just enough of an <img> for the action. */
function fakeImg(src: string) {
  const attrs = new Map([["src", src]]);
  let onerror: () => void = () => {};
  const img = {
    style: { backgroundColor: "" },
    loading: "lazy",
    getAttribute: (k: string) => attrs.get(k) ?? null,
    setAttribute: (k: string, v: string) => attrs.set(k, v),
    removeAttribute: (k: string) => attrs.delete(k),
    set src(v: string) { attrs.set("src", v); },
    get src() { return attrs.get("src") ?? ""; },
    addEventListener: (_: string, f: () => void) => (onerror = f),
    removeEventListener: () => {},
    fail: () => onerror(),
  };
  return img;
}

const warned: unknown[][] = [];
console.warn = (...a: unknown[]) => warned.push(a);
const url = "https://images.steamusercontent.com/ugc/1/preview.jpg";
const img = fakeImg(url);
thumb(img as unknown as HTMLImageElement, "Image failed to load");
assert.equal(img.style.backgroundColor, "var(--raised)", "a placeholder while it loads");
// first failure: tried once more, at once
img.fail();
assert.equal(img.loading, "eager");
await Promise.resolve();
assert.equal(img.src, url, "the same picture, again");
assert.equal(warned.length, 0);
// second failure: said in the console, and drawn in its box
img.fail();
assert.deepEqual(warned, [["PZMM: image failed to load", url, "images.steamusercontent.com"]]);
assert.equal(img.src, failedPicture("Image failed to load"));
assert.ok(decodeURIComponent(img.src).includes("Image failed to load"));
// the failed picture itself never loops
img.fail();
assert.equal(warned.length, 1);
// a new picture in the same <img> gets its own retry
img.src = "https://img.youtube.com/vi/x/mqdefault.jpg";
img.fail();
await Promise.resolve();
assert.equal(warned.length, 1);
assert.ok(decodeURIComponent(failedPicture("<b>&")).includes("&#60;b&#62;&#38;"), "text is escaped");
console.log("thumb-check: ok");
