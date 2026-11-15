import type { ReactNode } from "react"

/** A card of rows under a heading. `label` names it, and is the heading unless `heading` is set. */
export function SettingsGroup({
  label,
  heading = label,
  className,
  list = false,
  notice,
  footer,
  children,
}: {
  label?: string
  heading?: string
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
      {heading && <h3 className="mb-2 px-1 font-medium">{heading}</h3>}
      {notice}
      <Card className="divide-y rounded-lg border px-4">{children}</Card>
      {footer}
    </section>
  )
}
