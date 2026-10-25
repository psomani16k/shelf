import { create } from "@bufbuild/protobuf"
import { describe, expect, it } from "vitest"
import {
  ConfigSchema,
  ReaderFlow_Value,
  ReaderMargin_Value,
  ReaderTheme_Value,
  UiTheme_Value,
} from "@/gen/configpb/config_pb"
import {
  type Margins,
  readerCss,
  readerLayout,
  resolveAppearance,
  SCROLLBAR_ALLOWANCE,
} from "./appearance"

describe("resolveAppearance", () => {
  it("defaults to the system theme and scrolling", () => {
    expect(resolveAppearance(undefined, "dark")).toMatchObject({
      ui: "dark",
      reader: { flow: "scrolled", scheme: "dark" },
    })
  })

  it("lets the reader follow the UI or pick its own theme", () => {
    const config = (reader: ReaderTheme_Value) =>
      create(ConfigSchema, { ui: { theme: UiTheme_Value.LIGHT }, reader: { theme: reader } })

    expect(resolveAppearance(config(ReaderTheme_Value.MATCH_UI), "dark").reader.scheme).toBe("light")
    expect(resolveAppearance(config(ReaderTheme_Value.DARK), "dark").reader.scheme).toBe("dark")
  })

  it("reads the flow", () => {
    const config = create(ConfigSchema, { reader: { flow: ReaderFlow_Value.PAGINATED } })
    expect(resolveAppearance(config, "light").reader.flow).toBe("paginated")
  })
})

describe("readerLayout", () => {
  const margins = (overrides: Partial<Margins> = {}): Margins => ({
    top: "medium",
    bottom: "medium",
    side: "medium",
    betweenPages: "medium",
    ...overrides,
  })

  it("defaults margins to medium", () => {
    expect(resolveAppearance(undefined, "light").reader.margins).toEqual(margins())
    const config = create(ConfigSchema, {
      reader: { margins: { side: ReaderMargin_Value.LARGE, top: ReaderMargin_Value.NONE } },
    })
    expect(resolveAppearance(config, "light").reader.margins).toEqual(
      margins({ side: "large", top: "none" }),
    )
  })

  it("splits the side margin from the space between pages", () => {
    // Landscape desktop: side medium = 48px (capped), between large = 7% of 1400 = 98px.
    const layout = readerLayout(
      { width: 1400, height: 800 },
      { flow: "paginated", margins: margins({ betweenPages: "large" }) },
    )
    const viewWidth = 1400 - layout.marginLeft - layout.marginRight
    expect(layout.marginLeft).toBe(48 - 98)
    expect(layout.marginRight).toBe(48 - 98)
    // foliate puts `gap × view width` both at the sides and between pages.
    expect((layout.gapPercent / 100) * viewWidth).toBeCloseTo(98)
    expect((layout.gapPercent / 100) * viewWidth + layout.marginLeft).toBeCloseTo(48)
  })

  it("scales presets with the screen and keeps them in range", () => {
    const phone = readerLayout(
      { width: 400, height: 800 },
      { flow: "paginated", margins: margins({ bottom: "none" }) },
    )
    // Top medium: 5% of 800 = 40px. Side medium: 4% of 400 = 16px.
    expect(phone.paddingTop).toBe(40)
    expect(phone.paddingBottom).toBe(0)
    const viewWidth = 400 - phone.marginLeft - phone.marginRight
    expect(phone.marginLeft + (phone.gapPercent / 100) * viewWidth).toBeCloseTo(16)
  })

  it("ignores top, bottom and between pages when scrolled", () => {
    const layout = readerLayout({ width: 400, height: 800 }, { flow: "scrolled", margins: margins() })
    expect(layout).toEqual({
      paddingTop: 0,
      paddingBottom: 0,
      marginLeft: 16,
      marginRight: 16,
      gapPercent: 0,
      pageMarginRight: 0,
    })
  })

  it("keeps an inset clear on both sides, on top of the margin", () => {
    const size = { width: 1400, height: 800 }
    const paginated = readerLayout(size, { flow: "paginated", margins: margins() }, { inset: 72 })
    const viewWidth = 1400 - paginated.marginLeft - paginated.marginRight
    // Text starts at inset + side: 72 + 48.
    expect(paginated.marginLeft + (paginated.gapPercent / 100) * viewWidth).toBeCloseTo(120)
    const scrolled = readerLayout(
      size,
      { flow: "scrolled", margins: margins() },
      { inset: 72, clipScrollbar: true },
    )
    expect(scrolled.marginLeft).toBe(120)
    expect(scrolled.pageMarginRight).toBe(120 + SCROLLBAR_ALLOWANCE)
  })

  it("pushes the scrolled view's right edge out to hide the scrollbar", () => {
    const size = { width: 400, height: 800 }
    const reader = { flow: "scrolled" as const, margins: margins() }
    const layout = readerLayout(size, reader, { clipScrollbar: true })
    expect(layout.marginLeft).toBe(16)
    // The view's edge, and its scrollbar, end up past the wrapper; the text still ends at 16px.
    expect(layout.marginRight).toBe(-SCROLLBAR_ALLOWANCE)
    expect(layout.pageMarginRight).toBe(16 + SCROLLBAR_ALLOWANCE)
    expect(readerCss("light", { pageMarginRight: 40 })).toContain("margin-right: 40px")
  })
})
