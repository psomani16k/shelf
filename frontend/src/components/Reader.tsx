import { useQuery } from "@tanstack/react-query"
import type { Renderer, TocItem, View } from "foliate-js/view.js"
import {
  ALargeSmall,
  ChevronLeft,
  ChevronRight,
  LoaderCircle,
  Minus,
  Plus,
  RotateCcw,
} from "lucide-react"
import { type Ref, useEffect, useImperativeHandle, useRef, useState } from "react"
import { getBookFile, getProgress } from "@/api"
import { ProgressBar } from "@/components/ProgressBar"
import { Button } from "@/components/ui/button"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { type Appearance, readerBackground, readerLayout, type Scheme } from "@/lib/appearance"
import { applyAppearance, sectionFraction, turn } from "@/lib/foliate"
import { type Direction, swipeDirection, wheelTurner } from "@/lib/gestures"
import { progressSaver } from "@/lib/progressSaver"
import { stepZoom, useReaderZoom, ZOOM_STEPS } from "@/lib/readerZoom"
import { useUiScale } from "@/lib/uiScale"
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
  /** Whether the header and the bottom bar show. Hidden, only their 3px position lines do. */
  bars: boolean
  onBarsChange: (shown: boolean) => void
  ref?: Ref<ReaderHandle>
}

/** The wrapper's border box, which the padding we add doesn't change. */
const measure = (el: HTMLElement) => ({ width: el.offsetWidth, height: el.offsetHeight })

/**
 * Kept clear of text on each side while the buttons float: 16px in, 40px wide, 16px out, at
 * 100% interface scale.
 */
const BUTTON_INSET = 72

/** The position lines' height, kept clear of text since they never hide. */
const LINE = 3

/** px a finger must move for a touch to count as reading (a swipe or a scroll), not a tap. */
const TAP_SLOP = 10

/** Keys that turn or scroll the page. */
const READING_KEYS = new Set([
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "ArrowDown",
  "PageUp",
  "PageDown",
  " ",
])

/** A swipe to the left reveals what's on the right, like the right arrow. */
const opposite = (d: Direction): Direction => (d === "left" ? "right" : "left")

/**
 * Opens the book at its saved position and saves the position as pages turn.
 *
 * Input depends on the layout and the device:
 * - Paginated: foliate turns pages on swipe; ← → and the buttons do too.
 * - Scrolled: the section scrolls natively, with a line under the header instead of a scrollbar.
 *   A horizontal swipe, a horizontal wheel, ← → or the buttons change section.
 * - Buttons float at the sides for mouse users, and are always in the bottom bar.
 * - The header and the bottom bar cover the page. Reading (a swipe, a scroll, a reading key or
 *   a floating button) hides them; a tap or click on the page, not on a link, toggles them.
 */
export function Reader({
  id,
  paused,
  appearance,
  onOpen,
  onLocation,
  onOpenLibrary,
  bars,
  onBarsChange,
  ref,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null)
  const sectionBarRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<View | null>(null)
  const rendererRef = useRef<Renderer | null>(null)
  const [openError, setOpenError] = useState<unknown>(null)
  const [size, setSize] = useState({ width: 0, height: 0 })
  const [bookFraction, setBookFraction] = useState(0)
  const [chapter, setChapter] = useState("")
  const [zoom, setZoom] = useReaderZoom()
  const uiScale = useUiScale()
  const bookZoom = Math.round(uiScale * zoom * 100) / 100
  const coarse = useMediaQuery("(pointer: coarse)")

  const { flow, scheme } = appearance
  const unit = flow === "scrolled" ? "section" : "page"
  const sectionBar = flow === "scrolled"
  const floating = !coarse
  const inset = floating ? Math.round(BUTTON_INSET * uiScale) : 0
  const layout = readerLayout(size, appearance, { clipScrollbar: sectionBar, inset })

  // For listeners that are attached once but must see the latest props.
  const props = {
    paused,
    appearance,
    bookZoom,
    sectionBar,
    inset,
    onOpen,
    onLocation,
    bars,
    onBarsChange,
  }
  const latest = useRef(props)
  useEffect(() => {
    latest.current = props
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

    const hideBars = () => latest.current.bars && latest.current.onBarsChange(false)

    const onKey = (e: KeyboardEvent) => {
      if (!view || latest.current.paused) return
      // Space or an arrow on a focused button isn't reading.
      const control = (e.target as Element | null)?.closest?.("button, input, select, textarea")
      if (READING_KEYS.has(e.key) && !control) hideBars()
      if (e.key === "ArrowLeft") void turn(view, "left", unit())
      if (e.key === "ArrowRight") void turn(view, "right", unit())
    }

    const onClick = (e: MouseEvent) => {
      if (latest.current.paused || e.defaultPrevented) return
      // Iframe elements aren't `instanceof Element` here, hence the optional call.
      if ((e.target as Element | null)?.closest?.("a[href]")) return
      // The end of a text selection isn't a tap.
      if (e.view?.getSelection()?.isCollapsed === false) return
      latest.current.onBarsChange(!latest.current.bars)
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
      if (!start || !t || !view || latest.current.paused) return
      const dx = t.screenX - start.x
      const dy = t.screenY - start.y
      if (Math.hypot(dx, dy) >= TAP_SLOP) hideBars()
      if (unit() !== "section" || (visualViewport?.scale ?? 1) !== 1) return
      const dir = swipeDirection({ dx, dy, ms: e.timeStamp - start.t })
      if (dir) void turn(view, opposite(dir), "section")
    }

    const wheel = wheelTurner()
    const onWheel = (e: WheelEvent) => {
      if (!view || latest.current.paused || unit() !== "section") return
      hideBars()
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
      target.addEventListener("click", onClick as EventListener)
      target.addEventListener("touchstart", onTouchStart as EventListener, { passive: true })
      target.addEventListener("touchend", onTouchEnd as EventListener, { passive: true })
      target.addEventListener("wheel", onWheel as EventListener, { passive: false })
    }
    const unlisten = (target: Document | HTMLElement) => {
      target.removeEventListener("keydown", onKey as EventListener)
      target.removeEventListener("click", onClick as EventListener)
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
        setChapter(e.detail.tocItem?.label ?? "")
        syncSectionBar()
      })
      view.addEventListener("load", (e) => listen(e.detail.doc))
      container.append(view)

      await view.open(data.file)
      if (cancelled || !view.renderer) return
      rendererRef.current = view.renderer
      view.renderer.addEventListener("scroll", syncSectionBar)
      // Before `init`, so the first page is laid out once, with the right flow and margins.
      const { appearance, bookZoom, sectionBar, inset } = latest.current
      const layout = readerLayout(measure(container), appearance, {
        clipScrollbar: sectionBar,
        inset,
      })
      applyAppearance(view, { ...appearance, zoom: bookZoom }, layout)

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
    applyAppearance(
      view,
      { flow, scheme, zoom: bookZoom },
      { marginLeft, marginRight, gapPercent, pageMarginRight },
    )
    syncSectionBar()
    // syncSectionBar only reads refs.
  }, [flow, scheme, bookZoom, marginLeft, marginRight, gapPercent, pageMarginRight, size.width])

  const go = (side: Direction) => viewRef.current && void turn(viewRef.current, side, unit)
  // Turning from the bar keeps it up, for turning again; the floating buttons are reading.
  const goAndHide = (side: Direction) => {
    go(side)
    onBarsChange(false)
  }

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
    <div className="relative h-full" style={{ background: readerBackground(scheme) }}>
      <div
        ref={containerRef}
        className="size-full overflow-hidden"
        style={{
          paddingTop: layout.paddingTop + (sectionBar ? LINE : 0),
          paddingBottom: layout.paddingBottom + LINE,
        }}
      />
      {book.isPending && (
        <LoaderCircle className="absolute inset-0 m-auto size-8 animate-spin text-muted-foreground" />
      )}
      {floating && (
        <>
          <PageButton
            side="left"
            unit={unit}
            scheme={scheme}
            onClick={() => goAndHide("left")}
            className="absolute top-1/2 left-4 -translate-y-1/2"
          />
          <PageButton
            side="right"
            unit={unit}
            scheme={scheme}
            onClick={() => goAndHide("right")}
            className="absolute top-1/2 right-4 -translate-y-1/2"
          />
        </>
      )}
      {sectionBar && (
        // Sits under the header (h-12) while it shows, at the top edge while it's hidden.
        <div className={cn("absolute inset-x-0 top-0 z-10", BAR_MOTION, bars && "translate-y-12")}>
          <ProgressBar label="Position in this section" fillRef={sectionBarRef} variant="header" />
        </div>
      )}
      <div
        className={cn(
          // Above the drawer's edge-swipe strips, like the header.
          "absolute inset-x-0 bottom-0 z-20 flex flex-col",
          BAR_MOTION,
          !bars && "translate-y-12",
        )}
      >
        <ProgressBar label="Position in the book" fraction={bookFraction} variant="header" />
        <footer className="flex h-12 items-center gap-2 bg-background px-2" inert={!bars}>
          <BarButton side="left" unit={unit} onClick={() => go("left")} />
          <p className="min-w-0 flex-1 truncate text-center text-sm">{chapter}</p>
          <p className="text-sm text-muted-foreground tabular-nums">
            {Math.round(bookFraction * 100)}%
          </p>
          <ZoomControl zoom={zoom} onZoom={setZoom} bars={bars} />
          <BarButton side="right" unit={unit} onClick={() => go("right")} />
        </footer>
      </div>
    </div>
  )
}

/** Matches the header's slide in `SlideDrawer`. */
const BAR_MOTION = "transition-transform duration-200 ease-out motion-reduce:transition-none"

const label = (side: Direction, unit: "page" | "section") =>
  `${side === "left" ? "Previous" : "Next"} ${unit}`

/** − / + in 10% steps, and back to 100%. Relative to the interface scale. */
function ZoomControl({
  zoom,
  onZoom,
  bars,
}: {
  zoom: number
  onZoom: (zoom: number) => void
  bars: boolean
}) {
  const [open, setOpen] = useState(false)
  return (
    // Taps in the book's iframes don't reach the popup's outside-click check, so it closes
    // with the bars instead.
    <Popover open={open && bars} onOpenChange={setOpen}>
      <PopoverTrigger render={<Button variant="ghost" size="icon" aria-label="Zoom" />}>
        <ALargeSmall />
      </PopoverTrigger>
      <PopoverContent side="top" align="end" className="w-auto flex-row items-center gap-1 p-1">
        <Button
          variant="ghost"
          size="icon"
          aria-label="Zoom out"
          disabled={zoom === ZOOM_STEPS[0]}
          onClick={() => onZoom(stepZoom(zoom, -1))}
        >
          <Minus />
        </Button>
        <span className="w-12 text-center tabular-nums">{Math.round(zoom * 100)}%</span>
        <Button
          variant="ghost"
          size="icon"
          aria-label="Zoom in"
          disabled={zoom === ZOOM_STEPS.at(-1)}
          onClick={() => onZoom(stepZoom(zoom, 1))}
        >
          <Plus />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          aria-label="Reset zoom"
          disabled={zoom === 1}
          onClick={() => onZoom(1)}
        >
          <RotateCcw />
        </Button>
      </PopoverContent>
    </Popover>
  )
}

function BarButton({ side, unit, onClick }: Omit<PageButtonProps, "scheme" | "className">) {
  return (
    <Button variant="ghost" size="icon" onClick={onClick} aria-label={label(side, unit)}>
      {side === "left" ? <ChevronLeft /> : <ChevronRight />}
    </Button>
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
      aria-label={label(side, unit)}
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
