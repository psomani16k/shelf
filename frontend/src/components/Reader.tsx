import { useQuery } from "@tanstack/react-query"
import type { Renderer, TocItem, View } from "foliate-js/view.js"
import { ChevronLeft, ChevronRight, LoaderCircle } from "lucide-react"
import {
  type Ref,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { getBookFile, getProgress } from "@/api"
import { setSelectionMenu } from "@/api/app"
import { CenteredMessage } from "@/components/CenteredMessage"
import { DefinitionSheet } from "@/components/DefinitionSheet"
import { HighlightToolbar } from "@/components/HighlightToolbar"
import { ProgressBar } from "@/components/ProgressBar"
import { ReaderQuickSettings } from "@/components/ReaderQuickSettings"
import { Button } from "@/components/ui/button"
import { type Annotation, HighlightColor_Value } from "@/gen/bookkeeperpb/annotation_pb"
import { type Appearance, readerLayout, type Scheme } from "@/lib/appearance"
import { copyText } from "@/lib/clipboard"
import { isDefinable, normalizeTerm } from "@/lib/dictionary"
import { useKeepScreenOn } from "@/lib/keepScreenOn"
import { applyAppearance, sectionFraction, turn } from "@/lib/foliate"
import { type Direction, swipeDirection, wheelTurner } from "@/lib/gestures"
import {
  clampChars,
  drawHighlight,
  excerpt,
  highlightColor,
  highlightStyle,
  MAX_CHAPTER_CHARS,
  placeToolbar,
} from "@/lib/highlights"
import { log } from "@/lib/log"
import { IS_ANDROID } from "@/lib/platform"
import { progressSaver } from "@/lib/progressSaver"
import { usePinchZoom } from "@/lib/pinchZoom"
import { openBookKey } from "@/lib/queryKeys"
import { applyLook, type ThemeLook } from "@/lib/readerTheme"
import { useReaderZoom } from "@/lib/readerZoom"
import { useUiScale } from "@/lib/uiScale"
import { useLatest } from "@/lib/useLatest"
import {
  useAddHighlight,
  useDeleteHighlight,
  useHighlights,
  useUpdateHighlight,
} from "@/lib/useHighlights"
import { useMediaQuery } from "@/lib/useMediaQuery"
import { useSearchWeb } from "@/lib/webSearch"
import { cn } from "cn"

export type ReaderHandle = { goTo: (href: string) => void }

type Props = {
  id: string
  /** While true (e.g. a drawer is open), keys, swipes and the wheel are ignored. */
  paused: boolean
  appearance: Appearance["reader"]
  /** The reader theme, applied to each page without laying the book out again. */
  look: ThemeLook
  onOpen: (book: { title: string; toc: TocItem[] }) => void
  /** The `id` of the TOC entry the reader is in, as pages turn. */
  onLocation: (tocId: number | undefined) => void
  onOpenLibrary: () => void
  /** Settings → Reader, from the *Aa* popup. */
  onOpenSettings: () => void
  /** Opens the note editor for a highlight. */
  onEditNote: (annotationId: string) => void
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

/** The header's height (h-12) at 100% interface scale, kept clear by the toolbar while it shows. */
const HEADER = 48

/** ms a selection must stay still before the toolbar shows: Android's handles fire constantly. */
const SELECTION_SETTLE = 300

/** Text selected in the book, not highlighted yet. */
type Selected = { range: Range; cfi: string; text: string; chapter: string }

/** What the toolbar is for: a fresh selection, or a highlight tapped (or selected exactly). */
type ToolbarTarget = ({ kind: "new" } & Selected) | { kind: "edit"; id: string; range: Range }

/** Where `range` is in the wrapper, or `null` when none of it is on screen. */
function rangeBox(range: Range, wrapper: HTMLElement) {
  const frame = range.startContainer.ownerDocument?.defaultView?.frameElement
  const rect = range.getBoundingClientRect()
  if (!frame || (rect.width === 0 && rect.height === 0)) return null
  const f = frame.getBoundingClientRect()
  const w = wrapper.getBoundingClientRect()
  const box = {
    left: rect.left + f.left - w.left,
    right: rect.right + f.left - w.left,
    top: rect.top + f.top - w.top,
    bottom: rect.bottom + f.top - w.top,
  }
  const visible = box.right > 0 && box.left < w.width && box.bottom > 0 && box.top < w.height
  return visible ? box : null
}

/** Draws (or redraws) the highlight at `cfi`, if its section is loaded. */
function draw(view: View, cfi: string) {
  view.addAnnotation({ value: cfi }).catch((e: unknown) => {
    log.warn("couldn't draw a highlight", { cfi, error: String(e) })
  })
}

const deselect = (range: Range) =>
  range.startContainer.ownerDocument?.getSelection()?.removeAllRanges()

/** A swipe to the left reveals what's on the right, like the right arrow. */
const opposite = (d: Direction): Direction => (d === "left" ? "right" : "left")

/** Opens the book at its saved position, turns pages on input, and saves the position. */
export function Reader({
  id,
  paused,
  appearance,
  look,
  onOpen,
  onLocation,
  onOpenLibrary,
  onOpenSettings,
  onEditNote,
  bars,
  onBarsChange,
  ref,
}: Props) {
  const wrapperRef = useRef<HTMLDivElement>(null)
  const containerRef = useRef<HTMLDivElement>(null)
  const sectionBarRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<View | null>(null)
  const rendererRef = useRef<Renderer | null>(null)
  const [openError, setOpenError] = useState<unknown>(null)
  const [size, setSize] = useState({ width: 0, height: 0 })
  const [bookFraction, setBookFraction] = useState(0)
  const [chapter, setChapter] = useState("")
  const [zoom, setZoom] = useReaderZoom()
  const [quickSettings, setQuickSettings] = useState(false)
  // A new session starts the sheet's history over; the term stays while it slides away.
  const [define, setDefine] = useState({ term: "", session: 0 })
  const [defining, setDefining] = useState(false)
  const [bookLanguage, setBookLanguage] = useState<string>()
  const searchWeb = useSearchWeb()
  const uiScale = useUiScale()
  const bookZoom = Math.round(uiScale * zoom * 100) / 100
  const coarse = useMediaQuery("(pointer: coarse)")

  const highlights = useHighlights(id)
  const list = highlights.data
  const byCfi = useMemo(() => new Map(list?.map((a) => [a.cfi, a])), [list])
  const add = useAddHighlight(id)
  const update = useUpdateHighlight(id)
  const remove = useDeleteHighlight(id)
  const [target, setTarget] = useState<ToolbarTarget | null>(null)
  const [actionError, setActionError] = useState<unknown>(null)
  const toolbarRef = useRef<HTMLDivElement>(null)
  const lastColor = useRef(HighlightColor_Value.YELLOW)
  // What each CFI was last drawn as, to redraw only what changed.
  const drawn = useRef(new Map<string, Annotation>())
  const drawnScheme = useRef(look.scheme)
  const [opened, setOpened] = useState(false)

  const editing = target?.kind === "edit" ? list?.find((a) => a.id === target.id) : undefined
  // A highlight deleted from the list closes its toolbar.
  const shown = target?.kind === "new" || editing ? target : null

  // Drawers and settings cover the book.
  const [wasPaused, setWasPaused] = useState(paused)
  if (paused !== wasPaused) {
    setWasPaused(paused)
    if (paused) setTarget(null)
  }

  /** Moves the toolbar next to its range; `false` when the range is off the page. */
  const place = () => {
    const wrapper = wrapperRef.current
    const toolbar = toolbarRef.current
    const box = shown && wrapper ? rangeBox(shown.range, wrapper) : null
    if (!wrapper || !toolbar) return false
    toolbar.style.visibility = box ? "" : "hidden"
    if (!box) return false
    const page = { width: wrapper.offsetWidth, height: wrapper.offsetHeight }
    const own = { width: toolbar.offsetWidth, height: toolbar.offsetHeight }
    const { left, top } = placeToolbar(box, page, own, bars ? HEADER * uiScale : 0)
    toolbar.style.left = `${left}px`
    toolbar.style.top = `${top}px`
    return true
  }

  const openToolbar = (next: ToolbarTarget) => {
    add.reset()
    update.reset()
    remove.reset()
    setActionError(null)
    setTarget(next)
  }

  const { flow } = appearance
  const unit = flow === "scrolled" ? "section" : "page"
  const sectionBar = flow === "scrolled"
  const floating = !coarse
  const inset = floating ? Math.round(BUTTON_INSET * uiScale) : 0
  const layout = readerLayout(size, appearance, { clipScrollbar: sectionBar, inset })

  usePinchZoom(!paused && !quickSettings && !defining)
  const stayAwake = useKeepScreenOn(!paused)

  const latest = useLatest({
    // Taps and keys behind the *Aa* drawer or the dictionary aren't reading.
    paused: paused || quickSettings || defining,
    appearance,
    look,
    bookZoom,
    sectionBar,
    inset,
    onOpen,
    onLocation,
    bars,
    onBarsChange,
    byCfi,
    list,
    toolbarOpen: shown !== null,
    stayAwake,
    onMoved: () => {
      // A tapped highlight turned off the page takes its toolbar with it; a selection waits.
      if (!place() && shown?.kind === "edit") setTarget(null)
    },
    closeToolbar: () => setTarget(null),
    onSelected: (selected: Selected | null) => {
      if (!selected) {
        setTarget((t) => (t?.kind === "new" ? null : t))
        return
      }
      // Every change would fail too.
      if (highlights.isError) return
      const existing = byCfi.get(selected.cfi)
      openToolbar(
        existing
          ? { kind: "edit", id: existing.id, range: selected.range }
          : { kind: "new", ...selected },
      )
    },
    onShowHighlight: (cfi: string, range: Range) => {
      const highlight = byCfi.get(cfi)
      if (highlight) openToolbar({ kind: "edit", id: highlight.id, range })
    },
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
    queryKey: openBookKey(id),
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
      latest.current.stayAwake()
      if (e.key === "Escape" && latest.current.toolbarOpen) {
        latest.current.closeToolbar()
        return
      }
      const target = e.target as Element | null
      // An open dropdown's list takes the arrows itself.
      if (target?.closest?.('[role="listbox"]')) return
      // Space or an arrow on a focused button isn't reading.
      const control = target?.closest?.("button, input, select, textarea")
      if (READING_KEYS.has(e.key) && !control) hideBars()
      if (e.key === "ArrowLeft") void turn(view, "left", unit())
      if (e.key === "ArrowRight") void turn(view, "right", unit())
    }

    const onClick = (e: MouseEvent) => {
      if (latest.current.paused || e.defaultPrevented) return
      latest.current.stayAwake()
      // Iframe elements aren't `instanceof Element` here, hence the optional call.
      if ((e.target as Element | null)?.closest?.("a[href]")) return
      // The end of a text selection isn't a tap.
      if (e.view?.getSelection()?.isCollapsed === false) return
      // foliate's hit test for highlights runs after this listener (it's added after `load`), and
      // cancels this if the tap was on one. Not a microtask: those run between listeners.
      clearTimeout(tap)
      tap = setTimeout(() => {
        if (latest.current.toolbarOpen) latest.current.closeToolbar()
        else latest.current.onBarsChange(!latest.current.bars)
      }, 0)
    }
    let tap: ReturnType<typeof setTimeout> | undefined

    /** Finds what's selected in a section once the selection stops changing. */
    const watchSelection = (doc: Document, index: number) => {
      let settle: ReturnType<typeof setTimeout> | undefined
      let mouseDown = false
      const check = () => {
        if (!view || latest.current.paused) return
        const selection = doc.getSelection()
        const range = selection && !selection.isCollapsed ? selection.getRangeAt(0) : null
        const text = range ? excerpt(range.toString()) : ""
        if (!range || !text) return latest.current.onSelected(null)
        const label = view.getProgressOf(index, range).tocItem?.label ?? ""
        latest.current.onSelected({
          range,
          cfi: view.getCFI(index, range),
          text,
          chapter: clampChars(label.trim(), MAX_CHAPTER_CHARS),
        })
      }
      doc.addEventListener("pointerdown", (e) => {
        if (e.pointerType === "mouse") mouseDown = true
      })
      doc.addEventListener("pointerup", (e) => {
        if (e.pointerType !== "mouse") return
        mouseDown = false
        check()
      })
      doc.addEventListener("selectionchange", () => {
        clearTimeout(settle)
        // A mouse drag is checked when the button comes up.
        if (!mouseDown) settle = setTimeout(check, SELECTION_SETTLE)
      })
    }

    // Each CFI's section, for drawing a section's highlights when it loads.
    const sections = new Map<string, number>()
    const sectionOf = (cfi: string) => {
      let index = sections.get(cfi)
      if (index === undefined) {
        try {
          index = view?.resolveCFI(cfi).index ?? -1
        } catch {
          index = -1
        }
        sections.set(cfi, index)
      }
      return index
    }

    const moved = () => {
      if (latest.current.toolbarOpen) latest.current.onMoved()
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

    const listeners: [string, EventListener, AddEventListenerOptions?][] = [
      ["keydown", onKey as EventListener],
      ["click", onClick as EventListener],
      ["touchstart", onTouchStart as EventListener, { passive: true }],
      ["touchend", onTouchEnd as EventListener, { passive: true }],
      ["wheel", onWheel as EventListener, { passive: false }],
    ]
    /** Sections are iframes, so each document needs its own listeners. */
    const listen = (target: Document | HTMLElement) => {
      for (const [type, listener, options] of listeners) {
        target.addEventListener(type, listener, options)
      }
    }
    const unlisten = (target: Document | HTMLElement) => {
      for (const [type, listener] of listeners) target.removeEventListener(type, listener)
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
      for (const [name, value] of Object.entries(highlightStyle(latest.current.look.scheme))) {
        view.style.setProperty(name, value)
      }
      view.addEventListener("relocate", (e) => {
        latest.current.stayAwake()
        moved()
        saver.schedule(e.detail.cfi)
        latest.current.onLocation(e.detail.tocItem?.id)
        setBookFraction(e.detail.fraction ?? 0)
        setChapter(e.detail.tocItem?.label ?? "")
        syncSectionBar()
      })
      view.addEventListener("load", (e) => {
        listen(e.detail.doc)
        watchSelection(e.detail.doc, e.detail.index)
        applyLook(e.detail.doc, latest.current.look)
      })
      view.addEventListener("create-overlay", (e) => {
        for (const highlight of latest.current.list ?? []) {
          if (sectionOf(highlight.cfi) !== e.detail.index) continue
          drawn.current.set(highlight.cfi, highlight)
          if (view) draw(view, highlight.cfi)
        }
      })
      view.addEventListener("draw-annotation", (e) => {
        const highlight = latest.current.byCfi.get(e.detail.annotation.value)
        if (!highlight) return
        e.detail.draw(drawHighlight, {
          color: highlightColor(highlight.color).page[latest.current.look.scheme],
          note: highlight.note !== "",
        })
      })
      view.addEventListener("show-annotation", (e) => {
        clearTimeout(tap)
        latest.current.onShowHighlight(e.detail.value, e.detail.range)
      })
      container.append(view)

      await view.open(data.file)
      if (cancelled || !view.renderer) return
      rendererRef.current = view.renderer
      view.renderer.addEventListener("scroll", syncSectionBar)
      view.renderer.addEventListener("scroll", moved)
      // Before `init`, so the first page is laid out once, with the right flow and margins.
      const { appearance, bookZoom, sectionBar, inset } = latest.current
      const layout = readerLayout(measure(container), appearance, {
        clipScrollbar: sectionBar,
        inset,
      })
      applyAppearance(view, { flow: appearance.flow, zoom: bookZoom }, layout)

      const title = view.book?.metadata?.title
      setBookLanguage([view.book?.metadata?.language ?? []].flat()[0])
      latest.current.onOpen({
        title: typeof title === "string" ? title : (Object.values(title ?? {})[0] ?? ""),
        toc: view.book?.toc ?? [],
      })
      setOpened(true)
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
      clearTimeout(tap)
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
  }, [id, book.data, latest])

  const { marginLeft, marginRight, gapPercent, pageMarginRight } = layout
  useEffect(() => {
    // Any opened view, even one still running `init`, which `viewRef` doesn't hold yet.
    const view = containerRef.current?.querySelector("foliate-view")
    if (!view || size.width === 0) return
    applyAppearance(
      view,
      { flow, zoom: bookZoom },
      { marginLeft, marginRight, gapPercent, pageMarginRight },
    )
    syncSectionBar()
    // syncSectionBar only reads refs.
  }, [flow, bookZoom, marginLeft, marginRight, gapPercent, pageMarginRight, size.width])

  // Pages loaded later get the look from the `load` listener.
  useEffect(() => {
    const view = containerRef.current?.querySelector("foliate-view")
    for (const { doc } of view?.renderer?.getContents() ?? []) applyLook(doc, look)
    for (const [name, value] of Object.entries(highlightStyle(look.scheme))) {
      view?.style.setProperty(name, value)
    }
  }, [look])

  // Keeps the drawing in step with the cache; sections loaded later draw in `create-overlay`.
  useEffect(() => {
    const view = containerRef.current?.querySelector("foliate-view")
    if (!opened || !view || !list) return
    const now = new Map(list.map((a) => [a.cfi, a]))
    const all = drawnScheme.current !== look.scheme
    drawnScheme.current = look.scheme
    for (const cfi of [...drawn.current.keys()]) {
      if (now.has(cfi)) continue
      drawn.current.delete(cfi)
      view.deleteAnnotation({ value: cfi }).catch(() => {})
    }
    for (const [cfi, highlight] of now) {
      const before = drawn.current.get(cfi)
      drawn.current.set(cfi, highlight)
      const changed =
        !before ||
        before.color !== highlight.color ||
        (before.note === "") !== (highlight.note === "")
      if (all || changed) draw(view, cfi)
    }
  }, [opened, list, look.scheme])

  // Before paint, so a new toolbar never shows in the corner. It only measures while one shows.
  useLayoutEffect(() => void place())

  // Only while reading: drawers, settings and the dictionary have text fields that need Android's
  // Copy and Paste.
  const menuShown = paused || defining
  useEffect(() => {
    if (!IS_ANDROID) return
    setSelectionMenu(menuShown).catch((e: unknown) => {
      log.warn("couldn't switch the selection menu", { error: String(e) })
    })
  }, [menuShown])
  useEffect(
    () => () => {
      if (IS_ANDROID) setSelectionMenu(true).catch(() => {})
    },
    [],
  )

  const pickColor = (color: HighlightColor_Value) => {
    if (!shown) return
    lastColor.current = color
    if (shown.kind === "edit") {
      if (editing && editing.color !== color) update.mutate({ id: shown.id, color })
      return
    }
    const { range, cfi, text, chapter } = shown
    add.mutate(
      { cfi, text, chapter, note: "", color },
      {
        onSuccess: () => {
          deselect(range)
          setTarget(null)
        },
      },
    )
  }

  const editNote = () => {
    if (!shown) return
    if (shown.kind === "edit") {
      deselect(shown.range)
      setTarget(null)
      onEditNote(shown.id)
      return
    }
    const { range, cfi, text, chapter } = shown
    add.mutate(
      { cfi, text, chapter, note: "", color: lastColor.current },
      {
        onSuccess: (highlight) => {
          deselect(range)
          setTarget(null)
          onEditNote(highlight.id)
        },
      },
    )
  }

  const shownText = shown?.kind === "new" ? shown.text : (editing?.text ?? "")

  const copy = () => {
    if (!shown) return
    copyText(shownText).then(
      () => {
        deselect(shown.range)
        setTarget(null)
      },
      (e: unknown) => setActionError(e),
    )
  }

  const defineIt = () => {
    if (!shown) return
    deselect(shown.range)
    setTarget(null)
    setDefine((d) => ({ term: normalizeTerm(shownText), session: d.session + 1 }))
    setDefining(true)
  }

  const searchSelection = () => {
    if (!shown) return
    searchWeb(shownText).then(
      () => {
        deselect(shown.range)
        setTarget(null)
      },
      (e: unknown) => setActionError(e),
    )
  }

  const go = (side: Direction) => viewRef.current && void turn(viewRef.current, side, unit)
  // Turning from the bar keeps it up, for turning again; the floating buttons are reading.
  const goAndHide = (side: Direction) => {
    go(side)
    onBarsChange(false)
  }

  const error = book.error ?? openError
  if (error) {
    return (
      <CenteredMessage>
        <p className="font-medium">Couldn't open this book</p>
        <p className="max-w-sm text-sm text-destructive">{String(error)}</p>
        <Button onClick={onOpenLibrary}>Open library</Button>
      </CenteredMessage>
    )
  }

  return (
    <div ref={wrapperRef} className="relative h-full" style={{ background: look.page }}>
      <div
        ref={containerRef}
        className="size-full overflow-hidden"
        style={{
          paddingTop: layout.paddingTop + (sectionBar ? LINE : 0),
          paddingBottom: layout.paddingBottom + LINE,
          // The book's pages are light and transparent; a different scheme around the iframe
          // would make browsers paint it opaque.
          colorScheme: "light",
        }}
      />
      {shown && (
        <HighlightToolbar
          key={shown.kind === "edit" ? shown.id : shown.cfi}
          ref={toolbarRef}
          color={editing?.color}
          hasNote={editing ? editing.note !== "" : false}
          pending={add.isPending || update.isPending || remove.isPending}
          error={add.error ?? update.error ?? remove.error ?? actionError}
          onColor={pickColor}
          onNote={editNote}
          onCopy={copy}
          onDefine={isDefinable(shownText) ? defineIt : undefined}
          onSearchWeb={searchSelection}
          onDelete={
            editing
              ? () => remove.mutate(editing.id, { onSuccess: () => setTarget(null) })
              : undefined
          }
        />
      )}
      <DefinitionSheet
        open={defining}
        onClose={() => setDefining(false)}
        term={define.term}
        session={define.session}
        language={bookLanguage}
        onSearchWeb={(text) =>
          void searchWeb(text).catch((e: unknown) => {
            log.warn("couldn't search the web", { error: String(e) })
          })
        }
      />
      {book.isPending && (
        <LoaderCircle className="absolute inset-0 m-auto size-8 animate-spin text-muted-foreground" />
      )}
      {floating && (
        <>
          <PageButton
            side="left"
            unit={unit}
            scheme={look.scheme}
            onClick={() => goAndHide("left")}
            className="absolute top-1/2 left-4 -translate-y-1/2"
          />
          <PageButton
            side="right"
            unit={unit}
            scheme={look.scheme}
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
          <ReaderQuickSettings
            open={quickSettings}
            onOpenChange={setQuickSettings}
            zoom={zoom}
            onZoom={setZoom}
            onOpenSettings={onOpenSettings}
          />
          <BarButton side="right" unit={unit} onClick={() => go("right")} />
        </footer>
      </div>
    </div>
  )
}

/** The bars' slide; `SlideDrawer`'s header uses it too. */
export const BAR_MOTION = "transition-transform duration-200 ease-out motion-reduce:transition-none"

const label = (side: Direction, unit: "page" | "section") =>
  `${side === "left" ? "Previous" : "Next"} ${unit}`

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
