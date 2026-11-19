import { ActionRow } from "@/components/settings/ActionRow"
import { ChoiceSelect } from "@/components/settings/ChoiceSelect"

/** A setting's label and description beside a `Select` of its choices. */
export function ChoiceRow<T extends string | number>({
  label,
  description,
  choices,
  value,
  disabled = false,
  onChange,
}: {
  label: string
  description: string
  choices: { value: T; label: string }[]
  value: T
  disabled?: boolean
  onChange: (value: T) => void
}) {
  return (
    <ActionRow title={label} description={description} className={disabled ? "opacity-60" : undefined}>
      <ChoiceSelect
        choices={choices}
        value={value}
        disabled={disabled}
        label={label}
        className="w-full shrink-0 sm:w-44"
        onChange={onChange}
      />
    </ActionRow>
  )
}
