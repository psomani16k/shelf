// Types for the parts of the foliate-js submodule (plain JS) that Shelf uses.

declare module "foliate-js/view.js" {
  export interface TocItem {
    /** Assigned by foliate when the book opens; unique within the book. */
    id: number
    label: string
    /** Null for a heading with no link of its own. */
    href: string | null
    subitems?: TocItem[] | null
  }

  export interface RelocateDetail {
    cfi: string
    /** 0–1 through the whole book. */
    fraction: number
    tocItem?: TocItem | null
  }

  export interface LoadDetail {
    doc: Document
    index: number
  }

  /** The paginator for reflowable books. Its `flow` attribute is `scrolled` or `paginated`. */
  export interface Renderer extends HTMLElement {
    /** CSS added to every section's document. */
    setStyles(css: string): void
    readonly scrolled: boolean
    /** Scroll offset, viewport size and content size along the reading axis, in px. */
    readonly start: number
    readonly size: number
    readonly viewSize: number
    prevSection(): Promise<unknown>
    nextSection(): Promise<unknown>
    /** Fired on every scroll frame of the section. */
    addEventListener(type: "scroll", listener: () => void): void
    addEventListener(
      type: string,
      listener: EventListenerOrEventListenerObject | null,
      options?: boolean | AddEventListenerOptions,
    ): void
    removeEventListener(type: "scroll", listener: () => void): void
    removeEventListener(
      type: string,
      listener: EventListenerOrEventListenerObject | null,
      options?: boolean | EventListenerOptions,
    ): void
  }

  export class View extends HTMLElement {
    book?: {
      metadata?: { title?: string | Record<string, string> }
      toc?: TocItem[]
      /** `"rtl"` for books read right to left. */
      dir?: string
    }
    /** Set by `open()`. */
    renderer?: Renderer
    open(file: File): Promise<void>
    init(options: { lastLocation?: string | null; showTextStart?: boolean }): Promise<void>
    goTo(href: string): Promise<unknown>
    goLeft(): Promise<void>
    goRight(): Promise<void>
    close(): void
    addEventListener(
      type: "relocate",
      listener: (e: CustomEvent<RelocateDetail>) => void,
    ): void
    addEventListener(type: "load", listener: (e: CustomEvent<LoadDetail>) => void): void
    addEventListener(
      type: string,
      listener: EventListenerOrEventListenerObject | null,
      options?: boolean | AddEventListenerOptions,
    ): void
  }
}

interface HTMLElementTagNameMap {
  "foliate-view": import("foliate-js/view.js").View
}
