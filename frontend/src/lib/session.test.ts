import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { endSession, getToken, isSignedOut, setSignedOut, setToken } from "./session"

function memoryStorage(): Storage {
  const items = new Map<string, string>()
  return {
    get length() {
      return items.size
    },
    clear: () => items.clear(),
    getItem: (key) => items.get(key) ?? null,
    key: (i) => [...items.keys()][i] ?? null,
    removeItem: (key) => void items.delete(key),
    setItem: (key, value) => void items.set(key, value),
  }
}

beforeEach(() => {
  vi.stubGlobal("localStorage", memoryStorage())
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe("session", () => {
  it("keeps the key until it's ended", () => {
    setToken("abc")
    setSignedOut(false)
    expect(getToken()).toBe("abc")
    expect(isSignedOut()).toBe(false)

    endSession()

    expect(getToken()).toBeNull()
    expect(isSignedOut()).toBe(true)
  })

  it("works without storage", () => {
    vi.stubGlobal("localStorage", undefined)
    expect(() => setToken("abc")).not.toThrow()
    expect(getToken()).toBeNull()
  })
})
