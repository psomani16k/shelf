import { useSyncExternalStore } from "react"
import { uploadBook } from "@/api"

export type UploadStatus = "queued" | "uploading" | "done" | "failed"

export type Upload = {
  id: number
  name: string
  /** The folder it goes into, relative to the library root. */
  folder: string
  size: number
  sent: number
  status: UploadStatus
  error?: string
}

export type Uploader = (
  folder: string,
  file: File,
  onProgress: (sentBytes: number) => void,
) => Promise<unknown>

/**
 * Uploads files one at a time, in the order they were added. It lives outside React, so uploads
 * carry on while the library drawer is closed.
 */
export function createUploadQueue(upload: Uploader) {
  let items: Upload[] = []
  const pending = new Map<number, { file: File; onDone?: () => void }>()
  const listeners = new Set<() => void>()
  let nextId = 1
  let running = false

  const set = (next: Upload[]) => {
    items = next
    for (const listener of listeners) listener()
  }
  const update = (id: number, patch: Partial<Upload>) =>
    set(items.map((u) => (u.id === id ? { ...u, ...patch } : u)))

  async function run() {
    if (running) return
    running = true
    try {
      for (let next = firstQueued(); next; next = firstQueued()) {
        const { id, folder, size } = next
        const job = pending.get(id)!
        pending.delete(id)
        update(id, { status: "uploading" })
        try {
          await upload(folder, job.file, (sent) => update(id, { sent }))
          update(id, { status: "done", sent: size })
          job.onDone?.()
        } catch (e) {
          update(id, { status: "failed", error: e instanceof Error ? e.message : String(e) })
        }
      }
    } finally {
      running = false
    }
  }
  const firstQueued = () => items.find((u) => u.status === "queued")

  return {
    subscribe(listener: () => void) {
      listeners.add(listener)
      return () => void listeners.delete(listener)
    },
    getSnapshot: () => items,

    /** `onDone` runs after each file that uploads. */
    enqueue(folder: string, files: File[], onDone?: () => void) {
      const added = files.map((file): Upload => {
        const id = nextId++
        pending.set(id, { file, onDone })
        return { id, name: file.name, folder, size: file.size, sent: 0, status: "queued" }
      })
      set([...items, ...added])
      void run()
    },

    /** Forgets finished and failed uploads. */
    clearFinished() {
      set(items.filter((u) => u.status === "queued" || u.status === "uploading"))
    },

    /** Drops files not started yet; the one uploading finishes. */
    dropQueued() {
      for (const u of items) if (u.status === "queued") pending.delete(u.id)
      set(items.filter((u) => u.status !== "queued"))
    },
  }
}

export const uploads = createUploadQueue(uploadBook)

export function useUploads(): Upload[] {
  return useSyncExternalStore(uploads.subscribe, uploads.getSnapshot)
}
