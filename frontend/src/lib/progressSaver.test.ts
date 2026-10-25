import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { setProgress } from "@/api"
import { progressSaver } from "./progressSaver"

vi.mock("@/api", () => ({ setProgress: vi.fn(async () => {}) }))

beforeEach(() => vi.useFakeTimers())
afterEach(() => {
  vi.useRealTimers()
  vi.mocked(setProgress).mockClear()
})

describe("progressSaver", () => {
  it("saves the last position once page turns stop", () => {
    const saver = progressSaver("abc", undefined)
    saver.schedule("cfi-1")
    saver.schedule("cfi-2")
    expect(setProgress).not.toHaveBeenCalled()

    vi.advanceTimersByTime(2000)

    expect(setProgress).toHaveBeenCalledExactlyOnceWith("abc", "cfi-2", { keepalive: false })
  })

  it("doesn't re-save the position the book opened at", () => {
    const saver = progressSaver("abc", "cfi-1")
    saver.schedule("cfi-1")
    saver.flush()

    expect(setProgress).not.toHaveBeenCalled()
  })

  it("flushes right away with keepalive", () => {
    const saver = progressSaver("abc", undefined)
    saver.schedule("cfi-1")
    saver.flush()
    vi.advanceTimersByTime(2000)

    expect(setProgress).toHaveBeenCalledExactlyOnceWith("abc", "cfi-1", { keepalive: true })
  })
})
