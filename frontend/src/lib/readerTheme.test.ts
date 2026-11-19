import { create } from "@bufbuild/protobuf"
import { describe, expect, it } from "vitest"
import { ReaderThemeSpecSchema } from "@/gen/configpb/theme_pb"
import { themeId, themeLook } from "./readerTheme"

const dark = create(ReaderThemeSpecSchema, {
  name: "Dark",
  layers: [
    { skipImages: true, kind: { case: "invert", value: {} } },
    { skipImages: true, kind: { case: "hueRotate", value: { degrees: 180 } } },
    { kind: { case: "levels", value: { black: "#202020", white: "#d4d4d4" } } },
  ],
})

describe("themeLook", () => {
  it("leaves the book as published with no layers", () => {
    const look = themeLook(create(ReaderThemeSpecSchema, { name: "Light" }), "keep")
    expect(look).toEqual({ page: "rgb(255 255 255)", text: "rgb(0 0 0)", scheme: "light" })
    expect(themeLook(undefined, "keep").filter).toBeUndefined()
  })

  it("gives the page colour and a filter, with images undone when they keep colours", () => {
    const keep = themeLook(dark, "keep")
    expect(keep.page).toBe("rgb(32 32 32)")
    expect(keep.scheme).toBe("dark")
    expect(keep.filter?.id).toMatch(/^shelf-theme-/)
    expect(keep.imageFilter).toBeDefined()
    expect(themeLook(dark, "page").imageFilter).toBeUndefined()
  })

  it("gives the same id to the same look and skips layers it can't use", () => {
    const broken = create(ReaderThemeSpecSchema, {
      layers: [...dark.layers, { kind: { case: "multiply", value: { color: "red" } } }, {}],
    })
    expect(themeLook(broken, "keep").filter).toEqual(themeLook(dark, "keep").filter)
  })
})

describe("themeId", () => {
  it("follows the backend's rule", () => {
    expect(themeId(" My Night ")).toBe("my-night")
    expect(themeId("AMOLED")).toBe("amoled")
    for (const bad of ["", "  ", "a/b", "é", "a.b", "a".repeat(33)]) expect(themeId(bad)).toBeNull()
  })
})
