import { LibrarySwitcher } from "@/components/LibrarySwitcher"
import { useLibraries } from "@/lib/useAppLibrary"

/** App only: a switcher to another library, when there is one. */
export function OtherLibraries({ width }: { width: "max-w-xs" | "max-w-sm" }) {
  const others = (useLibraries().data?.libraries.length ?? 0) > 1
  if (!others) return null
  return (
    <div className={`flex w-full ${width} flex-col gap-1 text-left`}>
      <p className="text-sm text-muted-foreground">Or open another library:</p>
      <LibrarySwitcher className="rounded-lg border" />
    </div>
  )
}
