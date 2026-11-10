import { describe, expect, it } from "vitest"
import { readerCss } from "./appearance"
import { snapZoom, stepZoom } from "./readerZoom"

describe("stepZoom", () => {
  it("moves 10% at a time and stops at the ends", () => {
    expect(stepZoom(1, 1)).toBe(1.1)
    expect(stepZoom(1, -1)).toBe(0.9)
    expect(stepZoom(0.7, 1)).toBe(0.8)
    expect(stepZoom(2, 1)).toBe(2)
    expect(stepZoom(0.5, -1)).toBe(0.5)
  })

  it("restarts from 100% when the level isn't a step", () => {
    expect(stepZoom(1.25, 1)).toBe(1)
  })
})

describe("snapZoom", () => {
  it("moves levels from older steps to the nearest one", () => {
    expect(snapZoom(1.1)).toBe(1.1)
    expect(snapZoom(0.67)).toBe(0.7)
    expect(snapZoom(1.25)).toBe(1.3)
    expect(snapZoom(3)).toBe(2)
    expect(snapZoom(0.2)).toBe(0.5)
  })

  it("falls back to 100% for nothing stored or garbage", () => {
    expect(snapZoom(Number(null))).toBe(1)
    expect(snapZoom(Number("abc"))).toBe(1)
  })
})

describe("readerCss zoom", () => {
  it("zooms the body and undoes it on images", () => {
    const css = readerCss("light", { zoom: 1.25 })
    expect(css).toContain("body { zoom: 1.25 !important; }")
    expect(css).toContain("svg:not(svg svg) { zoom: 0.8 !important; }")
    expect(readerCss("light")).not.toContain("zoom")
  })
})
