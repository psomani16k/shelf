import {
  type Config,
  ReaderFlow_Value,
  ReaderMargin_Value,
  UiTheme_Value,
} from "@/gen/configpb/config_pb"

export type Scheme = "light" | "dark"
type Flow = "scrolled" | "paginated"

type MarginSize = "none" | "small" | "medium" | "large"
export type Margins = {
  top: MarginSize
  bottom: MarginSize
  side: MarginSize
  betweenPages: MarginSize
}

export type Appearance = {
  ui: Scheme
  /** The reader's theme is a list of themes, resolved by `useReaderLook`. */
  reader: { flow: Flow; margins: Margins }
}

const MARGIN_SIZES: Record<ReaderMargin_Value, MarginSize> = {
  [ReaderMargin_Value.NONE]: "none",
  [ReaderMargin_Value.SMALL]: "small",
  [ReaderMargin_Value.MEDIUM]: "medium",
  [ReaderMargin_Value.LARGE]: "large",
}
const marginSize = (value: ReaderMargin_Value | undefined) =>
  MARGIN_SIZES[value ?? ReaderMargin_Value.MEDIUM]

/** Unset fields mean the proto zero value: system theme, scrolled, medium margins. */
export function resolveAppearance(config: Config | undefined, system: Scheme): Appearance {
  const uiTheme = config?.ui?.theme
  const ui =
    uiTheme === UiTheme_Value.LIGHT ? "light" : uiTheme === UiTheme_Value.DARK ? "dark" : system

  const flow = config?.reader?.flow === ReaderFlow_Value.PAGINATED ? "paginated" : "scrolled"

  const m = config?.reader?.margins
  const margins = {
    top: marginSize(m?.top),
    bottom: marginSize(m?.bottom),
    side: marginSize(m?.side),
    betweenPages: marginSize(m?.betweenPages),
  }

  return { ui, reader: { flow, margins } }
}

/** Each preset is a share of the screen, kept within [min, max] px. */
type Preset = { share: number; min: number; max: number } | null

const SIDE: Record<MarginSize, Preset> = {
  none: null,
  small: { share: 0.02, min: 8, max: 24 },
  medium: { share: 0.04, min: 16, max: 48 },
  large: { share: 0.07, min: 28, max: 96 },
}
const BETWEEN_PAGES: Record<MarginSize, Preset> = {
  none: null,
  small: { share: 0.02, min: 12, max: 32 },
  medium: { share: 0.04, min: 24, max: 64 },
  large: { share: 0.07, min: 40, max: 112 },
}
const TOP_BOTTOM: Record<MarginSize, Preset> = {
  none: null,
  small: { share: 0.02, min: 8, max: 24 },
  medium: { share: 0.05, min: 24, max: 48 },
  large: { share: 0.09, min: 40, max: 96 },
}

const px = (preset: Preset, length: number) =>
  preset ? Math.round(Math.min(preset.max, Math.max(preset.min, preset.share * length))) : 0

/** Wider than any scrollbar, so pushing the view this far past the wrapper's edge hides it. */
export const SCROLLBAR_ALLOWANCE = 24

export type ReaderLayout = {
  paddingTop: number
  paddingBottom: number
  /** On `<foliate-view>`; negative widens it past the wrapper. */
  marginLeft: number
  marginRight: number
  /** foliate's `gap` attribute, in %. */
  gapPercent: number
  /** On the page's `<html>`, so text keeps clear of the strip hidden past the wrapper. */
  pageMarginRight: number
}

/**
 * foliate's `gap` sets the side margins and the space between pages to the same `gap × width`.
 * Widening or narrowing the view by `side − between` on each side splits the two apart.
 * `clipScrollbar` pushes the scrolled view's right edge, and its scrollbar, past the wrapper's
 * edge, where `overflow: hidden` clips it; the page itself gets a bigger right margin instead.
 * `inset` is kept clear on both sides, on top of the margin setting, e.g. for floating buttons.
 */
export function readerLayout(
  size: { width: number; height: number },
  { flow, margins }: Pick<Appearance["reader"], "flow" | "margins">,
  { clipScrollbar = false, inset = 0 } = {},
): ReaderLayout {
  const side = inset + px(SIDE[margins.side], size.width)
  if (flow === "scrolled") {
    return {
      paddingTop: 0,
      paddingBottom: 0,
      marginLeft: side,
      marginRight: clipScrollbar ? -SCROLLBAR_ALLOWANCE : side,
      gapPercent: 0,
      pageMarginRight: clipScrollbar ? side + SCROLLBAR_ALLOWANCE : 0,
    }
  }
  const between = px(BETWEEN_PAGES[margins.betweenPages], size.width)
  const viewWidth = size.width - 2 * side + 2 * between
  return {
    paddingTop: px(TOP_BOTTOM[margins.top], size.height),
    paddingBottom: px(TOP_BOTTOM[margins.bottom], size.height),
    marginLeft: side - between,
    marginRight: side - between,
    gapPercent: viewWidth > 0 ? (100 * between) / viewWidth : 0,
    pageMarginRight: 0,
  }
}

/**
 * Styles for each book page, drawn as published: the theme is a filter over the body, and its
 * variables are set on each page's `<html>` by `applyLook`, so changing it needs no new styles.
 * The page is transparent: the reader's wrapper behind the iframe has the theme's page colour,
 * which foliate's own background (copied from the page only when its styles are set) can't keep.
 * `zoom` scales the body, not `<html>`, whose columns foliate sizes in px. Images undo it:
 * foliate caps their height to the page in px, which zoom would scale past the page.
 */
export function readerCss({ pageMarginRight = 0, zoom = 1 } = {}): string {
  let css = `
    html { color-scheme: light; color: #000; background: transparent !important; }
    html { margin-right: ${pageMarginRight}px !important; }
    body { background: transparent !important; filter: var(--shelf-filter, none) !important; }
    body { hyphens: auto; -webkit-hyphens: auto; }
    img, video, canvas, svg:has(image) { filter: var(--shelf-image-filter, none) !important; }
  `
  if (zoom !== 1) {
    css += `
    body { zoom: ${zoom} !important; }
    img, video, svg:not(svg svg) { zoom: ${+(1 / zoom).toFixed(4)} !important; }
  `
  }
  return css
}
