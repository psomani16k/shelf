import type { Ref } from "react"

type Props = {
  label: string
  /** 0–1. Leave out when the fill is driven through `fillRef` instead. */
  fraction?: number
  /** The fill element, for updating `transform: scaleX(…)` on every frame without a render. */
  fillRef?: Ref<HTMLDivElement>
}

/** A 3px position bar. */
export function ProgressBar({ label, fraction = 0, fillRef }: Props) {
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={1}
      aria-valuenow={fraction}
      className="h-[3px] w-full shrink-0 bg-border"
    >
      <div
        ref={fillRef}
        className="h-full origin-left bg-primary"
        style={{ transform: `scaleX(${fraction})` }}
      />
    </div>
  )
}
