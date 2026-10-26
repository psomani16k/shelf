import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { LibraryBig } from "lucide-react"
import { type ReactNode, useState } from "react"
import { requestStorage, storageGranted } from "@/api/app"
import { CenteredScreen } from "@/components/CenteredScreen"
import { ChooseFolder } from "@/components/ChooseFolder"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { IS_ANDROID } from "@/lib/platform"
import { useAddLibrary } from "@/lib/useAppLibrary"

/** App only: shown until this device has a library to open. */
export function PickLibrary() {
  const add = useAddLibrary()
  const [url, setUrl] = useState("")

  return (
    <CenteredScreen>
      <LibraryBig className="size-10 text-muted-foreground" />
      <h1 className="text-2xl font-semibold">Welcome to Shelf</h1>
      <p className="max-w-sm text-muted-foreground">
        Your books can live in a folder on this device, or on a Shelf server.
      </p>

      <Option
        title="A folder on this device"
        description="Shelf reads the .epub files in it and keeps its own data in a hidden .shelf folder there."
      >
        <LocalFolder
          disabled={add.isPending}
          onPick={(path) => add.mutate({ kind: "local", path })}
        />
      </Option>

      <Option title="A Shelf server" description="The address of a shelf-server you can reach.">
        <form
          className="flex flex-col gap-2"
          onSubmit={(e) => {
            e.preventDefault()
            add.mutate({ kind: "remote", baseUrl: url.trim() })
          }}
        >
          <Input
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="http://nas:8080"
            inputMode="url"
            autoCapitalize="none"
          />
          <Button type="submit" disabled={!url.trim() || add.isPending}>
            Connect
          </Button>
        </form>
      </Option>

      {add.isError && <p className="max-w-sm text-sm text-destructive">{String(add.error)}</p>}
    </CenteredScreen>
  )
}

const STORAGE_KEY = ["storage-granted"]

/** On Android the folder can only be read once the user grants all-files access in settings. */
function LocalFolder({ disabled, onPick }: { disabled: boolean; onPick: (path: string) => void }) {
  const queryClient = useQueryClient()
  const storage = useQuery({
    queryKey: STORAGE_KEY,
    queryFn: storageGranted,
    enabled: IS_ANDROID,
    staleTime: Infinity,
  })
  const grant = useMutation({
    mutationFn: requestStorage,
    onSuccess: (granted) => queryClient.setQueryData(STORAGE_KEY, granted),
  })

  if (!IS_ANDROID || storage.data) {
    return <ChooseFolder label="Choose folder" disabled={disabled} onPick={onPick} />
  }
  return (
    <div className="flex flex-col gap-2">
      <p className="text-sm text-muted-foreground">
        Shelf needs permission to read files on this device. Android asks for it on a settings
        page.
      </p>
      <Button onClick={() => grant.mutate()} disabled={storage.isPending || grant.isPending}>
        Allow access to files
      </Button>
      {grant.data === false && (
        <p className="text-sm text-destructive">Access wasn't granted. Try again.</p>
      )}
    </div>
  )
}

function Option({
  title,
  description,
  children,
}: {
  title: string
  description: string
  children: ReactNode
}) {
  return (
    <section className="flex w-full max-w-sm flex-col gap-3 rounded-lg border p-4 text-left">
      <div>
        <h2 className="font-medium">{title}</h2>
        <p className="text-sm text-muted-foreground">{description}</p>
      </div>
      {children}
    </section>
  )
}
