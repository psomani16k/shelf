/** Wiktionary's definition HTML, reduced to what the sheet shows. Never rendered as HTML. */
export type WikiNode =
  | { kind: "text"; text: string }
  | { kind: "bold" | "italic"; children: WikiNode[] }
  | { kind: "link"; title: string; children: WikiNode[] }
  | { kind: "list"; ordered: boolean; items: WikiNode[][] }
  | { kind: "break" }

const DROPPED = new Set(["SCRIPT", "STYLE", "IMG", "SUP", "LINK", "META", "TEMPLATE", "svg"])

/**
 * Parsed into an inert document (`DOMParser` runs no scripts and loads nothing), then walked into
 * `WikiNode`s. Links to other entries are kept; everything else is text and a little formatting.
 */
export function parseWikiHtml(html: string): WikiNode[] {
  const doc = new DOMParser().parseFromString(html, "text/html")
  return merge(walkChildren(doc.body))
}

/** All the text, for telling an empty definition from a real one. */
export function plainText(nodes: readonly WikiNode[]): string {
  return nodes
    .map((n) => {
      switch (n.kind) {
        case "text":
          return n.text
        case "break":
          return " "
        case "list":
          return n.items.map(plainText).join(" ")
        default:
          return plainText(n.children)
      }
    })
    .join("")
}

function walkChildren(parent: Node): WikiNode[] {
  return [...parent.childNodes].flatMap(walk)
}

function walk(node: Node): WikiNode[] {
  if (node.nodeType === Node.TEXT_NODE) return [{ kind: "text", text: node.textContent ?? "" }]
  if (node.nodeType !== Node.ELEMENT_NODE) return []
  const el = node as Element
  if (DROPPED.has(el.tagName)) return []
  switch (el.tagName) {
    case "B":
    case "STRONG":
      return [{ kind: "bold", children: merge(walkChildren(el)) }]
    case "I":
    case "EM":
      return [{ kind: "italic", children: merge(walkChildren(el)) }]
    case "BR":
      return [{ kind: "break" }]
    case "OL":
    case "UL":
      return [
        {
          kind: "list",
          ordered: el.tagName === "OL",
          items: [...el.children]
            .filter((li) => li.tagName === "LI")
            .map((li) => merge(walkChildren(li))),
        },
      ]
    case "A": {
      const title = el.getAttribute("title") ?? ""
      const children = merge(walkChildren(el))
      // Namespaced titles (Appendix:, Category: …) aren't entries.
      const entry = el.getAttribute("rel") === "mw:WikiLink" && title !== "" && !title.includes(":")
      return entry ? [{ kind: "link", title, children }] : children
    }
    default:
      return walkChildren(el)
  }
}

/** Joins neighbouring text nodes, which unwrapping leaves behind. */
function merge(nodes: WikiNode[]): WikiNode[] {
  const out: WikiNode[] = []
  for (const node of nodes) {
    const last = out.at(-1)
    if (node.kind === "text" && last?.kind === "text") last.text += node.text
    else out.push(node.kind === "text" ? { ...node } : node)
  }
  return out
}
