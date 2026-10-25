import {
  type Config,
  ReaderFlow_Value,
  ReaderMargin_Value,
  ReaderTheme_Value,
  UiTheme_Value,
} from "@/gen/configpb/config_pb"

export type Scheme = "light" | "dark"
export type Flow = "scrolled" | "paginated"

export type MarginSize = "none" | "small" | "medium" | "large"
export type Margins = {
  top: MarginSize
  bottom: MarginSize
  side: MarginSize
  betweenPages: MarginSize
}

export type Appearance = {
  ui: Scheme
  reader: { flow: Flow; scheme: Scheme; margins: Margins }
}

const MARGIN_SIZES: Record<ReaderMargin_Value, MarginSize> = {
  [ReaderMargin_Value.NONE]: "none",
  [ReaderMargin_Value.SMALL]: "small",
  [ReaderMargin_Value.MEDIUM]: "medium",
  [ReaderMargin_Value.LARGE]: "large",
}
const marginSize = (value: ReaderMargin_Value | undefined) =>
  MARGIN_SIZES[value ?? ReaderMargin_Value.MEDIUM]

/** Unset fields mean the proto zero value: system theme, scrolled, reader matches the UI, medium margins. */
export function resolveAppearance(config: Config | undefined, system: Scheme): Appearance {
  const uiTheme = config?.ui?.theme
  const ui =
    uiTheme === UiTheme_Value.LIGHT ? "light" : uiTheme === UiTheme_Value.DARK ? "dark" : system

  const readerTheme = config?.reader?.theme
  const scheme =
    readerTheme === ReaderTheme_Value.LIGHT
      ? "light"
      : readerTheme === ReaderTheme_Value.DARK
        ? "dark"
        : ui
  const flow = config?.reader?.flow === ReaderFlow_Value.PAGINATED ? "paginated" : "scrolled"

  const m = config?.reader?.margins
  const margins = {
    top: marginSize(m?.top),
    bottom: marginSize(m?.bottom),
    side: marginSize(m?.side),
    betweenPages: marginSize(m?.betweenPages),
  }

  return { ui, reader: { flow, scheme, margins } }
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

// The same colours as --background and --foreground in index.css.
const PALETTE = {
  light: { background: "oklch(1 0 0)", text: "oklch(0.145 0 0)" },
  dark: { background: "oklch(0.145 0 0)", text: "oklch(0.985 0 0)" },
}

export const readerBackground = (scheme: Scheme) => PALETTE[scheme].background

/** Styles for each book page. Dark mode overrides the book's own colours so no text is lost. */
export function readerCss(scheme: Scheme, { pageMarginRight = 0 } = {}): string {
  const { background, text } = PALETTE[scheme]
  const base = `
    html { color-scheme: ${scheme}; color: ${text}; background: ${background} !important; }
    html { margin-right: ${pageMarginRight}px !important; }
    body { background: transparent !important; color: inherit !important; }
  `
  if (scheme === "light") return base
  return `${base}
    body *:not(a, a *) { color: inherit !important; background-color: transparent !important; }
    a:link, a:visited, a * { color: oklch(0.8 0.1 250) !important; }
  `
}
