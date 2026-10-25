import { afterEach, describe, expect, it, vi } from "vitest"
import { ApiError, getCover, getProgress, getState, listBooks, setProgress, setState } from "./http"

function mockFetch(status: number, body?: unknown) {
  const fetch = vi.fn(async (_url: string, _init?: RequestInit) =>
    body === undefined
      ? new Response(null, { status })
      : new Response(JSON.stringify(body), {
          status,
          headers: { "content-type": "application/json" },
        }),
  )
  vi.stubGlobal("fetch", fetch)
  return fetch
}

function sentJson(fetch: ReturnType<typeof mockFetch>): unknown {
  return JSON.parse(fetch.mock.calls[0][1]?.body as string)
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe("http api", () => {
  it("sends snake_case keys and decodes snake_case responses", async () => {
    const fetch = mockFetch(200, {
      current_directory: "sci",
      books: [{ id: "abc", file_name: "dune.epub", title: "Dune", authors: ["Frank Herbert"] }],
    })

    const res = await listBooks("sci")

    expect(fetch.mock.calls[0][0]).toBe("/api/books/list")
    expect(sentJson(fetch)).toEqual({ directory_path: "sci" })
    expect(res.currentDirectory).toBe("sci")
    expect(res.books[0].fileName).toBe("dune.epub")
  })

  it("ignores fields it doesn't know", async () => {
    mockFetch(200, { id: "abc", epub_progress: "epubcfi(/6/4)", added_later: true })

    const res = await getProgress("abc")

    expect(res.epubProgress).toBe("epubcfi(/6/4)")
  })

  it("accepts an empty 204 from setProgress", async () => {
    const fetch = mockFetch(204)

    await setProgress("abc", "epubcfi(/6/4)")

    expect(sentJson(fetch)).toEqual({ id: "abc", epub_progress: "epubcfi(/6/4)" })
  })

  it("maps state in both directions", async () => {
    const fetch = mockFetch(200, { last_book_id: "abc" })

    const res = await getState()

    expect(fetch.mock.calls[0][0]).toBe("/api/state/get")
    expect(res.lastBookId).toBe("abc")
  })

  it("sends last_book_id when setting state", async () => {
    const fetch = mockFetch(204)

    await setState("abc")

    expect(fetch.mock.calls[0][0]).toBe("/api/state/set")
    expect(sentJson(fetch)).toEqual({ last_book_id: "abc" })
  })

  it("turns error responses into ApiError", async () => {
    mockFetch(404, { message: "not found" })

    const error = await getProgress("nope").catch((e: unknown) => e)

    expect(error).toBeInstanceOf(ApiError)
    expect(error).toMatchObject({ status: 404, message: "not found" })
  })

  it("escapes ids in URLs", async () => {
    const fetch = mockFetch(200)

    await getCover("a/b")

    expect(fetch.mock.calls[0][0]).toBe("/api/books/a%2Fb/cover")
  })
})
