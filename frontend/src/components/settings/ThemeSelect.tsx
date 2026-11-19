import { ChoiceRow } from "@/components/settings/ChoiceRow"
import { ChoiceSelect } from "@/components/settings/ChoiceSelect"
import type { ConfigPatch } from "@/components/settings/sections"
import { useReaderTheme, useReaderThemes } from "@/lib/useAppearance"

const LABEL = "Theme"

function useThemeChoices() {
  const themes = useReaderThemes().data?.themes ?? []
  const current = useReaderTheme()
  const choices = themes.map((t) => ({ value: t.id, label: t.spec?.name ?? t.id }))
  return { choices, value: current?.id }
}

/** The reader theme, from the library's list; nothing until the list loads. */
export function ThemeSelect({ save, className }: { save: ConfigPatch; className: string }) {
  const { choices, value } = useThemeChoices()
  if (value === undefined) return null
  return (
    <ChoiceSelect
      choices={choices}
      value={value}
      disabled={false}
      label={LABEL}
      className={className}
      onChange={(theme) => save({ reader: { theme } })}
    />
  )
}

/** Settings → Reader's theme row. */
export function ThemeRow({ save }: { save: ConfigPatch }) {
  const { choices, value } = useThemeChoices()
  if (value === undefined) return null
  return (
    <ChoiceRow
      label={LABEL}
      description="Colours of the book's pages."
      choices={choices}
      value={value}
      onChange={(theme) => save({ reader: { theme } })}
    />
  )
}
