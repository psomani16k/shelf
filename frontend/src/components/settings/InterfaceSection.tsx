import { ChoiceRow } from "@/components/settings/ChoiceRow"
import { SettingsGroup } from "@/components/settings/SettingsGroup"
import { useKeepScreenOnSetting } from "@/lib/keepScreenOn"
import { IS_TAURI } from "@/lib/platform"
import { UI_SCALE_CHOICES, useSaveUiScale, useUiScale } from "@/lib/uiScale"

const CHOICES = UI_SCALE_CHOICES.map((p) => ({ value: p, label: `${p}%` }))
const ON_OFF = [
  { value: "on", label: "On" },
  { value: "off", label: "Off" },
]

/** App only, below the library's interface settings: the ones kept on this device. */
export function InterfaceSection() {
  return IS_TAURI ? <DeviceSettings /> : null
}

function DeviceSettings() {
  const scale = useUiScale()
  const save = useSaveUiScale()
  const [screenOn, setScreenOn] = useKeepScreenOnSetting()
  return (
    <SettingsGroup
      label="This device"
      className="mt-8"
      notice={save.isError && <p className="mb-4 px-1 text-destructive">{String(save.error)}</p>}
    >
      <ChoiceRow
        label="Scale"
        description="Size of text and controls in the app. Books zoom on top of this."
        choices={CHOICES}
        value={Math.round(scale * 100)}
        onChange={(percent) => save.mutate(percent)}
      />
      <ChoiceRow
        label="Keep screen on"
        description="While a book is open, until 10 minutes pass without turning a page."
        choices={ON_OFF}
        value={screenOn ? "on" : "off"}
        onChange={(v) => setScreenOn(v === "on")}
      />
    </SettingsGroup>
  )
}
