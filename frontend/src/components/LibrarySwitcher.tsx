import { Select } from "@base-ui/react/select"
import { CheckIcon, ChevronsUpDown } from "lucide-react"
import type { LibraryInfo } from "@/api/app"
import { useCurrentLibrary, useLibraries, useSwitchLibrary } from "@/lib/useAppLibrary"
import { cn } from "cn"

/** App only: picks which library is open. Each entry shows its name and where it lives. */
export function LibrarySwitcher({
  side = "bottom",
  className,
}: {
  /** Where the list opens; "top" when the switcher sits at the bottom of the screen. */
  side?: "top" | "bottom"
  className?: string
}) {
  const libraries = useLibraries().data?.libraries ?? []
  const current = useCurrentLibrary()
  const switchTo = useSwitchLibrary()

  if (!current) return null
  return (
    <div className={className}>
      <Select.Root
        value={current.name}
        onValueChange={(name) => {
          if (typeof name === "string" && name !== current.name) switchTo.mutate(name)
        }}
        disabled={switchTo.isPending}
      >
        <Select.Trigger
          aria-label="Library"
          className="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left outline-none hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring/50 disabled:opacity-50"
        >
          <LibraryLabel library={current} />
          <ChevronsUpDown className="size-4 shrink-0 text-muted-foreground" />
        </Select.Trigger>
        <Select.Portal>
          <Select.Positioner side={side} align="start" sideOffset={4} alignItemWithTrigger={false} className="isolate z-50">
            <Select.Popup className="max-h-(--available-height) w-(--anchor-width) min-w-56 overflow-y-auto rounded-lg bg-popover p-1 text-popover-foreground shadow-md ring-1 ring-foreground/10">
              <Select.List>
                {libraries.map((library) => (
                  <Select.Item
                    key={library.name}
                    value={library.name}
                    className="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 outline-none select-none data-highlighted:bg-accent data-highlighted:text-accent-foreground"
                  >
                    <Select.ItemText className="flex min-w-0 flex-1">
                      <LibraryLabel library={library} />
                    </Select.ItemText>
                    <Select.ItemIndicator className="shrink-0">
                      <CheckIcon className="size-4" />
                    </Select.ItemIndicator>
                  </Select.Item>
                ))}
              </Select.List>
            </Select.Popup>
          </Select.Positioner>
        </Select.Portal>
      </Select.Root>
      {switchTo.isError && (
        <p className="mt-1 px-2 text-sm text-destructive">{String(switchTo.error)}</p>
      )}
    </div>
  )
}

/** The name, with the folder path or server URL under it. */
export function LibraryLabel({
  library,
  className,
}: {
  library: LibraryInfo
  className?: string
}) {
  return (
    <span className={cn("flex min-w-0 flex-1 flex-col", className)}>
      <span className="truncate font-medium">{library.name}</span>
      <span className="truncate text-xs text-muted-foreground">{library.location}</span>
    </span>
  )
}
