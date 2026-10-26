import { setProgress } from "@/api"
import { log } from "@/lib/log"

const SAVE_DELAY_MS = 2000

export type ProgressSaver = {
  /** Saves `cfi` once page turns have stopped for a moment. */
  schedule: (cfi: string) => void
  /** Saves any pending position now, in a request that outlives the page. */
  flush: () => void
}

/** `saved` is the position already on the server, so reopening a book doesn't re-save it. */
export function progressSaver(id: string, saved: string | undefined): ProgressSaver {
  let pending: string | undefined
  let timer: ReturnType<typeof setTimeout> | undefined

  const save = (keepalive: boolean) => {
    clearTimeout(timer)
    timer = undefined
    if (pending === undefined || pending === saved) return
    const cfi = pending
    pending = undefined
    saved = cfi
    setProgress(id, cfi, { keepalive }).catch((e: unknown) => {
      log.error("couldn't save reading progress", { book_id: id, error: e })
    })
  }

  return {
    schedule(cfi) {
      pending = cfi
      clearTimeout(timer)
      timer = setTimeout(() => save(false), SAVE_DELAY_MS)
    },
    flush: () => save(true),
  }
}
