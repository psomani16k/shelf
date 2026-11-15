import { ALargeSmall, Minus, Plus, RotateCcw } from "lucide-react"
import { type ReactNode, useState } from "react"
import { READER_FLOW, READER_THEME } from "@/components/settings/sections"
import { ChoiceSelect } from "@/components/settings/ChoiceSelect"
import { Button } from "@/components/ui/button"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { stepZoom, ZOOM_STEPS } from "@/lib/readerZoom"
import { useLibraryConfig, useSetConfig } from "@/lib/useAppearance"

/** The bottom bar's *Aa* popup: zoom (this device), theme and layout (the user's config). */
export function ReaderQuickSettings({
  zoom,
  onZoom,
  bars,
  onOpenSettings,
}: {
  zoom: number
  onZoom: (zoom: number) => void
  bars: boolean
  onOpenSettings: () => void
}) {
  const [open, setOpen] = useState(false)
  const config = useLibraryConfig().data
  const save = useSetConfig()

  return (
    // Taps in the book's iframes don't reach the popup's outside-click check, so it closes
    // with the bars instead.
    <Popover open={open && bars} onOpenChange={setOpen}>
      <PopoverTrigger render={<Button variant="ghost" size="icon" aria-label="Reading settings" />}>
        <ALargeSmall />
      </PopoverTrigger>
      <PopoverContent side="top" align="end">
        <Row label="Zoom">
          <div className="flex items-center">
            <Button
              variant="ghost"
              size="icon"
              aria-label="Zoom out"
              disabled={zoom === ZOOM_STEPS[0]}
              onClick={() => onZoom(stepZoom(zoom, -1))}
            >
              <Minus />
            </Button>
            <span className="w-12 text-center tabular-nums">{Math.round(zoom * 100)}%</span>
            <Button
              variant="ghost"
              size="icon"
              aria-label="Zoom in"
              disabled={zoom === ZOOM_STEPS.at(-1)}
              onClick={() => onZoom(stepZoom(zoom, 1))}
            >
              <Plus />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              aria-label="Reset zoom"
              disabled={zoom === 1}
              onClick={() => onZoom(1)}
            >
              <RotateCcw />
            </Button>
          </div>
        </Row>
        {config &&
          [READER_THEME, READER_FLOW].map((setting) => (
            <Row key={setting.id} label={setting.label}>
              <ChoiceSelect
                choices={setting.choices}
                value={setting.read(config) ?? 0}
                disabled={false}
                label={setting.label}
                className="w-40"
                onChange={(value) => save.mutate(setting.patch(value))}
              />
            </Row>
          ))}
        {save.isError && <p className="text-destructive">{String(save.error)}</p>}
        <Button
          variant="ghost"
          className="self-start"
          onClick={() => {
            setOpen(false)
            onOpenSettings()
          }}
        >
          More settings
        </Button>
      </PopoverContent>
    </Popover>
  )
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-2">
      <span className="pl-1">{label}</span>
      {children}
    </div>
  )
}
