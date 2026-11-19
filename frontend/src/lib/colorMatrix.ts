/** sRGB channels in 0–1. */
export type Rgb = [number, number, number]

/** An affine colour transform, `out = m·in + t`, like SVG's `feColorMatrix` without alpha. */
export type ColorMatrix = { m: number[][]; t: number[] }

const IDENTITY: ColorMatrix = {
  m: [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
  ],
  t: [0, 0, 0],
}

export const invert = (): ColorMatrix => ({
  m: [
    [-1, 0, 0],
    [0, -1, 0],
    [0, 0, -1],
  ],
  t: [1, 1, 1],
})

/** The same matrix as CSS `hue-rotate()` (Filter Effects spec). */
export function hueRotate(deg: number): ColorMatrix {
  const rad = (deg * Math.PI) / 180
  const c = Math.cos(rad)
  const s = Math.sin(rad)
  return {
    m: [
      [0.213 + c * 0.787 - s * 0.213, 0.715 - c * 0.715 - s * 0.715, 0.072 - c * 0.072 + s * 0.928],
      [0.213 - c * 0.213 + s * 0.143, 0.715 + c * 0.285 + s * 0.14, 0.072 - c * 0.072 - s * 0.283],
      [0.213 - c * 0.213 - s * 0.787, 0.715 - c * 0.715 + s * 0.715, 0.072 + c * 0.928 + s * 0.072],
    ],
    t: [0, 0, 0],
  }
}

/** The same matrix as CSS `saturate()`: 0 is grey, 1 unchanged. */
export const saturate = (s: number): ColorMatrix => ({
  m: [
    [0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s],
    [0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s],
    [0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s],
  ],
  t: [0, 0, 0],
})

/** Scales each channel, like a Multiply layer of `rgb`. */
export const multiply = (rgb: Rgb): ColorMatrix => ({
  m: [
    [rgb[0], 0, 0],
    [0, rgb[1], 0],
    [0, 0, rgb[2]],
  ],
  t: [0, 0, 0],
})

/** CSS `brightness()`. */
export const brightness = (b: number): ColorMatrix => multiply([b, b, b])

/** CSS `contrast()`: scales around mid-grey. */
export const contrast = (c: number): ColorMatrix => ({
  ...multiply([c, c, c]),
  t: [0.5 - 0.5 * c, 0.5 - 0.5 * c, 0.5 - 0.5 * c],
})

/** 12 values: the rows for red, green and blue, each `r g b offset`. */
export const fromValues = (v: number[]): ColorMatrix => ({
  m: [0, 1, 2].map((i) => v.slice(i * 4, i * 4 + 3)),
  t: [v[3], v[7], v[11]],
})

/** Photoshop's Levels output sliders, per channel: 0 becomes `black` and 1 becomes `white`. */
export const levels = (black: Rgb, white: Rgb): ColorMatrix => ({
  m: [
    [white[0] - black[0], 0, 0],
    [0, white[1] - black[1], 0],
    [0, 0, white[2] - black[2]],
  ],
  t: [...black],
})

/** The layers applied in order (the first is the bottom one), as one matrix. */
export function compose(...layers: ColorMatrix[]): ColorMatrix {
  return layers.reduce(
    (a, b) => ({
      m: b.m.map((row) =>
        [0, 1, 2].map((j) => row[0] * a.m[0][j] + row[1] * a.m[1][j] + row[2] * a.m[2][j]),
      ),
      t: b.m.map((row, i) => row[0] * a.t[0] + row[1] * a.t[1] + row[2] * a.t[2] + b.t[i]),
    }),
    IDENTITY,
  )
}

/** Undoes `a`, or `null` when it loses information (e.g. full desaturation). */
export function inverse({ m, t }: ColorMatrix): ColorMatrix | null {
  const [[a, b, c], [d, e, f], [g, h, i]] = m
  const det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g)
  if (Math.abs(det) < 1e-9) return null
  const inv = [
    [e * i - f * h, c * h - b * i, b * f - c * e],
    [f * g - d * i, a * i - c * g, c * d - a * f],
    [d * h - e * g, b * g - a * h, a * e - b * d],
  ].map((row) => row.map((v) => v / det))
  return { m: inv, t: inv.map((row) => -(row[0] * t[0] + row[1] * t[1] + row[2] * t[2])) }
}

export function sameMatrix(a: ColorMatrix, b: ColorMatrix): boolean {
  const close = (x: number, y: number) => Math.abs(x - y) < 1e-6
  return (
    a.m.every((row, i) => row.every((v, j) => close(v, b.m[i][j]))) &&
    a.t.every((v, i) => close(v, b.t[i]))
  )
}

export const isIdentity = (a: ColorMatrix) => sameMatrix(a, IDENTITY)

const clamp = (v: number) => Math.min(1, Math.max(0, v))

export const applyMatrix = ({ m, t }: ColorMatrix, rgb: Rgb): Rgb =>
  [0, 1, 2].map((i) => clamp(m[i][0] * rgb[0] + m[i][1] * rgb[1] + m[i][2] * rgb[2] + t[i])) as Rgb

/** `"#rrggbb"` to channels. */
export const hex = (s: string): Rgb =>
  [1, 3, 5].map((i) => Number.parseInt(s.slice(i, i + 2), 16) / 255) as Rgb

export const toCss = (rgb: Rgb) => `rgb(${rgb.map((v) => Math.round(v * 255)).join(" ")})`

/** `feColorMatrix`'s `values`: 4 rows of 5, alpha passed through. */
export function svgValues({ m, t }: ColorMatrix): string {
  const rows = [0, 1, 2].map((i) => [...m[i], 0, t[i]])
  rows.push([0, 0, 0, 1, 0])
  return rows.map((row) => row.map((v) => +v.toFixed(4)).join(" ")).join("  ")
}
