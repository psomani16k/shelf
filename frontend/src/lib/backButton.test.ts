import { beforeEach, describe, expect, it, vi } from "vitest"
import { addBackHandler } from "./backButton"

const tauri = vi.hoisted(() => ({
  press: undefined as (() => void) | undefined,
  unregister: vi.fn(),
  onBackButtonPress: vi.fn(),
}))

vi.mock("@tauri-apps/api/app", () => ({ onBackButtonPress: tauri.onBackButtonPress }))

beforeEach(() => {
  tauri.unregister.mockReset()
  tauri.onBackButtonPress.mockReset().mockImplementation((cb: () => void) => {
    tauri.press = cb
    return Promise.resolve({ unregister: tauri.unregister })
  })
})

const handler = () => ({ current: vi.fn() })

describe("addBackHandler", () => {
  it("sends back to the newest open layer", () => {
    const drawer = handler()
    const settings = handler()
    const removeDrawer = addBackHandler(drawer)
    const removeSettings = addBackHandler(settings)
    expect(tauri.onBackButtonPress).toHaveBeenCalledTimes(1)

    tauri.press!()
    expect(settings.current).toHaveBeenCalledTimes(1)
    expect(drawer.current).not.toHaveBeenCalled()

    removeSettings()
    tauri.press!()
    expect(drawer.current).toHaveBeenCalledTimes(1)
    removeDrawer()
  })

  it("lets go of the button once nothing is open, and takes it again later", async () => {
    const remove = addBackHandler(handler())
    remove()
    await vi.waitFor(() => expect(tauri.unregister).toHaveBeenCalledTimes(1))

    const removeAgain = addBackHandler(handler())
    expect(tauri.onBackButtonPress).toHaveBeenCalledTimes(2)
    removeAgain()
  })

  it("ignores a second removal", async () => {
    const first = handler()
    const removeFirst = addBackHandler(first)
    const removeSecond = addBackHandler(handler())
    removeSecond()
    removeSecond()
    tauri.press!()
    expect(first.current).toHaveBeenCalledTimes(1)
    removeFirst()
    await vi.waitFor(() => expect(tauri.unregister).toHaveBeenCalledTimes(1))
  })
})
