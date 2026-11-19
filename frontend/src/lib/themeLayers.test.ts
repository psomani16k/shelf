import { describe, expect, it } from "vitest"
import { isIdentity } from "./colorMatrix"
import { layerMatrix } from "./readerTheme"
import { LAYER_LABELS, type LayerKind, moveItem, newLayer } from "./themeLayers"

describe("newLayer", () => {
  it("starts every kind but invert as no change", () => {
    for (const kind of Object.keys(LAYER_LABELS) as LayerKind[]) {
      const m = layerMatrix(newLayer(kind))
      expect(m).not.toBeNull()
      if (m && kind !== "invert") expect(isIdentity(m), kind).toBe(true)
    }
    expect(newLayer("invert").skipImages).toBe(true)
  })
})

describe("moveItem", () => {
  it("moves within the ends", () => {
    expect(moveItem(["a", "b", "c"], 0, 1)).toEqual(["b", "a", "c"])
    expect(moveItem(["a", "b", "c"], 2, -1)).toEqual(["a", "c", "b"])
    expect(moveItem(["a", "b", "c"], 0, -1)).toEqual(["a", "b", "c"])
  })
})
