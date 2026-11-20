import { ChevronRight, LoaderCircle, Plus } from "lucide-react"
import { useState } from "react"
import { SettingsGroup } from "@/components/settings/SettingsGroup"
import { useSettingsPage } from "@/components/settings/settingsNav"
import { Button } from "@/components/ui/button"
import { type ThemeEntry, ThemeSource_Value } from "@/gen/apipb/theme_pb"
import { themeLook } from "@/lib/readerTheme"
import { useDeleteTheme, useReaderThemes } from "@/lib/useAppearance"

/** Settings → Reader's way into the Themes page. */
export function ThemesRow() {
  const { open } = useSettingsPage()
  return (
    <button
      type="button"
      onClick={() => open("themes")}
      className="flex w-full items-center gap-6 py-4 text-left"
    >
      <span className="min-w-0 flex-1">
        <span className="block text-sm font-medium">Themes</span>
        <span className="block text-sm text-muted-foreground">
          Make your own, or remove ones you made.
        </span>
      </span>
      <ChevronRight className="size-4 shrink-0 text-muted-foreground" />
    </button>
  )
}

const KIND: Record<ThemeSource_Value, string> = {
  [ThemeSource_Value.BUILT_IN]: "Built-in",
  [ThemeSource_Value.OVERRIDDEN]: "Yours, in place of the built-in",
  [ThemeSource_Value.CUSTOM]: "Yours",
}

/** Every theme the user can pick, with *New theme*, and delete for their own. */
export function ThemesPage() {
  const themes = useReaderThemes()
  const remove = useDeleteTheme()
  const { open } = useSettingsPage()

  if (themes.isPending) {
    return <LoaderCircle className="mx-auto mt-8 size-6 animate-spin text-muted-foreground" />
  }
  if (themes.isError) return <p className="text-destructive">{String(themes.error)}</p>

  return (
    <div className="flex flex-col gap-4">
      {remove.isError && <p className="px-1 text-sm text-destructive">{String(remove.error)}</p>}
      <SettingsGroup
        label="Themes"
        heading=""
        description="A theme recolours the book's pages. Yours are kept in the library, for you."
        list
      >
        <li>
          <button
            type="button"
            onClick={() => open("new-theme")}
            className="flex w-full items-center gap-3 py-3 text-left"
          >
            <span
              aria-hidden
              className="flex size-10 shrink-0 items-center justify-center rounded-md border border-dashed text-muted-foreground"
            >
              <Plus className="size-5" />
            </span>
            <span className="min-w-0 flex-1">
              <span className="block text-sm font-medium">New theme</span>
              <span className="block text-sm text-muted-foreground">
                From scratch, or starting from any theme
              </span>
            </span>
            <ChevronRight className="size-4 shrink-0 text-muted-foreground" />
          </button>
        </li>
        {themes.data.themes.map((theme) => (
          <ThemeItem
            key={theme.id}
            theme={theme}
            busy={remove.isPending}
            onDelete={(name) => remove.mutate(name)}
          />
        ))}
      </SettingsGroup>
    </div>
  )
}

function ThemeItem({
  theme,
  busy,
  onDelete,
}: {
  theme: ThemeEntry
  busy: boolean
  onDelete: (name: string) => void
}) {
  const [confirming, setConfirming] = useState(false)
  const name = theme.spec?.name ?? theme.id
  const look = themeLook(theme.spec, "keep")
  const override = theme.source === ThemeSource_Value.OVERRIDDEN
  const own = override || theme.source === ThemeSource_Value.CUSTOM

  return (
    <li className="flex items-center gap-3 py-3">
      <span
        aria-hidden
        className="flex size-10 shrink-0 items-center justify-center rounded-md border font-serif"
        style={{ background: look.page, color: look.text }}
      >
        Aa
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-medium">{name}</span>
        <span className="block text-sm text-muted-foreground">
          {confirming
            ? override
              ? "Go back to the built-in one?"
              : "Delete it? This can't be undone."
            : KIND[theme.source]}
        </span>
      </span>
      {own &&
        (confirming ? (
          <div className="flex shrink-0 gap-2">
            <Button variant="ghost" onClick={() => setConfirming(false)} disabled={busy}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={() => onDelete(name)} disabled={busy}>
              {busy && <LoaderCircle className="animate-spin" />}
              {override ? "Reset" : "Delete"}
            </Button>
          </div>
        ) : (
          <Button variant="outline" className="shrink-0" onClick={() => setConfirming(true)}>
            {override ? "Reset" : "Delete"}
          </Button>
        ))}
    </li>
  )
}
