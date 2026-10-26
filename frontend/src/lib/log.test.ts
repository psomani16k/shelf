import { describe, expect, it } from "vitest"
import { withFields } from "./log"

describe("withFields", () => {
  it("leaves a bare message alone", () => {
    expect(withFields("opened")).toBe("opened")
  })

  it("appends fields as key=value, skipping undefined ones", () => {
    expect(withFields("saved", { book_id: "abc", page: 3, stack: undefined })).toBe(
      'saved book_id="abc" page=3',
    )
  })

  it("shows errors by name and message", () => {
    expect(withFields("failed", { error: new TypeError("bad") })).toBe(
      'failed error="TypeError: bad"',
    )
  })
})
