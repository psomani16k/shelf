import { describe, expect, it } from "vitest"
import {
  applyMatrix,
  brightness,
  compose,
  contrast,
  fromValues,
  hex,
  hueRotate,
  invert,
  inverse,
  isIdentity,
  levels,
  type Rgb,
  saturate,
  svgValues,
} from "./colorMatrix"

const near = (a: Rgb, b: Rgb) => a.forEach((v, i) => expect(v).toBeCloseTo(b[i], 3))
const flip = () => compose(invert(), hueRotate(180))

describe("colorMatrix", () => {
  it("maps white and black to the dark preset's page and text", () => {
    const dark = compose(invert(), hueRotate(180), levels(hex("#161616"), hex("#d4d4d4")))
    near(applyMatrix(dark, [1, 1, 1]), hex("#161616"))
    near(applyMatrix(dark, [0, 0, 0]), hex("#d4d4d4"))
  })

  it("flip is its own inverse", () => {
    const twice = compose(flip(), flip())
    near(applyMatrix(twice, [0.2, 0.5, 0.9]), [0.2, 0.5, 0.9])
  })

  it("flip keeps a colour's dominant channel", () => {
    const [r, g, b] = applyMatrix(flip(), hex("#008000"))
    expect(g).toBeGreaterThan(r)
    expect(g).toBeGreaterThan(b)
  })

  it("levels tints per channel", () => {
    const sepia = levels(hex("#5b4636"), hex("#f4ecd8"))
    near(applyMatrix(sepia, [1, 1, 1]), hex("#f4ecd8"))
    near(applyMatrix(sepia, [0, 0, 0]), hex("#5b4636"))
  })

  it("writes 20 feColorMatrix values", () => {
    expect(svgValues(levels([0, 0, 0], [1, 1, 1])).split(/\s+/)).toHaveLength(20)
  })

  it("matches CSS saturate, brightness and contrast at their ends", () => {
    near(applyMatrix(saturate(0), [1, 0, 0]), [0.213, 0.213, 0.213])
    expect(isIdentity(saturate(1))).toBe(true)
    near(applyMatrix(brightness(0.5), [1, 1, 1]), [0.5, 0.5, 0.5])
    near(applyMatrix(contrast(0), [0.9, 0.1, 0.4]), [0.5, 0.5, 0.5])
  })

  it("reads 12 values as rows of r g b offset", () => {
    const m = fromValues([0, 0, 0, 0.25, 0, 1, 0, 0, 0, 0, 1, 0])
    near(applyMatrix(m, [1, 0.5, 0.5]), [0.25, 0.5, 0.5])
  })

  it("inverts what can be undone", () => {
    const dark = compose(invert(), hueRotate(180), levels(hex("#202020"), hex("#d4d4d4")))
    const undo = inverse(dark)
    if (!undo) throw new Error("no inverse")
    expect(isIdentity(compose(dark, undo))).toBe(true)
    expect(inverse(saturate(0))).toBeNull()
  })
})
