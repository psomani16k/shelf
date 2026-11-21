import { useEffect, useState, useSyncExternalStore } from "react"
import { setKeepScreenOn } from "@/api/app"
import { log } from "@/lib/log"
import { IS_TAURI } from "@/lib/platform"

// Per device, like the interface scale: a phone on a stand and a laptop want different things.
const KEY = "shelf.keep-screen-on"

/** A book left open on the table still sleeps, this long after the last page turn or tap. */
export const IDLE_MS = 10 * 60_000

const listeners = new Set<() => void>()

function load(): boolean {
  try {
    return localStorage.getItem(KEY) !== "off"
  } catch {
    return true
  }
}

let enabled = load()

/** On unless turned off in Settings. */
export function useKeepScreenOnSetting(): [boolean, (on: boolean) => void] {
  const value = useSyncExternalStore(
    (listener) => {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    () => enabled,
  )
  return [value, saveSetting]
}

function saveSetting(on: boolean) {
  enabled = on
  try {
    localStorage.setItem(KEY, on ? "on" : "off")
  } catch {
    // Storage can be blocked; the choice then lasts until the app restarts.
  }
  for (const listener of listeners) listener()
}

/**
 * Keeps the screen on while `active` (the book has the screen), until `IDLE_MS` pass without a
 * call to the returned function, which the reader makes on every page turn, tap and key.
 */
export function useKeepScreenOn(active: boolean): () => void {
  const [setting] = useKeepScreenOnSetting()
  const on = IS_TAURI && setting && active
  const [keeper] = useState(screenKeeper)

  useEffect(() => {
    if (!on) return
    keeper.start()
    document.addEventListener("visibilitychange", keeper.onVisibility)
    return () => {
      document.removeEventListener("visibilitychange", keeper.onVisibility)
      keeper.stop()
    }
  }, [on, keeper])

  return keeper.poke
}

function screenKeeper() {
  let allowed = false
  let sent = false
  let idle: ReturnType<typeof setTimeout> | undefined

  const send = (value: boolean) => {
    if (sent === value) return
    sent = value
    setKeepScreenOn(value).catch((e: unknown) => {
      log.warn("couldn't change keeping the screen on", { on: value, error: String(e) })
    })
  }
  const rest = () => {
    clearTimeout(idle)
    send(false)
  }
  const poke = () => {
    if (!allowed || document.hidden) return
    send(true)
    clearTimeout(idle)
    idle = setTimeout(() => send(false), IDLE_MS)
  }
  return {
    poke,
    start: () => {
      allowed = true
      poke()
    },
    stop: () => {
      allowed = false
      rest()
    },
    onVisibility: () => (document.hidden ? rest() : poke()),
  }
}
