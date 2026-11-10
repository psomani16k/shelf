import type { Ref } from "react"
import { cn } from "cn"

type Props = {
  label: string
  /** 0–1. Leave out when the fill is driven through `fillRef` instead. */
  fraction?: number
  /** The fill element, for updating `transform: scaleX(…)` on every frame without a render. */
  fillRef?: Ref<HTMLDivElement>
  /** `header`: the UI's background and a foreground fill, whatever the reader's colours. */
  variant?: "default" | "header"
}

/** A 3px position bar. */
export function ProgressBar({ label, fraction = 0, fillRef, variant = "default" }: Props) {
  const header = variant === "header"
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={1}
      aria-valuenow={fraction}
      className={cn("h-[3px] w-full shrink-0", header ? "bg-background" : "bg-border")}
    >
      <div
        ref={fillRef}
        className={cn("h-full origin-left", header ? "bg-foreground" : "bg-primary")}
        style={{ transform: `scaleX(${fraction})` }}
      />
    </div>
  )
}
