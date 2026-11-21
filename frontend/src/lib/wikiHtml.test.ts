// @vitest-environment happy-dom
import { describe, expect, it } from "vitest"
import { parseWikiHtml, plainText } from "./wikiHtml"

describe("parseWikiHtml", () => {
  it("keeps links to entries, and only their text for other links", () => {
    const html =
      '<a rel="mw:WikiLink" href="/wiki/run" title="run">run</a> ' +
      '<a rel="mw:WikiLink" href="/wiki/Appendix:Glossary" title="Appendix:Glossary">participle</a> ' +
      '<a href="https://example.com">out</a>'
    expect(parseWikiHtml(html)).toEqual([
      { kind: "link", title: "run", children: [{ kind: "text", text: "run" }] },
      { kind: "text", text: " participle out" },
    ])
  })

  it("drops scripts, images and references with their content", () => {
    const html = 'A<script>alert(1)</script><img src="x" onerror="alert(2)"><sup>[1]</sup>B'
    expect(parseWikiHtml(html)).toEqual([{ kind: "text", text: "AB" }])
  })

  it("unwraps spans and keeps formatting", () => {
    expect(parseWikiHtml('<span class="x">a <b>bold</b> <i>it</i></span>')).toEqual([
      { kind: "text", text: "a " },
      { kind: "bold", children: [{ kind: "text", text: "bold" }] },
      { kind: "text", text: " " },
      { kind: "italic", children: [{ kind: "text", text: "it" }] },
    ])
  })

  it("keeps nested lists of sub-senses", () => {
    const nodes = parseWikiHtml("Moving.\n<ol><li>Of a horse.</li><li>Of water.</li></ol>")
    expect(nodes).toEqual([
      { kind: "text", text: "Moving.\n" },
      {
        kind: "list",
        ordered: true,
        items: [[{ kind: "text", text: "Of a horse." }], [{ kind: "text", text: "Of water." }]],
      },
    ])
    expect(plainText(nodes)).toBe("Moving.\nOf a horse. Of water.")
  })

  it("finds no text in an empty definition", () => {
    expect(plainText(parseWikiHtml('<span typeof="mw:Transclusion"></span> '))).toBe(" ")
  })
})
