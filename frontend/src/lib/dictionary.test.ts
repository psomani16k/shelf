// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest"
import {
  fetchDefinitions,
  isDefinable,
  normalizeTerm,
  orderLanguages,
  type RawLookup,
  titleCandidates,
  toLanguages,
} from "./dictionary"

describe("normalizeTerm", () => {
  it("strips surrounding punctuation and collapses whitespace", () => {
    expect(normalizeTerm("  “Running,”\n")).toBe("Running")
    expect(normalizeTerm("(well-known)")).toBe("well-known")
    expect(normalizeTerm("ice\n  cream.")).toBe("ice cream")
  })

  it("straightens apostrophes and keeps inner ones", () => {
    expect(normalizeTerm("‘don’t’")).toBe("don't")
  })

  it("is empty for punctuation alone", () => {
    expect(normalizeTerm(" — ")).toBe("")
  })
})

describe("isDefinable", () => {
  it("takes a word or a short phrase", () => {
    expect(isDefinable("lamp.")).toBe(true)
    expect(isDefinable("in a row")).toBe(true)
  })

  it("refuses sentences, long text and nothing", () => {
    expect(isDefinable("the storm came in")).toBe(false)
    expect(isDefinable("a".repeat(65))).toBe(false)
    expect(isDefinable("…")).toBe(false)
  })
})

describe("titleCandidates", () => {
  it("tries the lowercase title after the selected one", () => {
    expect(titleCandidates("Running")).toEqual(["Running", "running"])
    expect(titleCandidates("lamp")).toEqual(["lamp"])
  })
})

const RAW: RawLookup = {
  fr: [{ partOfSpeech: "Verb", language: "French", definitions: [{ definition: "to run" }] }],
  en: [
    {
      partOfSpeech: "Noun",
      language: "English",
      definitions: [
        { definition: '<span typeof="mw:Transclusion"></span>' },
        { definition: "A light.", examples: ["<b>lamp</b> lit", ""] },
      ],
    },
    { partOfSpeech: "Verb", language: "English", definitions: [{ definition: " " }] },
  ],
  de: [{ partOfSpeech: "Noun", language: "German", definitions: [] }],
}

describe("toLanguages", () => {
  it("drops empty definitions and entries, and sorts by language name", () => {
    const languages = toLanguages(RAW)
    expect(languages.map((l) => l.language)).toEqual(["English", "French"])
    const [english] = languages
    expect(english?.entries).toHaveLength(1)
    expect(english?.entries[0]?.definitions).toEqual([
      {
        text: [{ kind: "text", text: "A light." }],
        examples: [
          [
            { kind: "bold", children: [{ kind: "text", text: "lamp" }] },
            { kind: "text", text: " lit" },
          ],
        ],
      },
    ])
  })
})

describe("orderLanguages", () => {
  const languages = toLanguages(RAW)
  it("puts the book's language first, then English", () => {
    expect(orderLanguages(languages, "fr-CA").map((l) => l.code)).toEqual(["fr", "en"])
    expect(orderLanguages(languages, undefined).map((l) => l.code)).toEqual(["en", "fr"])
  })
})

describe("fetchDefinitions", () => {
  afterEach(() => vi.unstubAllGlobals())

  const answer = (status: number, body: unknown = {}) =>
    new Response(JSON.stringify(body), { status })

  it("falls back to the lowercase title on a 404", async () => {
    const fetch = vi.fn().mockResolvedValueOnce(answer(404)).mockResolvedValueOnce(answer(200, RAW))
    vi.stubGlobal("fetch", fetch)
    const found = await fetchDefinitions("Ice cream")
    expect(found?.title).toBe("ice cream")
    expect(fetch.mock.calls.map((c) => c[0])).toEqual([
      "https://en.wiktionary.org/api/rest_v1/page/definition/Ice_cream",
      "https://en.wiktionary.org/api/rest_v1/page/definition/ice_cream",
    ])
  })

  it("is null when no title exists", async () => {
    vi.stubGlobal("fetch", vi.fn().mockImplementation(async () => answer(404)))
    expect(await fetchDefinitions("Qwzxv")).toBeNull()
  })

  it("throws on other failures", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(answer(500)))
    await expect(fetchDefinitions("lamp")).rejects.toThrow("Wiktionary answered 500")
  })
})
