import type { Renderer, View } from "foliate-js/view.js"
import { type Appearance, type ReaderLayout, readerCss } from "@/lib/appearance"
import type { Direction } from "@/lib/gestures"

type Look = Pick<Appearance["reader"], "flow" | "scheme">
type Placement = Pick<
  ReaderLayout,
  "marginLeft" | "marginRight" | "gapPercent" | "pageMarginRight"
>

/** Lays the open book out for the flow, theme and margins. Safe to call again with no change. */
export function applyAppearance(
  view: View,
  { flow, scheme }: Look,
  { marginLeft, marginRight, gapPercent, pageMarginRight }: Placement,
) {
  const renderer = view.renderer
  if (!renderer) return
  // Setting an attribute, even to the same value, lays the book out again.
  const set = (name: string, value: string) => {
    if (renderer.getAttribute(name) !== value) renderer.setAttribute(name, value)
  }
  set("flow", flow)
  set("animated", "")
  // Top and bottom margins are the wrapper's padding, so they can differ.
  set("margin", "0px")
  set("gap", `${gapPercent}%`)
  view.style.marginLeft = `${marginLeft}px`
  view.style.marginRight = `${marginRight}px`
  const css = readerCss(scheme, { pageMarginRight })
  if (renderer.dataset.css !== css) {
    renderer.dataset.css = css
    renderer.setStyles(css)
  }
}

/** Moves one page or section the way the finger or arrow went, reading direction included. */
export function turn(view: View, direction: Direction, unit: "page" | "section") {
  if (unit === "page") {
    return direction === "left" ? view.goLeft() : view.goRight()
  }
  const renderer = view.renderer
  if (!renderer) return
  const forward = (direction === "right") !== (view.book?.dir === "rtl")
  return forward ? renderer.nextSection() : renderer.prevSection()
}

/** How far through the current section the view is scrolled, 0–1. */
export function sectionFraction(renderer: Renderer): number {
  const room = renderer.viewSize - renderer.size
  if (room <= 0) return 1
  return Math.min(1, Math.max(0, renderer.start / room))
}
