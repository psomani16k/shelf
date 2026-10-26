import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { type ReactNode, useState } from "react"
import { requestStorage, storageGranted } from "@/api/app"
import { ChooseFolder } from "@/components/ChooseFolder"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { IS_ANDROID } from "@/lib/platform"
import { nameTaken, useAddLibrary, useLibraries } from "@/lib/useAppLibrary"
import { cn } from "cn"

type Kind = "local" | "remote"

/** App only: name a library and point it at a folder or a server. Adding it switches to it. */
export function AddLibraryForm({ onCancel }: { onCancel?: () => void }) {
  const libraries = useLibraries().data?.libraries
  const add = useAddLibrary()
  const [name, setName] = useState("")
  const [kind, setKind] = useState<Kind>("local")
  const [path, setPath] = useState<string | null>(null)
  const [url, setUrl] = useState("")

  const taken = nameTaken(libraries, name)
  const hasSource = kind === "local" ? !!path : !!url.trim()
  const canAdd = !!name.trim() && !taken && hasSource && !add.isPending
  // A sensible name once the source is known; never overwrites one the user typed.
  const suggest = (suggestion: string | undefined) => {
    if (!name.trim() && suggestion && !nameTaken(libraries, suggestion)) setName(suggestion)
  }

  return (
    <form
      className="flex w-full flex-col gap-4 text-left"
      onSubmit={(e) => {
        e.preventDefault()
        if (!canAdd) return
        add.mutate(
          kind === "local" && path
            ? { kind, name: name.trim(), path }
            : { kind: "remote", name: name.trim(), baseUrl: url.trim() },
        )
      }}
    >
      <Field label="Name" error={taken && `There's already a library called ${name.trim()}.`}>
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g. Home books"
          aria-invalid={taken}
          required
        />
      </Field>

      <Field label="Where are the books?">
        <div className="grid grid-cols-2 gap-2">
          <KindButton active={kind === "local"} onClick={() => setKind("local")}>
            A folder on this device
          </KindButton>
          <KindButton active={kind === "remote"} onClick={() => setKind("remote")}>
            A Shelf server
          </KindButton>
        </div>
        {kind === "local" ? (
          <LocalFolder
            path={path}
            onPick={(picked) => {
              setPath(picked)
              suggest(picked.split(/[\\/]/).filter(Boolean).pop())
            }}
          />
        ) : (
          <Input
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onBlur={() => suggest(hostOf(url))}
            placeholder="http://nas:8080"
            inputMode="url"
            autoCapitalize="none"
          />
        )}
      </Field>

      {add.isError && <p className="text-sm text-destructive">{String(add.error)}</p>}
      <div className="flex justify-end gap-2">
        {onCancel && (
          <Button type="button" variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
        )}
        <Button type="submit" disabled={!canAdd}>
          {add.isPending ? "Adding…" : "Add library"}
        </Button>
      </div>
    </form>
  )
}

function hostOf(url: string): string | undefined {
  try {
    return new URL(url.includes("://") ? url : `http://${url}`).hostname || undefined
  } catch {
    return undefined
  }
}

const STORAGE_KEY = ["storage-granted"]

/** On Android the folder can only be read once the user grants all-files access in settings. */
function LocalFolder({ path, onPick }: { path: string | null; onPick: (path: string) => void }) {
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

  if (IS_ANDROID && !storage.data) {
    return (
      <div className="flex flex-col gap-2">
        <p className="text-sm text-muted-foreground">
          Shelf needs permission to read files on this device. Android asks for it on a settings
          page.
        </p>
        <Button
          type="button"
          onClick={() => grant.mutate()}
          disabled={storage.isPending || grant.isPending}
        >
          Allow access to files
        </Button>
        {grant.data === false && (
          <p className="text-sm text-destructive">Access wasn't granted. Try again.</p>
        )}
      </div>
    )
  }
  return (
    <div className="flex flex-col gap-2">
      {path && <p className="text-sm break-all text-muted-foreground">{path}</p>}
      <ChooseFolder
        label={path ? "Choose another folder" : "Choose folder"}
        variant={path ? "outline" : "default"}
        onPick={onPick}
      />
    </div>
  )
}

function KindButton({
  active,
  onClick,
  children,
}: {
  active: boolean
  onClick: () => void
  children: ReactNode
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        "rounded-lg border px-3 py-2 text-sm transition-colors",
        active ? "border-primary bg-accent font-medium" : "text-muted-foreground hover:bg-muted",
      )}
    >
      {children}
    </button>
  )
}

function Field({
  label,
  error,
  children,
}: {
  label: string
  error?: ReactNode
  children: ReactNode
}) {
  return (
    <div className="flex flex-col gap-2">
      <span className="text-sm font-medium">{label}</span>
      {children}
      {error && <p className="text-sm text-destructive">{error}</p>}
    </div>
  )
}
