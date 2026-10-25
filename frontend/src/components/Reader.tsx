import { useQuery } from "@tanstack/react-query"
import type { Renderer, TocItem, View } from "foliate-js/view.js"
import { ChevronLeft, ChevronRight, LoaderCircle } from "lucide-react"
import { type Ref, useEffect, useImperativeHandle, useRef, useState } from "react"
import { getBookFile, getProgress } from "@/api"
import { ProgressBar } from "@/components/ProgressBar"
import { Button } from "@/components/ui/button"
import { type Appearance, readerBackground, readerLayout, type Scheme } from "@/lib/appearance"
import { applyAppearance, sectionFraction, turn } from "@/lib/foliate"
import { type Direction, swipeDirection, wheelTurner } from "@/lib/gestures"
import { progressSaver } from "@/lib/progressSaver"
import { useMediaQuery } from "@/lib/useMediaQuery"
import { cn } from "cn"

export type ReaderHandle = { goTo: (href: string) => void }

type Props = {
  id: string
  /** While true (e.g. a drawer is open), keys, swipes and the wheel are ignored. */
  paused: boolean
  appearance: Appearance["reader"]
  onOpen: (book: { title: string; toc: TocItem[] }) => void
  /** The `id` of the TOC entry the reader is in, as pages turn. */
  onLocation: (tocId: number | undefined) => void
  onOpenLibrary: () => void
  ref?: Ref<ReaderHandle>
}

/** The wrapper's border box, which the padding we add doesn't change. */
const measure = (el: HTMLElement) => ({ width: el.offsetWidth, height: el.offsetHeight })

/** Kept clear of text on each side while the buttons float: 16px in, 40px wide, 16px out. */
const BUTTON_INSET = 72

/** A swipe to the left reveals what's on the right, like the right arrow. */
const opposite = (d: Direction): Direction => (d === "left" ? "right" : "left")

/**
 * Opens the book at its saved position and saves the position as pages turn.
 *
 * Input depends on the layout and the device:
 * - Paginated: foliate turns pages on swipe; ← → and the buttons do too.
 * - Scrolled: the section scrolls natively, with a bar under the header instead of a scrollbar.
 *   A horizontal swipe, a horizontal wheel, ← → or the buttons change section.
 * - Buttons float at the sides for mouse users. Touch screens get them in a footer when
 *   paginated, and none when scrolled.
 */
export function Reader({ id, paused, appearance, onOpen, onLocation, onOpenLibrary, ref }: Props) {
  const containerRef = useRef<HTMLDivElement>(null)
  const sectionBarRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<View | null>(null)
  const rendererRef = useRef<Renderer | null>(null)
  const [openError, setOpenError] = useState<unknown>(null)
  const [size, setSize] = useState({ width: 0, height: 0 })
  const [bookFraction, setBookFraction] = useState(0)
  const coarse = useMediaQuery("(pointer: coarse)")

  const { flow, scheme } = appearance
  const unit = flow === "scrolled" ? "section" : "page"
  const sectionBar = flow === "scrolled"
  const buttons = !coarse ? "floating" : flow === "paginated" ? "footer" : "none"
  const inset = buttons === "floating" ? BUTTON_INSET : 0
  const layout = readerLayout(size, appearance, { clipScrollbar: sectionBar, inset })

  // For listeners that are attached once but must see the latest props.
  const latest = useRef({ paused, appearance, sectionBar, inset, onOpen, onLocation })
  useEffect(() => {
    latest.current = { paused, appearance, sectionBar, inset, onOpen, onLocation }
  })

  useImperativeHandle(ref, () => ({ goTo: (href) => void viewRef.current?.goTo(href) }), [])

  useEffect(() => {
    const container = containerRef.current
    if (!container) return
    const observer = new ResizeObserver(() => setSize(measure(container)))
    observer.observe(container)
    return () => observer.disconnect()
  }, [])

  const syncSectionBar = () => {
    const fill = sectionBarRef.current
    const renderer = rendererRef.current
    if (fill && renderer) fill.style.transform = `scaleX(${sectionFraction(renderer)})`
  }

  // Progress is read once, when the book opens; nothing refetches it while reading.
  const book = useQuery({
    queryKey: ["open-book", id],
    queryFn: async () => {
      const [blob, progress] = await Promise.all([getBookFile(id), getProgress(id)])
      // foliate reads `file.name` to detect the format, so a bare Blob won't do.
      const file = new File([blob], `${id}.epub`, { type: "application/epub+zip" })
      return { file, cfi: progress.epubProgress }
    },
    staleTime: Infinity,
    gcTime: 0,
    retry: 1,
    refetchOnWindowFocus: false,
  })

  useEffect(() => {
    const data = book.data
    const container = containerRef.current
    if (!data || !container) return

    let cancelled = false
    let view: View | undefined
    const saver = progressSaver(id, data.cfi)
    const unit = () => (latest.current.appearance.flow === "scrolled" ? "section" : "page")

    const onKey = (e: KeyboardEvent) => {
      if (!view || latest.current.paused) return
      if (e.key === "ArrowLeft") void turn(view, "left", unit())
      if (e.key === "ArrowRight") void turn(view, "right", unit())
    }

    let touch: { x: number; y: number; t: number } | null = null
    const onTouchStart = (e: TouchEvent) => {
      const t = e.changedTouches[0]
      // A second finger is a pinch, not a swipe.
      touch = e.touches.length === 1 && t ? { x: t.screenX, y: t.screenY, t: e.timeStamp } : null
    }
    const onTouchEnd = (e: TouchEvent) => {
      const start = touch
      touch = null
      const t = e.changedTouches[0]
      if (!start || !t || !view || latest.current.paused || unit() !== "section") return
      if ((visualViewport?.scale ?? 1) !== 1) return
      const dir = swipeDirection({
        dx: t.screenX - start.x,
        dy: t.screenY - start.y,
        ms: e.timeStamp - start.t,
      })
      if (dir) void turn(view, opposite(dir), "section")
    }

    const wheel = wheelTurner()
    const onWheel = (e: WheelEvent) => {
      if (!view || latest.current.paused || unit() !== "section") return
      if (Math.abs(e.deltaX) <= Math.abs(e.deltaY)) return
      // Keeps macOS from reading the swipe as browser back/forward.
      e.preventDefault()
      const perLine = e.deltaMode === WheelEvent.DOM_DELTA_LINE ? 16 : 1
      const step = wheel(e.deltaX * perLine, e.timeStamp)
      if (step) void turn(view, step > 0 ? "right" : "left", "section")
    }

    /** Sections are iframes, so each document needs its own listeners. */
    const listen = (target: Document | HTMLElement) => {
      target.addEventListener("keydown", onKey as EventListener)
      target.addEventListener("touchstart", onTouchStart as EventListener, { passive: true })
      target.addEventListener("touchend", onTouchEnd as EventListener, { passive: true })
      target.addEventListener("wheel", onWheel as EventListener, { passive: false })
    }
    const unlisten = (target: Document | HTMLElement) => {
      target.removeEventListener("keydown", onKey as EventListener)
      target.removeEventListener("touchstart", onTouchStart as EventListener)
      target.removeEventListener("touchend", onTouchEnd as EventListener)
      target.removeEventListener("wheel", onWheel as EventListener)
    }

    const onHidden = () => {
      if (document.visibilityState === "hidden") saver.flush()
    }

    const open = async () => {
      await import("foliate-js/view.js")
      if (cancelled) return
      view = document.createElement("foliate-view")
      // No width: the side margins set it.
      view.className = "block h-full"
      view.addEventListener("relocate", (e) => {
        saver.schedule(e.detail.cfi)
        latest.current.onLocation(e.detail.tocItem?.id)
        setBookFraction(e.detail.fraction ?? 0)
        syncSectionBar()
      })
      view.addEventListener("load", (e) => listen(e.detail.doc))
      container.append(view)

      await view.open(data.file)
      if (cancelled || !view.renderer) return
      rendererRef.current = view.renderer
      view.renderer.addEventListener("scroll", syncSectionBar)
      // Before `init`, so the first page is laid out once, with the right flow and margins.
      const { appearance, sectionBar, inset } = latest.current
      const layout = readerLayout(measure(container), appearance, {
        clipScrollbar: sectionBar,
        inset,
      })
      applyAppearance(view, appearance, layout)

      const title = view.book?.metadata?.title
      latest.current.onOpen({
        title: typeof title === "string" ? title : (Object.values(title ?? {})[0] ?? ""),
        toc: view.book?.toc ?? [],
      })
      await view.init({ lastLocation: data.cfi, showTextStart: true })
      if (!cancelled) viewRef.current = view
    }
    open().catch((e: unknown) => {
      if (!cancelled) setOpenError(e)
    })

    // The wrapper catches keys when focus is outside the book, and touches in the margins.
    listen(container)
    window.addEventListener("keydown", onKey)
    window.addEventListener("pagehide", saver.flush)
    document.addEventListener("visibilitychange", onHidden)
    return () => {
      cancelled = true
      saver.flush()
      unlisten(container)
      window.removeEventListener("keydown", onKey)
      window.removeEventListener("pagehide", saver.flush)
      document.removeEventListener("visibilitychange", onHidden)
      viewRef.current = null
      rendererRef.current = null
      view?.close()
      view?.remove()
    }
  }, [id, book.data])

  const { marginLeft, marginRight, gapPercent, pageMarginRight } = layout
  useEffect(() => {
    // Any opened view, even one still running `init`, which `viewRef` doesn't hold yet.
    const view = containerRef.current?.querySelector("foliate-view")
    if (!view || size.width === 0) return
    applyAppearance(view, { flow, scheme }, { marginLeft, marginRight, gapPercent, pageMarginRight })
    syncSectionBar()
    // syncSectionBar only reads refs.
  }, [flow, scheme, marginLeft, marginRight, gapPercent, pageMarginRight, size.width])

  const goLeft = () => viewRef.current && void turn(viewRef.current, "left", unit)
  const goRight = () => viewRef.current && void turn(viewRef.current, "right", unit)

  const error = book.error ?? openError
  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4 p-6 text-center">
        <p className="font-medium">Couldn't open this book</p>
        <p className="max-w-sm text-sm text-destructive">{String(error)}</p>
        <Button onClick={onOpenLibrary}>Open library</Button>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col" style={{ background: readerBackground(scheme) }}>
      {sectionBar && <ProgressBar label="Position in this section" fillRef={sectionBarRef} />}
      <div className="relative min-h-0 flex-1">
        <div
          ref={containerRef}
          className="size-full overflow-hidden"
          style={{ paddingTop: layout.paddingTop, paddingBottom: layout.paddingBottom }}
        />
        {book.isPending && (
          <LoaderCircle className="absolute inset-0 m-auto size-8 animate-spin text-muted-foreground" />
        )}
        {buttons === "floating" && (
          <>
            <PageButton
              side="left"
              unit={unit}
              scheme={scheme}
              onClick={goLeft}
              className="absolute top-1/2 left-4 -translate-y-1/2"
            />
            <PageButton
              side="right"
              unit={unit}
              scheme={scheme}
              onClick={goRight}
              className="absolute top-1/2 right-4 -translate-y-1/2"
            />
          </>
        )}
      </div>
      {buttons === "footer" && (
        <footer
          className={cn(
            "flex h-14 shrink-0 items-center justify-between border-t px-4",
            scheme === "light" ? "border-neutral-200" : "border-neutral-800",
          )}
        >
          <PageButton side="left" unit={unit} scheme={scheme} onClick={goLeft} />
          <PageButton side="right" unit={unit} scheme={scheme} onClick={goRight} />
        </footer>
      )}
      <ProgressBar label="Position in the book" fraction={bookFraction} />
    </div>
  )
}

type PageButtonProps = {
  side: Direction
  unit: "page" | "section"
  scheme: Scheme
  onClick: () => void
  className?: string
}

function PageButton({ side, unit, scheme, onClick, className }: PageButtonProps) {
  const Icon = side === "left" ? ChevronLeft : ChevronRight
  return (
    <button
      type="button"
      aria-label={`${side === "left" ? "Previous" : "Next"} ${unit}`}
      onClick={onClick}
      className={cn(
        "inline-flex size-10 items-center justify-center rounded-full shadow-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50",
        scheme === "light"
          ? "bg-neutral-200/90 text-neutral-900 hover:bg-neutral-300"
          : "bg-neutral-700/90 text-neutral-50 hover:bg-neutral-600",
        className,
      )}
    >
      <Icon className="size-5" />
    </button>
  )
}
