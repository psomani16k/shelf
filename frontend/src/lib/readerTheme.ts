import type { ReaderThemeSpec, ThemeLayer } from "@/gen/configpb/theme_pb"
import type { Scheme } from "@/lib/appearance"
import {
  applyMatrix,
  brightness,
  type ColorMatrix,
  compose,
  contrast,
  fromValues,
  hex,
  hueRotate,
  invert,
  inverse,
  isIdentity,
  levels,
  multiply,
  type Rgb,
  sameMatrix,
  saturate,
  svgValues,
  toCss,
} from "@/lib/colorMatrix"

export type ReaderImages = "keep" | "page"

/** The backend's rule (`crates/core/src/themes/mod.rs`), for checking a name as it's typed. */
const THEME_NAME = /^[A-Za-z0-9 _-]{1,32}$/

/** The name's id (lowercase, spaces as `-`), or `null` if the backend would refuse the name. */
export function themeId(name: string): string | null {
  const trimmed = name.trim()
  return THEME_NAME.test(trimmed) ? trimmed.toLowerCase().replaceAll(" ", "-") : null
}

/** An SVG `feColorMatrix` filter; the id comes from the values, so a new look gets a new id. */
export type SvgFilter = { id: string; values: string }

export type ThemeLook = {
  /** What the book's white becomes: the reader behind the page, which is transparent. */
  page: string
  /** What the book's black becomes, for swatches. */
  text: string
  /** For controls over the page. */
  scheme: Scheme
  /** For each page's body. None leaves the book as published. */
  filter?: SvgFilter
  /** For images inside the body, before `filter`, so the two give the image layers only. */
  imageFilter?: SvgFilter
}

const factor = (v = 1) => (Number.isFinite(v) ? Math.max(0, v) : 1)

const color = (s: string, fallback: string) => {
  const value = s || fallback
  return /^#[0-9a-f]{6}$/i.test(value) ? hex(value) : null
}

/** `null` for a layer this build doesn't know, or one with bad values: it's skipped. */
export function layerMatrix({ kind }: ThemeLayer): ColorMatrix | null {
  switch (kind.case) {
    case "invert":
      return invert()
    case "hueRotate":
      return Number.isFinite(kind.value.degrees) ? hueRotate(kind.value.degrees) : null
    case "saturate":
      return saturate(factor(kind.value.amount))
    case "brightnessContrast":
      return compose(
        brightness(factor(kind.value.brightness)),
        contrast(factor(kind.value.contrast)),
      )
    case "levels": {
      const black = color(kind.value.black, "#000000")
      const white = color(kind.value.white, "#ffffff")
      return black && white ? levels(black, white) : null
    }
    case "multiply": {
      const rgb = color(kind.value.color, "")
      return rgb ? multiply(rgb) : null
    }
    case "matrix": {
      const v = kind.value.values
      return v.length === 12 && v.every(Number.isFinite) ? fromValues(v) : null
    }
    default:
      return null
  }
}

const WHITE: Rgb = [1, 1, 1]
const BLACK: Rgb = [0, 0, 0]
const luminance = ([r, g, b]: Rgb) => 0.2126 * r + 0.7152 * g + 0.0722 * b

/** The theme as filters. With `keep`, images get the layers not marked `skip_images` only. */
export function themeLook(spec: ReaderThemeSpec | undefined, images: ReaderImages): ThemeLook {
  const layers = (spec?.layers ?? []).flatMap((layer) => {
    const m = layerMatrix(layer)
    return m ? [{ m, skipImages: layer.skipImages }] : []
  })
  const full = compose(...layers.map((l) => l.m))
  const page = applyMatrix(full, WHITE)
  const look: ThemeLook = {
    page: toCss(page),
    text: toCss(applyMatrix(full, BLACK)),
    scheme: luminance(page) < 0.5 ? "dark" : "light",
  }
  if (isIdentity(full)) return look
  look.filter = svgFilter(full)

  if (images === "keep") {
    const forImages = compose(...layers.filter((l) => !l.skipImages).map((l) => l.m))
    const undo = inverse(full)
    // Without an inverse (e.g. a full desaturation) images get the whole theme.
    if (undo && !sameMatrix(forImages, full)) look.imageFilter = svgFilter(compose(forImages, undo))
  }
  return look
}

function svgFilter(m: ColorMatrix): SvgFilter {
  const values = svgValues(m)
  let hash = 5381
  for (let i = 0; i < values.length; i++) hash = Math.imul(hash, 33) ^ values.charCodeAt(i)
  return { id: `shelf-theme-${(hash >>> 0).toString(36)}`, values }
}

const SVG = "http://www.w3.org/2000/svg"

/**
 * Themes one book page. The filters go in the page's own document (`url(#…)` can't reach
 * another), and `readerCss` reads the variables, so a new look never lays the book out again.
 */
export function applyLook(doc: Document, look: ThemeLook) {
  for (const filter of [look.filter, look.imageFilter]) {
    if (filter && !doc.getElementById(filter.id)) addFilter(doc, filter)
  }
  const ref = (filter?: SvgFilter) => (filter ? `url(#${filter.id})` : "none")
  const { style } = doc.documentElement
  style.setProperty("--shelf-filter", ref(look.filter))
  style.setProperty("--shelf-image-filter", ref(look.imageFilter))
}

function addFilter(doc: Document, { id, values }: SvgFilter) {
  const svg = doc.getElementById(FILTERS_ID) ?? addFilterHolder(doc)
  const filter = doc.createElementNS(SVG, "filter")
  filter.setAttribute("id", id)
  filter.setAttribute("color-interpolation-filters", "sRGB")
  const fe = doc.createElementNS(SVG, "feColorMatrix")
  fe.setAttribute("type", "matrix")
  fe.setAttribute("values", values)
  filter.append(fe)
  svg.append(filter)
}

const FILTERS_ID = "shelf-theme-filters"

function addFilterHolder(doc: Document) {
  const svg = doc.createElementNS(SVG, "svg")
  svg.id = FILTERS_ID
  svg.setAttribute("aria-hidden", "true")
  svg.setAttribute("style", "position: absolute; width: 0; height: 0")
  ;(doc.body ?? doc.documentElement).append(svg)
  return svg
}
