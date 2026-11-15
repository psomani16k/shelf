/** A name, and a small badge such as "You" after it. */
export function NameWithBadge({ name, badge }: { name: string; badge?: string | false }) {
  return (
    <p className="flex items-center gap-2 text-sm font-medium">
      <span className="truncate">{name}</span>
      {badge && (
        <span className="shrink-0 rounded-full bg-muted px-2 py-0.5 text-xs font-normal text-muted-foreground">
          {badge}
        </span>
      )}
    </p>
  )
}
