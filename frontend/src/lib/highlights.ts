import { compare } from "foliate-js/epubcfi.js"
import { Overlayer } from "foliate-js/overlayer.js"
import { type Annotation, HighlightColor_Value } from "@/gen/bookkeeperpb/annotation_pb"
import type { Scheme } from "@/lib/appearance"

/** The backend's limits, in characters (`crates/core/src/library/annotations.rs`). */
export const MAX_TEXT_CHARS = 2000
export const MAX_CHAPTER_CHARS = 500
export const MAX_NOTE_CHARS = 10000

type HighlightColor = {
  value: HighlightColor_Value
  label: string
  /** For the UI: the toolbar's dots and the list's bars. */
  swatch: string
  /** Drawn over the page, per scheme. */
  page: Record<Scheme, string>
}

export const HIGHLIGHT_COLORS: HighlightColor[] = [
  {
    value: HighlightColor_Value.YELLOW,
    label: "Yellow",
    swatch: "#facc15",
    page: { light: "#ffdf3d", dark: "#8a6d00" },
  },
  {
    value: HighlightColor_Value.GREEN,
    label: "Green",
    swatch: "#4ade80",
    page: { light: "#7fe08f", dark: "#2c7a3a" },
  },
  {
    value: HighlightColor_Value.BLUE,
    label: "Blue",
    swatch: "#60a5fa",
    page: { light: "#82bfff", dark: "#2a5ca6" },
  },
  {
    value: HighlightColor_Value.PINK,
    label: "Pink",
    swatch: "#f472b6",
    page: { light: "#ff9ccb", dark: "#9a3769" },
  },
]

/** A value this build doesn't know shows as yellow. */
export const highlightColor = (value: HighlightColor_Value): HighlightColor =>
  HIGHLIGHT_COLORS.find((c) => c.value === value) ?? HIGHLIGHT_COLORS[0]

/**
 * How highlights mix with the page. Multiply keeps dark text dark on a light page; on a dark one
 * it would hide the highlight, so there the lighter of the two shows, which keeps light text light.
 */
export function highlightStyle(scheme: Scheme): Record<string, string> {
  return scheme === "light"
    ? { "--overlayer-highlight-blend-mode": "multiply", "--overlayer-highlight-opacity": "0.45" }
    : { "--overlayer-highlight-blend-mode": "lighten", "--overlayer-highlight-opacity": "0.7" }
}

/** At most `n` characters, counting as the backend does (code points, not UTF-16 units). */
export function clampChars(s: string, n: number): string {
  const chars = [...s]
  return chars.length <= n ? s : chars.slice(0, n).join("")
}

/** The selection's text as one line. */
export const excerpt = (text: string) =>
  clampChars(text.replace(/\s+/g, " ").trim(), MAX_TEXT_CHARS)

/** In reading order; highlights at the same place, oldest first. */
export function byPosition(list: readonly Annotation[]): Annotation[] {
  return [...list].sort((a, b) => compare(a.cfi, b.cfi) || a.created.localeCompare(b.created))
}

export type ChapterGroup = { chapter: string; items: Annotation[] }

/** Runs of consecutive highlights in the same chapter, so a chapter read twice shows twice. */
export function groupByChapter(sorted: readonly Annotation[]): ChapterGroup[] {
  const groups: ChapterGroup[] = []
  for (const item of sorted) {
    const last = groups.at(-1)
    if (last?.chapter === item.chapter) last.items.push(item)
    else groups.push({ chapter: item.chapter, items: [item] })
  }
  return groups
}

type Box = { left: number; top: number; right: number; bottom: number }
type Size = { width: number; height: number }

/** px kept between the toolbar and the selection, and between the toolbar and the page's edges. */
const GAP = 8

/**
 * Where the toolbar goes, in the page's coordinates: centred above `target`, or below it when
 * there's no room above (`clearTop` keeps it off the header), and always inside the page.
 */
export function placeToolbar(
  target: Box,
  page: Size,
  toolbar: Size,
  clearTop = 0,
): { left: number; top: number } {
  const centre = (target.left + target.right) / 2
  const left = clamp(centre - toolbar.width / 2, GAP, page.width - toolbar.width - GAP)
  const above = target.top - GAP - toolbar.height
  const top =
    above >= clearTop + GAP
      ? above
      : clamp(target.bottom + GAP, clearTop + GAP, page.height - toolbar.height - GAP)
  return { left, top }
}

// When the page is too small, the start edge wins.
const clamp = (v: number, min: number, max: number) => Math.max(min, Math.min(v, max))

/** foliate's highlight, plus a dot where it ends when it has a note. */
export function drawHighlight(
  rects: DOMRectList,
  { color, note }: { color: string; note: boolean },
): SVGElement {
  const highlight = Overlayer.highlight(rects, { color })
  const last = rects[rects.length - 1]
  if (!note || !last) return highlight
  const g = document.createElementNS("http://www.w3.org/2000/svg", "g")
  const dot = document.createElementNS("http://www.w3.org/2000/svg", "circle")
  dot.setAttribute("cx", String(last.right))
  dot.setAttribute("cy", String(last.top))
  dot.setAttribute("r", "4")
  dot.setAttribute("fill", color)
  g.append(highlight, dot)
  return g
}
