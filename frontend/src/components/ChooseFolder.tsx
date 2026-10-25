import { open } from "@tauri-apps/plugin-dialog"
import { FolderOpen } from "lucide-react"
import { type ComponentProps, useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { IS_ANDROID } from "@/lib/platform"

type Props = {
  label: string
  onPick: (path: string) => void
  disabled?: boolean
} & Pick<ComponentProps<typeof Button>, "variant" | "size">

/** App only: a folder picker. The dialog plugin has none on Android, so there the path is typed. */
export function ChooseFolder({ label, onPick, disabled, variant, size }: Props) {
  const [path, setPath] = useState("")

  if (IS_ANDROID) {
    return (
      <form
        className="flex flex-col gap-2"
        onSubmit={(e) => {
          e.preventDefault()
          onPick(path)
        }}
      >
        <Input
          value={path}
          onChange={(e) => setPath(e.target.value)}
          placeholder="/storage/emulated/0/Books"
        />
        <Button type="submit" variant={variant} size={size} disabled={!path || disabled}>
          {label}
        </Button>
      </form>
    )
  }

  return (
    <Button
      variant={variant}
      size={size}
      disabled={disabled}
      onClick={async () => {
        const dir = await open({ directory: true, title: "Choose your library folder" })
        if (dir) onPick(dir)
      }}
    >
      <FolderOpen />
      {label}
    </Button>
  )
}
