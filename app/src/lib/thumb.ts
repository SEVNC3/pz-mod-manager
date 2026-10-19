// Remote pictures (Workshop previews and screenshots, YouTube thumbnails): `<img use:thumb={text} src=...>`.
// A placeholder colour while loading, one retry without lazy loading, then "image failed to load" drawn in the
// image's own box (so each page's image CSS still fits it), and the URL and host in the console for bug reports.

/** The failed picture: `text` on the placeholder colour, 16:9. */
export function failedPicture(text: string): string {
  const esc = text.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
  return `data:image/svg+xml,${encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 90"><rect width="160" height="90" fill="#8882"/><text x="80" y="49" font-size="9" text-anchor="middle" fill="#999" font-family="sans-serif">${esc}</text></svg>`,
  )}`;
}

export function thumb(img: HTMLImageElement, text: string) {
  img.style.backgroundColor = "var(--raised)";
  let retried = "";
  const onerror = () => {
    const url = img.getAttribute("src") ?? "";
    if (!url || url.startsWith("data:")) return;
    if (retried !== url) {
      // once more, at once (a lazy image the webview gave up on, a blip on Steam's CDN)
      retried = url;
      img.loading = "eager";
      img.removeAttribute("src");
      queueMicrotask(() => img.setAttribute("src", url));
      return;
    }
    let host = "";
    try {
      host = new URL(url).host;
    } catch {}
    console.warn("PZMM: image failed to load", url, host);
    img.src = failedPicture(text);
  };
  img.addEventListener("error", onerror);
  return {
    update(t: string) {
      text = t;
    },
    destroy() {
      img.removeEventListener("error", onerror);
    },
  };
}
