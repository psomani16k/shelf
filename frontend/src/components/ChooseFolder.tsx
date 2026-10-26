import { open } from "@tauri-apps/plugin-dialog"
import { FolderOpen } from "lucide-react"
import type { ComponentProps } from "react"
import { pickFolder } from "@/api/app"
import { Button } from "@/components/ui/button"
import { IS_ANDROID } from "@/lib/platform"

type Props = {
  label: string
  onPick: (path: string) => void
  disabled?: boolean
} & Pick<ComponentProps<typeof Button>, "variant" | "size">

/** App only: a native folder picker. Resolves to nothing when the user cancels. */
async function chooseFolder(): Promise<string | null> {
  // The dialog plugin has no folder picker on Android; the app's own plugin wraps the system one.
  if (IS_ANDROID) return pickFolder().catch(() => null)
  return (await open({ directory: true, title: "Choose your library folder" })) ?? null
}

export function ChooseFolder({ label, onPick, disabled, variant, size }: Props) {
  return (
    <Button
      type="button"
      variant={variant}
      size={size}
      disabled={disabled}
      onClick={async () => {
        const dir = await chooseFolder()
        if (dir) onPick(dir)
      }}
    >
      <FolderOpen />
      {label}
    </Button>
  )
}
