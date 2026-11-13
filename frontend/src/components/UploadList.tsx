import { CircleAlert, CircleCheck } from "lucide-react"
import { ProgressBar } from "@/components/ProgressBar"
import { Button } from "@/components/ui/button"
import { uploads, useUploads } from "@/lib/uploads"

/** The uploads of this session, newest last, until cleared. */
export function UploadList() {
  const items = useUploads()
  if (items.length === 0) return null
  const busy = items.some((u) => u.status === "queued" || u.status === "uploading")

  return (
    <div className="flex max-h-48 flex-col border-t">
      <div className="flex items-center justify-between py-1 pr-2 pl-4">
        <span className="text-sm font-medium">Uploads</span>
        {!busy && (
          <Button variant="ghost" size="sm" onClick={() => uploads.clearFinished()}>
            Clear
          </Button>
        )}
      </div>
      <ul className="flex flex-col gap-2 overflow-y-auto overscroll-contain px-4 pb-2">
        {items.map((u) => (
          <li key={u.id} className="flex flex-col gap-1 text-sm">
            <div className="flex items-center gap-2">
              <span className="min-w-0 flex-1 truncate">{u.name}</span>
              {u.status === "done" && <CircleCheck className="size-4 shrink-0 text-primary" />}
              {u.status === "failed" && (
                <CircleAlert className="size-4 shrink-0 text-destructive" />
              )}
              {u.status === "queued" && (
                <span className="shrink-0 text-xs text-muted-foreground">Waiting</span>
              )}
            </div>
            {u.status === "uploading" && (
              <ProgressBar label={`Uploading ${u.name}`} fraction={u.size ? u.sent / u.size : 1} />
            )}
            {u.error && <p className="text-xs text-destructive">{u.error}</p>}
          </li>
        ))}
      </ul>
    </div>
  )
}
