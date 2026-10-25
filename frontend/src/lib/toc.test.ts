import type { TocItem } from "foliate-js/view.js"
import { describe, expect, it } from "vitest"
import { tocPath } from "./toc"

const item = (id: number, subitems?: TocItem[]): TocItem => ({
  id,
  label: `Item ${id}`,
  href: `ch${id}.xhtml`,
  subitems,
})

const toc = [item(0, [item(1), item(2, [item(3)])]), item(4)]

describe("tocPath", () => {
  it("lists every ancestor down to the item", () => {
    expect(tocPath(toc, 3)).toEqual([0, 2, 3])
    expect(tocPath(toc, 4)).toEqual([4])
  })

  it("is empty for an unknown or missing id", () => {
    expect(tocPath(toc, 99)).toEqual([])
    expect(tocPath(toc, undefined)).toEqual([])
  })
})
