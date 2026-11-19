import { ALargeSmall, Minus, Plus, RotateCcw } from "lucide-react"
import type { ReactNode } from "react"
import { ChoiceSelect } from "@/components/settings/ChoiceSelect"
import { READER_FLOW, READER_IMAGES } from "@/components/settings/sections"
import { ThemeSelect } from "@/components/settings/ThemeSelect"
import { Button } from "@/components/ui/button"
import { Drawer, DrawerContent, DrawerTitle, DrawerTrigger } from "@/components/ui/drawer"
import { useBackButton } from "@/lib/backButton"
import { stepZoom, ZOOM_STEPS } from "@/lib/readerZoom"
import { useLibraryConfig, useSetConfig } from "@/lib/useAppearance"

/** The bottom bar's *Aa* drawer: zoom (this device), theme, layout and images (the user's config). */
export function ReaderQuickSettings({
  open,
  onOpenChange,
  zoom,
  onZoom,
  onOpenSettings,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  zoom: number
  onZoom: (zoom: number) => void
  onOpenSettings: () => void
}) {
  const config = useLibraryConfig().data
  const save = useSetConfig()
  useBackButton(() => onOpenChange(false), open)

  return (
    <Drawer open={open} onOpenChange={onOpenChange} showSwipeHandle>
      <DrawerTrigger render={<Button variant="ghost" size="icon" aria-label="Reading settings" />}>
        <ALargeSmall />
      </DrawerTrigger>
      {/* No dimming or blur, so theme and zoom changes show on the page behind. */}
      <DrawerContent overlayClassName="bg-transparent supports-backdrop-filter:backdrop-blur-none">
        <div className="mx-auto flex w-full max-w-md flex-col gap-2.5 px-4 pt-2 pb-[calc(1rem+var(--inset-bottom))]">
          <DrawerTitle className="pl-1">Reading settings</DrawerTitle>
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
          <Row label="Theme">
            <ThemeSelect save={save.mutate} className="w-40" />
          </Row>
          {config &&
            [READER_FLOW, READER_IMAGES].map((setting) => (
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
              onOpenChange(false)
              onOpenSettings()
            }}
          >
            More settings
          </Button>
        </div>
      </DrawerContent>
    </Drawer>
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
