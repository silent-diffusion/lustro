import { describe, expect, it } from "vitest";
import { notesToHtml, notesToText, parseNotes } from "../lib/notes";

// Release notes come from the network: nothing in them may become live markup or a link.
const TAGS = /<(?!\/?(h4|p|ul|li|b|code)>)/i;

describe("release notes renderer", () => {
  it("renders the safe subset", () => {
    const html = notesToHtml("## What's new\n- **Faster** scans\n- `Copy path` fixed\n\nSigned updates only.");
    expect(html).toBe("<h4>What&#39;s new</h4><ul><li><b>Faster</b> scans</li><li><code>Copy path</code> fixed</li></ul><p>Signed updates only.</p>");
  });

  it("neutralizes <script>", () => {
    const src = "- fix <script>alert(1)</script>\n<script src=x></script>";
    const html = notesToHtml(src);
    expect(html).not.toMatch(/<script/i);
    expect(html).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(html).not.toMatch(TAGS);
    // The text is kept, as text.
    expect(notesToText(src)).toContain("<script>alert(1)</script>");
  });

  it("neutralizes <img onerror> and other raw HTML", () => {
    const src = '<img src=x onerror="alert(1)">\n- <a href="https://example.invalid">click</a> <iframe></iframe>\n**<b onmouseover=x>bold</b>**';
    const html = notesToHtml(src);
    expect(html).not.toMatch(/<img|<a\s|<iframe|<b\s/i);
    // Attribute text survives only as escaped text, never inside a tag.
    expect(html).not.toMatch(/<[^>]*onerror/i);
    expect(html).not.toMatch(TAGS);
    expect(html).toContain("&lt;img src=x onerror=&quot;alert(1)&quot;&gt;");
  });

  it("turns links into their label as plain text, including javascript: links", () => {
    const src = "- See [the docs](https://example.invalid/docs) and [run](javascript:alert(1))\n![pic](javascript:alert(2))";
    const html = notesToHtml(src);
    expect(html).not.toMatch(/javascript:/i);
    expect(html).not.toMatch(/href|<a/i);
    expect(html).toContain("See the docs and run");
    const blocks = parseNotes(src);
    expect(JSON.stringify(blocks)).not.toMatch(/javascript:|https:/);
  });

  it("keeps markup inside bold and code escaped", () => {
    const html = notesToHtml("**<i>x</i>** `<svg onload=1>`");
    expect(html).toBe("<p><b>&lt;i&gt;x&lt;/i&gt;</b> <code>&lt;svg onload=1&gt;</code></p>");
  });

  it("strips control and bidi-override characters and caps the length", () => {
    expect(notesToText("a‮b\u0000c")).toBe("abc");
    expect(notesToText("x".repeat(50_000)).length).toBe(20_000);
  });

  it("handles empty or odd input", () => {
    expect(parseNotes("")).toEqual([]);
    expect(notesToHtml("#not a heading\n*not a bullet")).toBe("<p>#not a heading *not a bullet</p>");
  });
});
