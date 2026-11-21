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
    /** The section's index in the book's spine. */
    index: number
  }

  /** foliate keys drawn annotations by `value`; Shelf uses the highlight's CFI. */
  export interface AnnotationRef {
    value: string
  }

  export interface DrawAnnotationDetail {
    /** Draws with `draw(rects, options)`, which returns the SVG element to show. */
    draw: <O>(draw: (rects: DOMRectList, options: O) => SVGElement, options: O) => void
    annotation: AnnotationRef
    doc: Document
    range: Range
  }

  export interface ShowAnnotationDetail {
    value: string
    index: number
    range: Range
  }

  /** The paginator for reflowable books. Its `flow` attribute is `scrolled` or `paginated`. */
  export interface Renderer extends HTMLElement {
    /** CSS added to every section's document. */
    setStyles(css: string): void
    /** The section documents loaded now (one, for reflowable books). */
    getContents(): { doc: Document; index: number }[]
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
      metadata?: { title?: string | Record<string, string>; language?: string | string[] }
      toc?: TocItem[]
      /** `"rtl"` for books read right to left. */
      dir?: string
    }
    /** Set by `open()`. */
    renderer?: Renderer
    open(file: File): Promise<void>
    init(options: { lastLocation?: string | null; showTextStart?: boolean }): Promise<void>
    goTo(href: string): Promise<unknown>
    /** Draws it (through `draw-annotation`) if its section is loaded; resolves once done. */
    addAnnotation(annotation: AnnotationRef): Promise<unknown>
    deleteAnnotation(annotation: AnnotationRef): Promise<unknown>
    getCFI(index: number, range: Range): string
    resolveCFI(cfi: string): { index: number }
    getProgressOf(index: number, range: Range): { tocItem?: TocItem | null }
    goLeft(): Promise<void>
    goRight(): Promise<void>
    close(): void
    addEventListener(
      type: "relocate",
      listener: (e: CustomEvent<RelocateDetail>) => void,
    ): void
    addEventListener(type: "load", listener: (e: CustomEvent<LoadDetail>) => void): void
    addEventListener(
      type: "draw-annotation",
      listener: (e: CustomEvent<DrawAnnotationDetail>) => void,
    ): void
    /** A section's overlay was made, empty: draw its annotations. */
    addEventListener(
      type: "create-overlay",
      listener: (e: CustomEvent<{ index: number }>) => void,
    ): void
    /** A tap or click landed on a drawn annotation. */
    addEventListener(
      type: "show-annotation",
      listener: (e: CustomEvent<ShowAnnotationDetail>) => void,
    ): void
    addEventListener(
      type: string,
      listener: EventListenerOrEventListenerObject | null,
      options?: boolean | AddEventListenerOptions,
    ): void
  }
}

declare module "foliate-js/epubcfi.js" {
  /** Negative when `a` comes first in the book. */
  export function compare(a: string, b: string): number
}

declare module "foliate-js/overlayer.js" {
  export class Overlayer {
    /** Filled boxes, faded and blended by `--overlayer-highlight-opacity` and `-blend-mode`. */
    static highlight(rects: DOMRectList, options?: { color?: string }): SVGElement
  }
}

interface HTMLElementTagNameMap {
  "foliate-view": import("foliate-js/view.js").View
}
