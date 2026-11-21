import { BookA, Copy, Globe, NotebookPen, Trash2 } from "lucide-react"
import { type CSSProperties, type PointerEvent, type ReactNode, type Ref, useState } from "react"
import { InlineConfirm } from "@/components/InlineConfirm"
import type { HighlightColor_Value } from "@/gen/bookkeeperpb/annotation_pb"
import { HIGHLIGHT_COLORS } from "@/lib/highlights"
import { cn } from "cn"

type Props = {
  /** Set for a highlight that exists; unset for a selection that isn't one yet. */
  color?: HighlightColor_Value
  hasNote?: boolean
  pending: boolean
  error: unknown
  onColor: (color: HighlightColor_Value) => void
  onNote: () => void
  onCopy: () => void
  /** Only for a word or short phrase. */
  onDefine?: () => void
  onSearchWeb: () => void
  /** Only for an existing highlight. One with a note asks first. */
  onDelete?: () => void
  style?: CSSProperties
  ref?: Ref<HTMLDivElement>
}

// A press on the toolbar mustn't take focus from the book, which would clear its selection.
const keepSelection = (e: PointerEvent) => e.preventDefault()

/**
 * Floats by the selection or tapped highlight: colours, note, copy, define, search web and, for a
 * highlight, delete.
 */
export function HighlightToolbar({
  color,
  hasNote = false,
  pending,
  error,
  onColor,
  onNote,
  onCopy,
  onDefine,
  onSearchWeb,
  onDelete,
  style,
  ref,
}: Props) {
  const [confirming, setConfirming] = useState(false)

  return (
    <div
      ref={ref}
      role="toolbar"
      aria-label={color === undefined ? "Highlight" : "Edit highlight"}
      onPointerDown={keepSelection}
      className="absolute z-30 flex max-w-[calc(100%-1rem)] flex-col gap-1 rounded-xl bg-popover p-1 text-popover-foreground shadow-md ring-1 ring-foreground/10"
      style={style}
    >
      {confirming && onDelete ? (
        <InlineConfirm
          className="p-2"
          confirmLabel="Delete"
          onConfirm={onDelete}
          onCancel={() => setConfirming(false)}
          pending={pending}
          error={error ? String(error) : undefined}
        >
          Delete this highlight and its note?
        </InlineConfirm>
      ) : (
        <>
          <div className="flex flex-wrap items-center justify-center">
            {HIGHLIGHT_COLORS.map((c) => (
              <button
                key={c.value}
                type="button"
                aria-label={c.label}
                aria-pressed={color === undefined ? undefined : color === c.value}
                disabled={pending}
                onClick={() => onColor(c.value)}
                className="flex size-9 items-center justify-center rounded-lg outline-none hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring/50"
              >
                <span
                  className={cn(
                    "size-5 rounded-full",
                    color === c.value && "ring-2 ring-foreground ring-offset-2 ring-offset-popover",
                  )}
                  style={{ background: c.swatch }}
                />
              </button>
            ))}
            <span className="mx-1 h-6 w-px bg-border" />
            <ToolButton
              label={hasNote ? "Edit note" : "Add note"}
              disabled={pending}
              onClick={onNote}
            >
              <NotebookPen />
            </ToolButton>
            <ToolButton label="Copy" onClick={onCopy}>
              <Copy />
            </ToolButton>
            {onDefine && (
              <ToolButton label="Define" onClick={onDefine}>
                <BookA />
              </ToolButton>
            )}
            <ToolButton label="Search web" onClick={onSearchWeb}>
              <Globe />
            </ToolButton>
            {onDelete && (
              <ToolButton
                label="Delete highlight"
                disabled={pending}
                onClick={() => (hasNote ? setConfirming(true) : onDelete())}
              >
                <Trash2 />
              </ToolButton>
            )}
          </div>
          {error != null && <p className="px-2 pb-1 text-sm text-destructive">{String(error)}</p>}
        </>
      )}
    </div>
  )
}

function ToolButton({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string
  disabled?: boolean
  onClick: () => void
  children: ReactNode
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="flex size-9 items-center justify-center rounded-lg outline-none hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring/50 disabled:opacity-50 [&_svg]:size-4.5"
    >
      {children}
    </button>
  )
}
