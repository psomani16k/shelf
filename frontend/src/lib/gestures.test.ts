import { describe, expect, it } from "vitest"
import { swipeDirection, wheelTurner } from "./gestures"

describe("swipeDirection", () => {
  it("reads a quick horizontal movement", () => {
    expect(swipeDirection({ dx: -80, dy: 10, ms: 200 })).toBe("left")
    expect(swipeDirection({ dx: 120, dy: -30, ms: 500 })).toBe("right")
  })

  it("ignores taps, scrolls and slow drags", () => {
    expect(swipeDirection({ dx: 5, dy: 2, ms: 100 })).toBeNull()
    expect(swipeDirection({ dx: 70, dy: 60, ms: 200 })).toBeNull()
    expect(swipeDirection({ dx: 200, dy: 0, ms: 2000 })).toBeNull()
  })
})

describe("wheelTurner", () => {
  it("fires once per touchpad swipe", () => {
    const turn = wheelTurner()
    expect(turn(30, 0)).toBe(0)
    expect(turn(30, 16)).toBe(0)
    expect(turn(30, 32)).toBe(1)
    expect(turn(30, 48)).toBe(0)
    expect(turn(30, 64)).toBe(0)
  })

  it("fires again after a pause, and backwards for a negative delta", () => {
    const turn = wheelTurner()
    expect(turn(100, 0)).toBe(1)
    expect(turn(-100, 100)).toBe(0)
    expect(turn(-100, 1000)).toBe(-1)
  })
})
