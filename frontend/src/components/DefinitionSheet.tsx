import { ArrowLeft, Globe, LoaderCircle, Search } from "lucide-react"
import { Fragment, useMemo, useRef, useState } from "react"
import { Button } from "@/components/ui/button"
import { Drawer, DrawerContent, DrawerTitle } from "@/components/ui/drawer"
import { Input } from "@/components/ui/input"
import { useBackButton } from "@/lib/backButton"
import { entryUrl, normalizeTerm, orderLanguages, useDefinition } from "@/lib/dictionary"
import { openExternal } from "@/lib/external"
import { log } from "@/lib/log"
import type { WikiNode } from "@/lib/wikiHtml"

type Props = {
  open: boolean
  onClose: () => void
  /** The selected word. Changing `session` starts a fresh lookup with it. */
  term: string
  session: number
  /** The book's, to show its own language's entries first. */
  language?: string
  onSearchWeb: (text: string) => void
}

/** The reader's dictionary: Wiktionary's entries for an editable word, over the page. */
export function DefinitionSheet({ open, onClose, term, session, language, onSearchWeb }: Props) {
  return (
    <Drawer open={open} onOpenChange={(o) => !o && onClose()} showSwipeHandle>
      {/* Tall enough that the word field sits above Android's keyboard. */}
      <DrawerContent className="[--drawer-content-height:70dvh]">
        <DrawerTitle className="sr-only">Dictionary</DrawerTitle>
        <Lookup
          key={session}
          initial={term}
          open={open}
          onClose={onClose}
          language={language}
          onSearchWeb={onSearchWeb}
        />
      </DrawerContent>
    </Drawer>
  )
}

function Lookup({
  initial,
  open,
  onClose,
  language,
  onSearchWeb,
}: {
  initial: string
  open: boolean
  onClose: () => void
  language?: string
  onSearchWeb: (text: string) => void
}) {
  // Links and edits push; Back pops.
  const [history, setHistory] = useState([initial])
  const [draft, setDraft] = useState(initial)
  const fieldRef = useRef<HTMLInputElement>(null)
  const term = history.at(-1) ?? initial
  const query = useDefinition(term)
  const languages = useMemo(
    () => (query.data ? orderLanguages(query.data.languages, language) : []),
    [query.data, language],
  )

  const lookUp = (raw: string) => {
    const next = normalizeTerm(raw)
    if (next === "") return
    setDraft(next)
    if (next !== term) setHistory((h) => [...h, next])
  }
  const back = () => {
    const previous = history.at(-2)
    if (previous === undefined) return onClose()
    setHistory((h) => h.slice(0, -1))
    setDraft(previous)
  }
  useBackButton(back, open)

  const title = query.data?.title ?? term
  const openEntry = () =>
    openExternal(entryUrl(title)).catch((e: unknown) => {
      log.warn("couldn't open the Wiktionary entry", { error: String(e) })
    })

  return (
    <div className="flex min-h-0 flex-1 flex-col pb-[var(--inset-bottom)]">
      <form
        className="flex items-center gap-1 border-b px-2 pt-1 pb-2"
        onSubmit={(e) => {
          e.preventDefault()
          lookUp(draft)
          fieldRef.current?.blur()
        }}
      >
        {history.length > 1 && (
          <Button type="button" variant="ghost" size="icon" aria-label="Back" onClick={back}>
            <ArrowLeft />
          </Button>
        )}
        <Input
          ref={fieldRef}
          aria-label="Word"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          enterKeyHint="search"
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
          className="flex-1 text-base select-text"
        />
        <Button type="submit" variant="ghost" size="icon" aria-label="Look up">
          <Search />
        </Button>
      </form>

      <div className="flex-1 overflow-y-auto overscroll-contain px-4 py-3 select-text">
        {query.isPending ? (
          <LoaderCircle className="mx-auto mt-8 size-6 animate-spin text-muted-foreground" />
        ) : query.isError ? (
          <div className="mt-8 flex flex-col items-center gap-3 text-center">
            <p className="font-medium">Couldn't reach Wiktionary</p>
            <p className="text-muted-foreground">Looking words up needs an internet connection.</p>
            <Button onClick={() => void query.refetch()}>Try again</Button>
          </div>
        ) : languages.length === 0 ? (
          <div className="mt-8 flex flex-col items-center gap-3 text-center">
            <p>No Wiktionary entry for “{term}”.</p>
            <Button variant="outline" onClick={() => onSearchWeb(term)}>
              <Globe />
              Search web
            </Button>
          </div>
        ) : (
          languages.map((lang) => (
            <section key={lang.code} className="mb-4">
              {languages.length > 1 && <h3 className="mb-1 text-base font-semibold">{lang.language}</h3>}
              {lang.entries.map((entry, i) => (
                <div key={i} className="mb-3">
                  <p className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
                    {entry.partOfSpeech}
                  </p>
                  <ol className="mt-1 flex list-decimal flex-col gap-1.5 pl-5 text-base">
                    {entry.definitions.map((d, j) => (
                      <li key={j}>
                        <WikiText nodes={d.text} onLookup={lookUp} />
                        {d.examples.map((ex, k) => (
                          <p key={k} className="mt-0.5 text-sm text-muted-foreground italic">
                            <WikiText nodes={ex} onLookup={lookUp} />
                          </p>
                        ))}
                      </li>
                    ))}
                  </ol>
                </div>
              ))}
            </section>
          ))
        )}
      </div>

      <div className="flex items-center gap-2 border-t px-2 pt-2">
        <Button variant="ghost" size="sm" onClick={() => onSearchWeb(term)}>
          <Globe />
          Search web
        </Button>
        <p className="ml-auto pr-2 text-xs text-muted-foreground">
          From{" "}
          <button type="button" className="underline underline-offset-2" onClick={openEntry}>
            Wiktionary
          </button>{" "}
          · CC BY-SA 4.0
        </p>
      </div>
    </div>
  )
}

function WikiText({
  nodes,
  onLookup,
}: {
  nodes: readonly WikiNode[]
  onLookup: (title: string) => void
}) {
  return nodes.map((node, i) => (
    <Fragment key={i}>
      <WikiPart node={node} onLookup={onLookup} />
    </Fragment>
  ))
}

function WikiPart({ node, onLookup }: { node: WikiNode; onLookup: (title: string) => void }) {
  switch (node.kind) {
    case "text":
      return node.text
    case "break":
      return <br />
    case "bold":
      return (
        <b>
          <WikiText nodes={node.children} onLookup={onLookup} />
        </b>
      )
    case "italic":
      return (
        <i>
          <WikiText nodes={node.children} onLookup={onLookup} />
        </i>
      )
    case "link":
      return (
        <button
          type="button"
          className="text-primary underline decoration-primary/40 underline-offset-2"
          onClick={() => onLookup(node.title)}
        >
          <WikiText nodes={node.children} onLookup={onLookup} />
        </button>
      )
    case "list": {
      const List = node.ordered ? "ol" : "ul"
      return (
        <List className={node.ordered ? "list-[lower-alpha] pl-5" : "list-disc pl-5"}>
          {node.items.map((item, i) => (
            <li key={i}>
              <WikiText nodes={item} onLookup={onLookup} />
            </li>
          ))}
        </List>
      )
    }
  }
}
