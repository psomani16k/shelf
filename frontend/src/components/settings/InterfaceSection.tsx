import { ChoiceRow } from "@/components/settings/ChoiceRow"
import { IS_TAURI } from "@/lib/platform"
import { UI_SCALE_CHOICES, useSaveUiScale, useUiScale } from "@/lib/uiScale"

const CHOICES = UI_SCALE_CHOICES.map((p) => ({ value: p, label: `${p}%` }))

/** App only, below the library's interface settings: the ones kept on this device. */
export function InterfaceSection() {
  return IS_TAURI ? <DeviceSettings /> : null
}

function DeviceSettings() {
  const scale = useUiScale()
  const save = useSaveUiScale()
  return (
    <section aria-label="This device" className="mt-8">
      <h3 className="mb-2 px-1 font-medium">This device</h3>
      {save.isError && <p className="mb-4 px-1 text-destructive">{String(save.error)}</p>}
      <div className="divide-y rounded-lg border px-4">
        <ChoiceRow
          label="Scale"
          description="Size of text and controls in the app. Books zoom on top of this."
          choices={CHOICES}
          value={Math.round(scale * 100)}
          onChange={(percent) => save.mutate(percent)}
        />
      </div>
    </section>
  )
}
