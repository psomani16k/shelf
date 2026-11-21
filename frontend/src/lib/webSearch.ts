import { SearchEngine_Value } from "@/gen/configpb/config_pb"
import { openExternal } from "@/lib/external"
import { clampChars } from "@/lib/highlights"
import { useLibraryConfig } from "@/lib/useAppearance"

/** Longer selections are cut, so the URL stays a sensible size. */
export const MAX_QUERY_CHARS = 500

type SearchEngine = { value: SearchEngine_Value; label: string; base: string }

/** In the order Settings lists them; the first is the default (the proto's zero value). */
export const SEARCH_ENGINES: SearchEngine[] = [
  { value: SearchEngine_Value.DUCKDUCKGO, label: "DuckDuckGo", base: "https://duckduckgo.com/?q=" },
  { value: SearchEngine_Value.GOOGLE, label: "Google", base: "https://www.google.com/search?q=" },
  { value: SearchEngine_Value.BING, label: "Bing", base: "https://www.bing.com/search?q=" },
  {
    value: SearchEngine_Value.STARTPAGE,
    label: "Startpage",
    base: "https://www.startpage.com/do/search?query=",
  },
  { value: SearchEngine_Value.ECOSIA, label: "Ecosia", base: "https://www.ecosia.org/search?q=" },
]

/** A value this build doesn't know searches with the default. */
export function searchUrl(engine: SearchEngine_Value | undefined, text: string): string {
  const { base } = SEARCH_ENGINES.find((e) => e.value === engine) ?? SEARCH_ENGINES[0]
  const query = clampChars(text.replace(/\s+/g, " ").trim(), MAX_QUERY_CHARS)
  return base + encodeURIComponent(query)
}

/** Searches the text with the user's engine (*Settings → Reader → Search web*). */
export function useSearchWeb(): (text: string) => Promise<void> {
  const engine = useLibraryConfig().data?.reader?.searchEngine
  return (text) => openExternal(searchUrl(engine, text))
}
