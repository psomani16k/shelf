import { useQuery } from "@tanstack/react-query"
import { clampChars } from "@/lib/highlights"
import { definitionKey } from "@/lib/queryKeys"
import { parseWikiHtml, plainText, type WikiNode } from "@/lib/wikiHtml"

const API = "https://en.wiktionary.org/api/rest_v1/page/definition/"
const ENTRY_URL = "https://en.wiktionary.org/wiki/"

const MAX_TERM_CHARS = 100
const MAX_DEFINABLE_CHARS = 64
const MAX_DEFINABLE_WORDS = 3

/** The selection as a dictionary title: no surrounding punctuation, one line, straight apostrophes. */
export function normalizeTerm(raw: string): string {
  const term = raw
    .replace(/[’‘]/g, "'")
    .replace(/\s+/g, " ")
    .replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "")
  return clampChars(term, MAX_TERM_CHARS)
}

/** A word or a short phrase; longer selections only get *Search web*. */
export function isDefinable(text: string): boolean {
  const term = normalizeTerm(text)
  return (
    term !== "" &&
    term.length <= MAX_DEFINABLE_CHARS &&
    term.split(" ").length <= MAX_DEFINABLE_WORDS
  )
}

/** Titles are case-sensitive, and a word starting a sentence is capitalised. */
export function titleCandidates(term: string): string[] {
  return [...new Set([term, term.toLowerCase()])]
}

export const entryUrl = (title: string) => ENTRY_URL + encodeURIComponent(title.replaceAll(" ", "_"))

type RawDefinition = { definition?: string; examples?: string[] }
type RawEntry = { partOfSpeech?: string; language?: string; definitions?: RawDefinition[] }
/** The API's answer: entries by the word's language code. */
export type RawLookup = Record<string, RawEntry[] | undefined>

export type Definition = { text: WikiNode[]; examples: WikiNode[][] }
export type Entry = { partOfSpeech: string; definitions: Definition[] }
export type LanguageEntries = { code: string; language: string; entries: Entry[] }
export type Lookup = { title: string; languages: LanguageEntries[] }

/** The first title Wiktionary has, or `null` when it has none of them. */
export async function fetchDefinitions(
  term: string,
  signal?: AbortSignal,
): Promise<{ title: string; raw: RawLookup } | null> {
  for (const title of titleCandidates(term)) {
    const res = await fetch(API + encodeURIComponent(title.replaceAll(" ", "_")), {
      headers: { "Api-User-Agent": "Shelf" },
      signal,
    })
    if (res.status === 404) continue
    if (!res.ok) throw new Error(`Wiktionary answered ${res.status}`)
    return { title, raw: (await res.json()) as RawLookup }
  }
  return null
}

const hasText = (nodes: WikiNode[]) => plainText(nodes).trim() !== ""

/** Parsed, without empty definitions or entries, languages by name. */
export function toLanguages(raw: RawLookup): LanguageEntries[] {
  return Object.entries(raw)
    .map(([code, list = []]) => ({
      code,
      language: list.find((e) => e.language)?.language ?? code,
      entries: list
        .map((e) => ({
          partOfSpeech: e.partOfSpeech ?? "",
          definitions: (e.definitions ?? [])
            .map((d) => ({
              text: parseWikiHtml(d.definition ?? ""),
              examples: (d.examples ?? []).map(parseWikiHtml).filter(hasText),
            }))
            .filter((d) => hasText(d.text)),
        }))
        .filter((e) => e.definitions.length > 0),
    }))
    .filter((l) => l.entries.length > 0)
    .sort((a, b) => a.language.localeCompare(b.language))
}

/** The book's language first, then English, then the rest as they were. */
export function orderLanguages(
  languages: readonly LanguageEntries[],
  bookLanguage: string | undefined,
): LanguageEntries[] {
  const book = bookLanguage?.split("-")[0]?.toLowerCase()
  const rank = (code: string) => (code === book ? 0 : code === "en" ? 1 : 2)
  return [...languages].sort((a, b) => rank(a.code) - rank(b.code))
}

/** `null` data is "no entry"; each term is fetched once per session. */
export function useDefinition(term: string) {
  return useQuery({
    queryKey: definitionKey(term),
    queryFn: async ({ signal }): Promise<Lookup | null> => {
      const found = await fetchDefinitions(term, signal)
      return found && { title: found.title, languages: toLanguages(found.raw) }
    },
    enabled: term !== "",
    staleTime: Infinity,
    retry: 1,
  })
}
