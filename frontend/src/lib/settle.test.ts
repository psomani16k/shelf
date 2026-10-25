import { describe, expect, it } from "vitest"
import { settle } from "./settle"

describe("settle", () => {
  it("follows a fast flick whatever the position", () => {
    expect(settle(0.2, 1)).toBe(true)
    expect(settle(0.9, -1)).toBe(false)
  })

  it("otherwise picks the nearer side", () => {
    expect(settle(0.6, 0.1)).toBe(true)
    expect(settle(0.4, -0.1)).toBe(false)
  })
})
