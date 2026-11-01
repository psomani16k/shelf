import { onBackButtonPress } from "@tauri-apps/api/app"
import type { PluginListener } from "@tauri-apps/api/core"
import { type RefObject, useEffect, useRef } from "react"
import { log } from "@/lib/log"
import { IS_ANDROID } from "@/lib/platform"

type Handler = RefObject<() => void>

/** Open layers, newest last. Back goes to the newest. */
const handlers: Handler[] = []
let listener: Promise<PluginListener | undefined> | undefined

/**
 * Makes `handler` the newest layer and returns its removal. Tauri's listener takes the button
 * over completely, so it's registered only while a layer is open: with none, Android's own back
 * behaviour applies.
 */
export function addBackHandler(handler: Handler): () => void {
  handlers.push(handler)
  listener ??= onBackButtonPress(() => handlers.at(-1)?.current()).catch((error: unknown) => {
    log.warn("back button listener failed", { error })
    return undefined
  })
  return () => {
    const i = handlers.indexOf(handler)
    if (i === -1) return
    handlers.splice(i, 1)
    if (handlers.length || !listener) return
    const old = listener
    listener = undefined
    old
      .then((l) => l?.unregister())
      .catch((error: unknown) => log.warn("back button listener removal failed", { error }))
  }
}

/** Android only: while `active`, the back button calls `onBack`, unless a newer layer opens. */
export function useBackButton(onBack: () => void, active = true) {
  const ref = useRef(onBack)
  useEffect(() => {
    ref.current = onBack
  })
  useEffect(() => {
    if (!IS_ANDROID || !active) return
    return addBackHandler(ref)
  }, [active])
}
