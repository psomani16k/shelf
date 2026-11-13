import { ChevronRight } from "lucide-react"

/** `folder` is `/`-separated; `""` is the root, shown as `rootLabel`. */
export function Breadcrumb({
  folder,
  rootLabel = "All books",
  onNavigate,
}: {
  folder: string
  rootLabel?: string
  onNavigate: (folder: string) => void
}) {
  const parts = folder ? folder.split("/") : []
  return (
    <nav className="mt-1 flex flex-wrap items-center gap-1 text-sm text-muted-foreground">
      <button type="button" className="hover:text-foreground" onClick={() => onNavigate("")}>
        {rootLabel}
      </button>
      {parts.map((part, i) => (
        <span key={i} className="flex items-center gap-1">
          <ChevronRight className="size-3.5" />
          <button
            type="button"
            className="hover:text-foreground"
            onClick={() => onNavigate(parts.slice(0, i + 1).join("/"))}
          >
            {part}
          </button>
        </span>
      ))}
    </nav>
  )
}
