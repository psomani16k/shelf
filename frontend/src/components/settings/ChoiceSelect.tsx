import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

/** A `Select` of labelled values. `label` is its accessible name. */
export function ChoiceSelect<T extends string | number>({
  choices,
  value,
  disabled,
  label,
  className,
  onChange,
}: {
  choices: { value: T; label: string }[]
  value: T
  disabled: boolean
  label: string
  /** The trigger's classes, e.g. its width. */
  className: string
  onChange: (value: T) => void
}) {
  return (
    <Select
      items={choices}
      value={value}
      disabled={disabled}
      onValueChange={(next) => {
        if (next !== null) onChange(next)
      }}
    >
      <SelectTrigger className={className} aria-label={label}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {choices.map((choice) => (
          <SelectItem key={choice.value} value={choice.value}>
            {choice.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
