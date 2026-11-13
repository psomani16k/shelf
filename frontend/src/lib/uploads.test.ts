import { describe, expect, it } from "vitest"
import { createUploadQueue, type Uploader } from "./uploads"

/** An uploader whose calls finish only when the test says so. */
function controlledUploader() {
  const calls: { name: string; finish: (error?: Error) => void; progress: (n: number) => void }[] =
    []
  const upload: Uploader = (_folder, file, onProgress) =>
    new Promise((resolve, reject) => {
      calls.push({
        name: file.name,
        finish: (error) => (error ? reject(error) : resolve(undefined)),
        progress: onProgress,
      })
    })
  return { upload, calls }
}

const file = (name: string, size = 10) => new File([new Uint8Array(size)], name)
const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

describe("upload queue", () => {
  it("uploads one file at a time, in order", async () => {
    const { upload, calls } = controlledUploader()
    const queue = createUploadQueue(upload)
    let done = 0

    queue.enqueue("sci", [file("a.epub"), file("b.epub")], () => done++)
    await tick()
    expect(calls.map((c) => c.name)).toEqual(["a.epub"])
    calls[0].progress(4)
    expect(queue.getSnapshot().map((u) => [u.status, u.sent])).toEqual([
      ["uploading", 4],
      ["queued", 0],
    ])

    calls[0].finish()
    await tick()
    expect(calls.map((c) => c.name)).toEqual(["a.epub", "b.epub"])
    expect(queue.getSnapshot()[0]).toMatchObject({ status: "done", sent: 10, folder: "sci" })
    expect(done).toBe(1)
  })

  it("keeps going after a failure", async () => {
    const { upload, calls } = controlledUploader()
    const queue = createUploadQueue(upload)

    queue.enqueue("", [file("bad.epub"), file("good.epub")])
    await tick()
    calls[0].finish(new Error("not a readable EPUB"))
    await tick()
    calls[1].finish()
    await tick()

    expect(queue.getSnapshot().map((u) => [u.status, u.error])).toEqual([
      ["failed", "not a readable EPUB"],
      ["done", undefined],
    ])
    queue.clearFinished()
    expect(queue.getSnapshot()).toEqual([])
  })

  it("drops queued files but lets the current one finish", async () => {
    const { upload, calls } = controlledUploader()
    const queue = createUploadQueue(upload)

    queue.enqueue("", [file("a.epub"), file("b.epub"), file("c.epub")])
    await tick()
    queue.dropQueued()
    calls[0].finish()
    await tick()

    expect(calls.map((c) => c.name)).toEqual(["a.epub"])
    expect(queue.getSnapshot().map((u) => [u.name, u.status])).toEqual([["a.epub", "done"]])
  })
})
