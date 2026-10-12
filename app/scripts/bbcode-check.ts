// Self-check for src/lib/bbcode.ts. Run: node scripts/bbcode-check.ts
import assert from "node:assert/strict";
import { bbcode, markdown } from "../src/lib/bbcode.ts";

assert.equal(bbcode("[b]Bold[/b] and [i]it[/i]"), "<strong>Bold</strong> and <em>it</em>");
assert.equal(bbcode("<script>alert(1)</script>"), "&lt;script&gt;alert(1)&lt;/script&gt;", "HTML is escaped");
assert.equal(bbcode("[url=javascript:alert(1)]x[/url]"), "x", "only http(s) links");
assert.equal(bbcode("[url=https://a.b/c?d=1&e=2]site[/url]"), '<a href="#" data-href="https://a.b/c?d=1&amp;e=2">site</a>');
assert.equal(bbcode("see https://x.y/z"), 'see <a href="#" data-href="https://x.y/z">https://x.y/z</a>');
assert.equal(bbcode("[list][*]one\n[*]two\n[/list]"), "<ul><li>one</li><li>two</li></ul>");
assert.equal(bbcode("[img]https://i.png[/img]"), '<img src="https://i.png" alt="" loading="lazy">');
assert.equal(bbcode("[img]file:///c:/x[/img]"), "");
assert.equal(bbcode("[code]<b>[b]x[/b]</b>[/code]"), "<pre>&lt;b&gt;[b]x[/b]&lt;/b&gt;</pre>");
assert.equal(bbcode("[h1]Title[/h1]\nline\n[unknowntag]kept[/unknowntag]"), "<h3>Title</h3>line<br>kept");
assert.equal(bbcode("[url=http://a.b/c]x[/url]"), '<a href="#" data-href="https://a.b/c">x</a>', "http links are upgraded");
assert.equal(
  markdown("### New\n- **Faster** radio\n- fixed [B42] maps, *really*\nsee [the site](https://a.b) <b>"),
  '<h5>New</h5><ul><li><strong>Faster</strong> radio</li><li>fixed [B42] maps, <em>really</em></li></ul>see <a href="#" data-href="https://a.b">the site</a> &lt;b&gt;',
);
assert.equal(markdown("Plain line\nnext"), "Plain line<br>next");
console.log("bbcode-check: ok");
