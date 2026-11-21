import { create } from "@bufbuild/protobuf"
import { describe, expect, it } from "vitest"
import { AnnotationSchema, HighlightColor_Value } from "@/gen/bookkeeperpb/annotation_pb"
import {
  byPosition,
  clampChars,
  excerpt,
  groupByChapter,
  highlightColor,
  MAX_TEXT_CHARS,
  placeToolbar,
} from "./highlights"

const at = (cfi: string, chapter = "One", created = "2026-11-20T09:00:00Z") =>
  create(AnnotationSchema, { id: cfi, cfi, chapter, created, updated: created })

describe("byPosition", () => {
  it("sorts by place in the book, then by age", () => {
    const later = at("epubcfi(/6/4!/4/2,/1:0,/1:5)", "One", "2026-11-20T10:00:00Z")
    const list = [
      at("epubcfi(/6/8!/4/2,/1:0,/1:5)"),
      later,
      at("epubcfi(/6/4!/4/2,/1:0,/1:5)"),
      at("epubcfi(/6/4!/4/10,/1:0,/1:5)"),
    ]

    expect(byPosition(list).map((a) => [a.cfi, a.created])).toEqual([
      ["epubcfi(/6/4!/4/2,/1:0,/1:5)", "2026-11-20T09:00:00Z"],
      ["epubcfi(/6/4!/4/2,/1:0,/1:5)", "2026-11-20T10:00:00Z"],
      ["epubcfi(/6/4!/4/10,/1:0,/1:5)", "2026-11-20T09:00:00Z"],
      ["epubcfi(/6/8!/4/2,/1:0,/1:5)", "2026-11-20T09:00:00Z"],
    ])
  })

  it("compares offsets within a text node", () => {
    const list = [at("epubcfi(/6/4!/4/2,/1:20,/1:25)"), at("epubcfi(/6/4!/4/2,/1:3,/1:9)")]
    expect(byPosition(list).map((a) => a.cfi)).toEqual([
      "epubcfi(/6/4!/4/2,/1:3,/1:9)",
      "epubcfi(/6/4!/4/2,/1:20,/1:25)",
    ])
  })
})

describe("groupByChapter", () => {
  it("groups consecutive highlights of a chapter", () => {
    const groups = groupByChapter([at("a", "One"), at("b", "One"), at("c", "Two"), at("d", "One")])
    expect(groups.map((g) => [g.chapter, g.items.map((a) => a.id)])).toEqual([
      ["One", ["a", "b"]],
      ["Two", ["c"]],
      ["One", ["d"]],
    ])
  })
})

describe("text limits", () => {
  it("counts characters, not UTF-16 units", () => {
    expect(clampChars("😀😀😀", 2)).toBe("😀😀")
    expect(clampChars("abc", 3)).toBe("abc")
  })

  it("makes an excerpt one line within the backend's limit", () => {
    expect(excerpt("  It was\n\tthe best  ")).toBe("It was the best")
    expect([...excerpt("x".repeat(MAX_TEXT_CHARS + 5))]).toHaveLength(MAX_TEXT_CHARS)
  })
})

describe("highlightColor", () => {
  it("falls back to yellow for a value it doesn't know", () => {
    expect(highlightColor(HighlightColor_Value.BLUE).label).toBe("Blue")
    expect(highlightColor(42 as HighlightColor_Value).label).toBe("Yellow")
  })
})

describe("placeToolbar", () => {
  const page = { width: 400, height: 800 }
  const toolbar = { width: 200, height: 40 }

  it("centres it above the selection", () => {
    const target = { left: 100, top: 300, right: 200, bottom: 320 }
    expect(placeToolbar(target, page, toolbar)).toEqual({ left: 50, top: 252 })
  })

  it("puts it below when there's no room above, header included", () => {
    const target = { left: 100, top: 70, right: 200, bottom: 90 }
    expect(placeToolbar(target, page, toolbar, 48)).toEqual({ left: 50, top: 98 })
  })

  it("keeps it inside the page", () => {
    expect(placeToolbar({ left: 0, top: 300, right: 10, bottom: 320 }, page, toolbar).left).toBe(8)
    expect(
      placeToolbar({ left: 390, top: 300, right: 400, bottom: 320 }, page, toolbar).left,
    ).toBe(192)
    const low = { left: 100, top: 10, right: 200, bottom: 790 }
    expect(placeToolbar(low, page, toolbar).top).toBe(752)
  })
})
