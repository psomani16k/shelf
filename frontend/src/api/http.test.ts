import { afterEach, describe, expect, it, vi } from "vitest"
import { getToken, isSignedOut, setSignedOut, setToken } from "@/lib/session"
import { Role_Value } from "@/gen/apipb/auth_pb"
import {
  ApiError,
  approveUserRequest,
  createDirectory,
  deleteItems,
  getCover,
  getProgress,
  getState,
  listBooks,
  listUserRequests,
  listUsers,
  login,
  logout,
  rebuildIndex,
  cleanUpProgress,
  setProgress,
  setState,
  setUserRole,
  setUserScope,
  signUp,
} from "./http"

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

  it("rebuilds the index with an empty request", async () => {
    const fetch = mockFetch(200, { books: 12, removed: 1 })

    const res = await rebuildIndex()

    expect(fetch.mock.calls[0][0]).toBe("/api/library/rebuild")
    expect(sentJson(fetch)).toEqual({})
    expect(res).toMatchObject({ books: 12, failed: 0, removed: 1 })
  })

  it("cleans up progress with an empty request", async () => {
    const fetch = mockFetch(200, { books: 3 })

    const res = await cleanUpProgress()

    expect(fetch.mock.calls[0][0]).toBe("/api/library/progress/clean")
    expect(sentJson(fetch)).toEqual({})
    expect(res.books).toBe(3)
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

  it("sends folder and delete requests as snake_case", async () => {
    const fetch = mockFetch(200, {})
    await createDirectory("sci", "Old")
    expect(fetch.mock.calls[0][0]).toBe("/api/directories/create")
    expect(sentJson(fetch)).toEqual({ parent_directory: "sci", directory_name: "Old" })

    const del = mockFetch(200, { books: 1, directories: 2 })
    const res = await deleteItems({
      books: [{ id: "abc", directoryPath: "sci", fileName: "Dune.epub" }],
      directoryPaths: ["sci/old", "x"],
    })
    expect(del.mock.calls[0][0]).toBe("/api/library/delete")
    expect(sentJson(del)).toEqual({
      books: [{ id: "abc", directory_path: "sci", file_name: "Dune.epub" }],
      directory_paths: ["sci/old", "x"],
    })
    expect([res.books, res.directories]).toEqual([1, 2])
  })

  it("shows the server's message for a conflict", async () => {
    mockFetch(409, { message: "Old already exists" })
    await expect(createDirectory("", "Old")).rejects.toEqual(new ApiError(409, "Old already exists"))
  })

  describe("auth", () => {
    const storage = () => {
      const items = new Map<string, string>()
      vi.stubGlobal("localStorage", {
        getItem: (key: string) => items.get(key) ?? null,
        setItem: (key: string, value: string) => void items.set(key, value),
        removeItem: (key: string) => void items.delete(key),
      })
    }
    const sentAuthorization = (fetch: ReturnType<typeof mockFetch>) =>
      new Headers(fetch.mock.calls[0][1]?.headers).get("authorization")

    it("login keeps the key and later requests send it", async () => {
      storage()
      setSignedOut(true)
      mockFetch(200, { token: "key-1", user: { username: "kid" } })

      const user = await login("kid", "pancakes")

      expect(user.username).toBe("kid")
      expect(getToken()).toBe("key-1")
      expect(isSignedOut()).toBe(false)
      const fetch = mockFetch(200, { current_directory: "", books: [] })
      await listBooks("")
      expect(sentAuthorization(fetch)).toBe("Bearer key-1")
    })

    it("a 401 forgets the key and signs out", async () => {
      storage()
      setToken("old")
      setSignedOut(false)
      mockFetch(401, { message: "this session has ended; sign in again" })

      await expect(getState()).rejects.toMatchObject({ status: 401 })

      expect(getToken()).toBeNull()
      expect(isSignedOut()).toBe(true)
    })

    it("a wrong password doesn't count as being signed out of a session", async () => {
      storage()
      setSignedOut(true)
      mockFetch(401, { message: "wrong username or password" })

      await expect(login("kid", "nope")).rejects.toMatchObject({
        status: 401,
        message: "wrong username or password",
      })
    })

    it("logout forgets the key even when the server is unreachable", async () => {
      storage()
      setToken("key-1")
      setSignedOut(false)
      vi.stubGlobal("fetch", vi.fn(async () => Promise.reject(new TypeError("offline"))))

      await expect(logout()).rejects.toThrow()

      expect(getToken()).toBeNull()
      expect(isSignedOut()).toBe(true)
    })

    it("signs up and shows why a name is refused", async () => {
      storage()
      const fetch = mockFetch(204)
      await signUp("newbie", "secret")
      expect(fetch.mock.calls[0][0]).toBe("/api/auth/signup")
      expect(sentJson(fetch)).toEqual({ username: "newbie", password: "secret" })

      mockFetch(409, { message: "this username is already taken" })
      await expect(signUp("parth", "x")).rejects.toMatchObject({
        status: 409,
        message: "this username is already taken",
      })
    })

    it("lists user requests and approves one with a role", async () => {
      mockFetch(200, {
        requests: [{ id: "r1", username: "newbie", created: "2026-11-13T10:00:00Z", taken: true }],
      })
      const [request] = await listUserRequests()
      expect(request).toMatchObject({ id: "r1", username: "newbie", taken: true })

      const fetch = mockFetch(204)
      await approveUserRequest("r1", Role_Value.LIBRARIAN)
      expect(fetch.mock.calls[0][0]).toBe("/api/users/requests/approve")
      expect(sentJson(fetch)).toEqual({ id: "r1", role: "LIBRARIAN" })
    })

    it("lists users, with no role meaning banned, and bans one", async () => {
      mockFetch(200, {
        users: [{ username: "kid" }, { username: "parth", role: "OWNER" }],
        scopes: { kid: { library_root: "kid", books: ["!scary/**"] }, parth: {} },
      })
      const { users, scopes } = await listUsers()
      expect(users.map((u) => [u.username, u.role])).toEqual([
        ["kid", Role_Value.BANNED],
        ["parth", Role_Value.OWNER],
      ])
      expect([scopes.kid.libraryRoot, scopes.kid.books]).toEqual(["kid", ["!scary/**"]])

      const fetch = mockFetch(204)
      await setUserRole("kid", Role_Value.BANNED)
      expect(fetch.mock.calls[0][0]).toBe("/api/users/role/set")
      expect(sentJson(fetch)).toMatchObject({ username: "kid" })
    })

    it("sends a user's scope as snake_case", async () => {
      const fetch = mockFetch(204)
      await setUserScope("kid", { libraryRoot: "kid", books: ["**/*.epub"] })
      expect(fetch.mock.calls[0][0]).toBe("/api/users/scope/set")
      expect(sentJson(fetch)).toEqual({
        username: "kid",
        scope: { library_root: "kid", books: ["**/*.epub"] },
      })
    })
  })
})
