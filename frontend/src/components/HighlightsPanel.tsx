import { Copy, Ellipsis, LoaderCircle, NotebookPen, Trash2 } from "lucide-react"
import { useEffect, useMemo, useRef, useState } from "react"
import { InlineConfirm } from "@/components/InlineConfirm"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import type { Annotation } from "@/gen/bookkeeperpb/annotation_pb"
import { useBackButton } from "@/lib/backButton"
import { copyText } from "@/lib/clipboard"
import { DATE_TIME } from "@/lib/format"
import {
  byPosition,
  clampChars,
  groupByChapter,
  HIGHLIGHT_COLORS,
  highlightColor,
  MAX_NOTE_CHARS,
} from "@/lib/highlights"
import { useDeleteHighlight, useHighlights, useUpdateHighlight } from "@/lib/useHighlights"
import { cn } from "cn"

type Props = {
  bookId: string
  /** The highlight whose note is being edited, which replaces the list. */
  noteFor: string | null
  onNoteForChange: (id: string | null) => void
  onPick: (cfi: string) => void
}

/** The book's highlights by chapter, in reading order; or the note editor for one of them. */
export function HighlightsPanel({ bookId, noteFor, onNoteForChange, onPick }: Props) {
  const highlights = useHighlights(bookId)
  const [notesOnly, setNotesOnly] = useState(false)
  const [expanded, setExpanded] = useState<string | null>(null)
  const sorted = useMemo(() => byPosition(highlights.data ?? []), [highlights.data])

  const editing = noteFor ? sorted.find((a) => a.id === noteFor) : undefined
  if (editing) {
    return (
      <NoteEditor
        key={editing.id}
        bookId={bookId}
        highlight={editing}
        onDone={() => onNoteForChange(null)}
      />
    )
  }

  if (highlights.isPending) {
    return <LoaderCircle className="m-auto size-6 animate-spin text-muted-foreground" />
  }
  if (highlights.isError) {
    return (
      <div className="mt-8 flex flex-col items-center gap-3 px-4 text-center">
        <p className="font-medium">Couldn't load highlights</p>
        <p className="text-sm text-destructive">{String(highlights.error)}</p>
        <Button onClick={() => void highlights.refetch()}>Try again</Button>
      </div>
    )
  }
  if (sorted.length === 0) {
    return (
      <p className="mt-8 px-4 text-center text-muted-foreground">
        Select text in the book to highlight it.
      </p>
    )
  }

  const withNotes = sorted.filter((a) => a.note !== "")
  const chips = withNotes.length > 0 && withNotes.length < sorted.length
  const groups = groupByChapter(chips && notesOnly ? withNotes : sorted)

  return (
    <div className="flex-1 overflow-y-auto overscroll-contain p-2">
      {chips && (
        <div className="flex gap-2 px-2 pt-1 pb-3">
          <Chip active={!notesOnly} onClick={() => setNotesOnly(false)}>
            All
          </Chip>
          <Chip active={notesOnly} onClick={() => setNotesOnly(true)}>
            With notes
          </Chip>
        </div>
      )}
      {groups.map((group, i) => (
        <section key={`${i}:${group.chapter}`} className="mb-2">
          {group.chapter && (
            <h3 className="px-3 pt-2 pb-1 text-xs font-medium tracking-wide text-muted-foreground uppercase">
              {group.chapter}
            </h3>
          )}
          <ol>
            {group.items.map((highlight) => (
              <Entry
                key={highlight.id}
                bookId={bookId}
                highlight={highlight}
                expanded={expanded === highlight.id}
                onToggle={() => setExpanded((id) => (id === highlight.id ? null : highlight.id))}
                onPick={() => onPick(highlight.cfi)}
                onEditNote={() => onNoteForChange(highlight.id)}
              />
            ))}
          </ol>
        </section>
      ))}
    </div>
  )
}

function Chip({
  active,
  onClick,
  children,
}: {
  active: boolean
  onClick: () => void
  children: string
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        "rounded-full border px-3 py-1 text-sm",
        active ? "border-transparent bg-primary text-primary-foreground" : "hover:bg-muted",
      )}
    >
      {children}
    </button>
  )
}

function Entry({
  bookId,
  highlight,
  expanded,
  onToggle,
  onPick,
  onEditNote,
}: {
  bookId: string
  highlight: Annotation
  expanded: boolean
  onToggle: () => void
  onPick: () => void
  onEditNote: () => void
}) {
  const update = useUpdateHighlight(bookId)
  const remove = useDeleteHighlight(bookId)
  const [confirming, setConfirming] = useState(false)
  const [copyError, setCopyError] = useState<unknown>(null)
  const hasNote = highlight.note !== ""
  const error = update.error ?? remove.error ?? copyError
  const pending = update.isPending || remove.isPending

  const deleteIt = () => remove.mutate(highlight.id)

  return (
    <li>
      <div className="flex items-start">
        <button
          type="button"
          onClick={onPick}
          className="flex min-w-0 flex-1 gap-3 rounded-md px-3 py-2 text-left hover:bg-muted"
        >
          <span
            className="w-[3px] shrink-0 self-stretch rounded-full"
            style={{ background: highlightColor(highlight.color).swatch }}
          />
          <span className="flex min-w-0 flex-1 flex-col gap-1">
            <span className="line-clamp-3 text-sm">{highlight.text}</span>
            {hasNote && (
              <span className="line-clamp-4 text-sm whitespace-pre-wrap text-muted-foreground">
                {highlight.note}
              </span>
            )}
            <span className="text-xs text-muted-foreground">
              {DATE_TIME.format(new Date(highlight.created))}
            </span>
          </span>
        </button>
        <Button
          variant="ghost"
          size="icon"
          aria-label="Highlight actions"
          aria-expanded={expanded}
          onClick={onToggle}
        >
          <Ellipsis />
        </Button>
      </div>
      {expanded &&
        (confirming ? (
          <InlineConfirm
            className="mx-3 mb-2 rounded-md border p-3"
            confirmLabel="Delete"
            onConfirm={deleteIt}
            onCancel={() => setConfirming(false)}
            pending={pending}
            error={error ? String(error) : undefined}
          >
            Delete this highlight and its note?
          </InlineConfirm>
        ) : (
          <div className="mx-3 mb-2 flex flex-col gap-1">
            <div className="flex flex-wrap items-center">
              {HIGHLIGHT_COLORS.map((c) => (
                <button
                  key={c.value}
                  type="button"
                  aria-label={c.label}
                  aria-pressed={highlight.color === c.value}
                  disabled={pending}
                  onClick={() =>
                    highlight.color !== c.value &&
                    update.mutate({ id: highlight.id, color: c.value })
                  }
                  className="flex size-9 items-center justify-center rounded-lg hover:bg-muted"
                >
                  <span
                    className={cn(
                      "size-5 rounded-full",
                      highlight.color === c.value &&
                        "ring-2 ring-foreground ring-offset-2 ring-offset-background",
                    )}
                    style={{ background: c.swatch }}
                  />
                </button>
              ))}
              <span className="mx-1 h-6 w-px bg-border" />
              <Button variant="ghost" size="sm" disabled={pending} onClick={onEditNote}>
                <NotebookPen />
                {hasNote ? "Edit note" : "Add note"}
              </Button>
              <Button
                variant="ghost"
                size="icon"
                aria-label="Copy"
                onClick={() =>
                  copyText(highlight.text).then(() => setCopyError(null), setCopyError)
                }
              >
                <Copy />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                aria-label="Delete highlight"
                disabled={pending}
                onClick={() => (hasNote ? setConfirming(true) : deleteIt())}
              >
                <Trash2 />
              </Button>
            </div>
            {error != null && <p className="text-sm text-destructive">{String(error)}</p>}
          </div>
        ))}
    </li>
  )
}

/** In the drawer, not a bottom sheet, so Android's keyboard can't cover the field. */
function NoteEditor({
  bookId,
  highlight,
  onDone,
}: {
  bookId: string
  highlight: Annotation
  onDone: () => void
}) {
  const [note, setNote] = useState(highlight.note)
  const update = useUpdateHighlight(bookId)
  const fieldRef = useRef<HTMLTextAreaElement>(null)
  useBackButton(onDone)

  useEffect(() => {
    // Scrolling it into view would also scroll the drawer's clipped root.
    fieldRef.current?.focus({ preventScroll: true })
  }, [])

  const save = () =>
    update.mutate(
      { id: highlight.id, note: note.trim() === "" ? "" : note },
      { onSuccess: onDone },
    )

  return (
    <div className="flex flex-1 flex-col gap-3 overflow-y-auto overscroll-contain p-4">
      <blockquote
        className="line-clamp-6 border-l-[3px] pl-3 text-sm text-muted-foreground"
        style={{ borderColor: highlightColor(highlight.color).swatch }}
      >
        {highlight.text}
      </blockquote>
      <Textarea
        ref={fieldRef}
        aria-label="Note"
        placeholder="Your note"
        value={note}
        onChange={(e) => setNote(clampChars(e.target.value, MAX_NOTE_CHARS))}
        className="min-h-32"
      />
      {update.error && <p className="text-sm text-destructive">{String(update.error)}</p>}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onDone} disabled={update.isPending}>
          Cancel
        </Button>
        <Button onClick={save} disabled={update.isPending || note === highlight.note}>
          Save
        </Button>
      </div>
    </div>
  )
}
