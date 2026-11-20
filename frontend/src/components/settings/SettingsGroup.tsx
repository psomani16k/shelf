import type { ReactNode } from "react"

/** A group's heading and the line explaining it, above its card. */
export function GroupHeading({
  heading,
  description,
}: {
  heading?: string
  description?: ReactNode
}) {
  if (!heading && !description) return null
  return (
    <div className="mb-2 px-1">
      {heading && <h3 className="font-medium">{heading}</h3>}
      {description && <p className="text-sm text-muted-foreground">{description}</p>}
    </div>
  )
}

/** A card of rows under a heading. `label` names it, and is the heading unless `heading` is set. */
export function SettingsGroup({
  label,
  heading = label,
  description,
  className,
  list = false,
  notice,
  footer,
  children,
}: {
  label?: string
  heading?: string
  description?: ReactNode
  className?: string
  /** Renders the card as a `<ul>` of `<li>` rows. */
  list?: boolean
  /** Between the heading and the card. */
  notice?: ReactNode
  /** Under the card. */
  footer?: ReactNode
  children: ReactNode
}) {
  const Card = list ? "ul" : "div"
  return (
    <section aria-label={label} className={className}>
      <GroupHeading heading={heading} description={description} />
      {notice}
      <Card className="divide-y rounded-lg border px-4">{children}</Card>
      {footer}
    </section>
  )
}
