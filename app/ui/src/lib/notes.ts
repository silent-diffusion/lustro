// Release notes are untrusted network input. This renders a small, safe Markdown subset:
// `#` headings, `-`/`*` bullets, paragraphs, **bold** and `code`. Nothing else is interpreted:
// raw HTML stays literal text, and links become their label as plain text (never clickable).
//
// The UI renders the parsed blocks as Svelte text nodes (components/Notes.svelte), so no string from the
// notes ever reaches innerHTML. `notesToHtml` escapes every piece first, for callers that need markup.

export type Inline = { t: "text" | "b" | "code"; s: string };
export type Block = { t: "h"; c: Inline[] } | { t: "p"; c: Inline[] } | { t: "ul"; items: Inline[][] };

/** Generous cap: release notes are a few hundred words. */
const MAX_CHARS = 20_000;

// Strip control characters (keep tab), and the bidi overrides that can disguise text.
const UNSAFE_CHARS = /[\u0000-\u0008\u000b-\u001f\u007f‪-‮⁦-⁩]/g;

/** `[label](anything)` → label. Images `![alt](src)` → alt. The target is dropped entirely. */
function stripLinks(s: string): string {
  return s.replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1");
}

export function parseInline(src: string): Inline[] {
  const s = stripLinks(src);
  const out: Inline[] = [];
  const re = /\*\*(.+?)\*\*|`([^`]+)`/g;
  let last = 0;
  for (let m = re.exec(s); m; m = re.exec(s)) {
    if (m.index > last) out.push({ t: "text", s: s.slice(last, m.index) });
    out.push(m[1] !== undefined ? { t: "b", s: m[1] } : { t: "code", s: m[2] });
    last = m.index + m[0].length;
  }
  if (last < s.length) out.push({ t: "text", s: s.slice(last) });
  return out;
}

export function parseNotes(src: string): Block[] {
  const text = (src ?? "").slice(0, MAX_CHARS).replace(/\r\n?/g, "\n").replace(UNSAFE_CHARS, "");
  const blocks: Block[] = [];
  let list: Inline[][] | null = null;
  let para: string[] = [];
  const flushPara = () => {
    if (para.length) blocks.push({ t: "p", c: parseInline(para.join(" ")) });
    para = [];
  };
  for (const line of text.split("\n")) {
    const li = line.match(/^\s*[-*]\s+(.*)$/);
    const h = line.match(/^#{1,6}\s+(.*)$/);
    if (!li && list) { blocks.push({ t: "ul", items: list }); list = null; }
    if (h) { flushPara(); blocks.push({ t: "h", c: parseInline(h[1]) }); }
    else if (li) { flushPara(); (list ??= []).push(parseInline(li[1])); }
    else if (line.trim()) para.push(line.trim());
    else flushPara();
  }
  flushPara();
  if (list) blocks.push({ t: "ul", items: list });
  return blocks;
}

export function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
}

function inlineHtml(c: Inline[]): string {
  return c.map((x) => (x.t === "b" ? `<b>${escapeHtml(x.s)}</b>` : x.t === "code" ? `<code>${escapeHtml(x.s)}</code>` : escapeHtml(x.s))).join("");
}

/** Escaped HTML for the subset above. Only <h4>, <p>, <ul>, <li>, <b> and <code> can appear. */
export function notesToHtml(src: string): string {
  return parseNotes(src).map((b) =>
    b.t === "h" ? `<h4>${inlineHtml(b.c)}</h4>`
    : b.t === "p" ? `<p>${inlineHtml(b.c)}</p>`
    : `<ul>${b.items.map((i) => `<li>${inlineHtml(i)}</li>`).join("")}</ul>`).join("");
}

/** Plain text (for a one-line summary or a screen reader label). */
export function notesToText(src: string): string {
  return parseNotes(src).map((b) => (b.t === "ul" ? b.items.map((i) => "• " + i.map((x) => x.s).join("")).join("\n") : b.c.map((x) => x.s).join(""))).join("\n");
}
