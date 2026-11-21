import { describe, expect, it } from "vitest"
import { SearchEngine_Value } from "@/gen/configpb/config_pb"
import { MAX_QUERY_CHARS, SEARCH_ENGINES, searchUrl } from "./webSearch"

describe("searchUrl", () => {
  it("encodes the query for each engine", () => {
    expect(searchUrl(SearchEngine_Value.DUCKDUCKGO, "café & tea?")).toBe(
      "https://duckduckgo.com/?q=caf%C3%A9%20%26%20tea%3F",
    )
    for (const engine of SEARCH_ENGINES) {
      expect(searchUrl(engine.value, "a b")).toBe(`${engine.base}a%20b`)
    }
  })

  it("collapses whitespace", () => {
    expect(searchUrl(SearchEngine_Value.GOOGLE, "  two\n  lines ")).toBe(
      "https://www.google.com/search?q=two%20lines",
    )
  })

  it("defaults to DuckDuckGo when unset or unknown", () => {
    expect(searchUrl(undefined, "x")).toBe("https://duckduckgo.com/?q=x")
    expect(searchUrl(99 as SearchEngine_Value, "x")).toBe("https://duckduckgo.com/?q=x")
  })

  it("cuts long selections", () => {
    const url = searchUrl(SearchEngine_Value.BING, "a".repeat(MAX_QUERY_CHARS + 50))
    expect(url).toBe(`https://www.bing.com/search?q=${"a".repeat(MAX_QUERY_CHARS)}`)
  })
})
