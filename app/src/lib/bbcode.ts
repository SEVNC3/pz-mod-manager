// Steam Workshop descriptions are BBCode. This turns them into safe HTML: the text is
// escaped first, so only the tags made here can appear. Links carry data-href and are
// opened by the app (never navigated in the window); only http(s) URLs survive.

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const safeUrl = (u: string) => (/^https?:\/\/[^\s"<>]+$/i.test(u) ? u : null);

export function bbcode(src: string): string {
  // [noparse] and [code] keep their contents verbatim
  const kept: string[] = [];
  let s = src.replace(/\[(noparse|code)\]([\s\S]*?)\[\/\1\]/gi, (_, tag: string, body: string) => {
    kept.push(tag.toLowerCase() === "code" ? `<pre>${esc(body)}</pre>` : esc(body));
    return `\u0000${kept.length - 1}\u0000`;
  });
  s = esc(s);
  const link = (url: string, label: string) => {
    const u = safeUrl(url.replace(/&amp;/g, "&"));
    return u ? `<a href="#" data-href="${esc(u)}">${label}</a>` : label;
  };
  const rules: [RegExp, string | ((...m: string[]) => string)][] = [
    [/\[h1\]([\s\S]*?)\[\/h1\]/gi, "<h3>$1</h3>"],
    [/\[h2\]([\s\S]*?)\[\/h2\]/gi, "<h4>$1</h4>"],
    [/\[h3\]([\s\S]*?)\[\/h3\]/gi, "<h5>$1</h5>"],
    [/\[b\]([\s\S]*?)\[\/b\]/gi, "<strong>$1</strong>"],
    [/\[i\]([\s\S]*?)\[\/i\]/gi, "<em>$1</em>"],
    [/\[u\]([\s\S]*?)\[\/u\]/gi, "<u>$1</u>"],
    [/\[(?:strike|s)\]([\s\S]*?)\[\/(?:strike|s)\]/gi, "<s>$1</s>"],
    [/\[spoiler\]([\s\S]*?)\[\/spoiler\]/gi, '<span class="spoiler">$1</span>'],
    [/\[quote(?:=[^\]]*)?\]([\s\S]*?)\[\/quote\]/gi, "<blockquote>$1</blockquote>"],
    [/\[hr\]\s*(?:\[\/hr\])?/gi, "<hr>"],
    [/\[img\]\s*([^\[\s]+?)\s*\[\/img\]/gi, (_m, u) => (safeUrl(u.replace(/&amp;/g, "&")) ? `<img src="${u}" alt="" loading="lazy">` : "")],
    [/\[url=([^\]]+)\]([\s\S]*?)\[\/url\]/gi, (_m, u, label) => link(u.trim(), label)],
    [/\[url\]([\s\S]*?)\[\/url\]/gi, (_m, u) => link(u.trim(), u)],
    [/\[\*\]([\s\S]*?)(?=\[\*\]|\[\/list\]|\[\/olist\]|$)/gi, "<li>$1</li>"],
    [/\[list\]([\s\S]*?)\[\/list\]/gi, "<ul>$1</ul>"],
    [/\[olist\]([\s\S]*?)\[\/olist\]/gi, "<ol>$1</ol>"],
    [/\[table[^\]]*\]([\s\S]*?)\[\/table\]/gi, "<table>$1</table>"],
    [/\[tr\]([\s\S]*?)\[\/tr\]/gi, "<tr>$1</tr>"],
    [/\[(th|td)\]([\s\S]*?)\[\/\1\]/gi, "<$1>$2</$1>"],
  ];
  // repeat so nested tags resolve ([b][i]x[/i][/b])
  for (let pass = 0; pass < 3; pass++) for (const [re, to] of rules) s = s.replace(re, to as string);
  // bare links
  s = s.replace(/(^|[\s(])(https?:\/\/[^\s<"]+)/g, (_m, pre, u) => pre + link(u, u));
  // anything left that looks like a tag is noise
  s = s.replace(/\[\/?[a-z0-9*]+(?:=[^\]]*)?\]/gi, "");
  s = s.replace(/\n/g, "<br>").replace(/<br>\s*(<\/?(?:ul|ol|li|h3|h4|h5|blockquote|table|tr|td|th|hr|pre)[^>]*>)/g, "$1").replace(/(<\/?(?:ul|ol|li|h3|h4|h5|blockquote|table|tr|hr|pre)[^>]*>)\s*<br>/g, "$1");
  return s.replace(/\u0000(\d+)\u0000/g, (_m, i) => kept[Number(i)]);
}
