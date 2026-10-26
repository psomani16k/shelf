import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { ClipboardCopy, Download, LoaderCircle } from "lucide-react"
import { type ReactNode, useState } from "react"
import {
  copyLogs,
  type LogFileInfo,
  listLogFiles,
  logSettings,
  saveLogFile,
  saveLogs,
  setLogFiles,
} from "@/api/app"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { IS_ANDROID } from "@/lib/platform"

const LOG_SETTINGS_KEY = ["log-settings"]
const LOG_FILES_KEY = ["log-files"]

/** App only, under *Developer options*: how many log files this device keeps, and getting them off it. */
export function LogsSection() {
  return (
    <section aria-label="Logs" className="flex flex-col gap-4">
      <h3 className="-mb-2 px-1 font-medium">Logs</h3>
      <p className="px-1 text-sm text-muted-foreground">
        The app keeps a log of what it does, to help track down problems. It stays on this device
        until you copy or save it.
      </p>
      <div className="divide-y rounded-lg border px-4">
        <KeepFiles />
        <CopyLogs />
        <SaveLogs />
      </div>
      <LogFiles />
    </section>
  )
}

/** Every log file on this device, newest first, each with its own save button. */
function LogFiles() {
  const list = useQuery({ queryKey: LOG_FILES_KEY, queryFn: listLogFiles })
  if (list.isPending) return null
  if (list.isError) return <p className="px-1 text-sm text-destructive">{String(list.error)}</p>

  return (
    <section aria-label="Log files">
      <h3 className="mb-2 px-1 font-medium">Files</h3>
      <ul className="divide-y rounded-lg border px-4">
        {list.data.files.map((file) => (
          <LogFileRow key={file.name} file={file} />
        ))}
      </ul>
      {!IS_ANDROID && (
        <p className="mt-2 px-1 text-xs break-all text-muted-foreground">In {list.data.dir}</p>
      )}
    </section>
  )
}

function LogFileRow({ file }: { file: LogFileInfo }) {
  const save = useMutation({ mutationFn: () => saveLogFile(file.name) })
  return (
    <li className="flex items-center gap-3 py-3">
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium">{timeRange(file.started, file.last_write)}</p>
        <p className="truncate text-sm text-muted-foreground">
          <span className="font-mono text-xs">{file.name}</span>
          {file.current && " · this run"}
        </p>
        {save.data === true && <p className="text-sm">Saved.</p>}
        {save.isError && <p className="text-sm text-destructive">{String(save.error)}</p>}
      </div>
      <span className="shrink-0 text-sm text-muted-foreground tabular-nums">
        {fileSize(file.bytes)}
      </span>
      <Button
        variant="ghost"
        size="icon"
        className="shrink-0"
        onClick={() => save.mutate()}
        disabled={save.isPending}
        aria-label={`Save ${file.name}`}
      >
        {save.isPending ? <LoaderCircle className="animate-spin" /> : <Download />}
      </Button>
    </li>
  )
}

const DATE_TIME = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" })
const TIME = new Intl.DateTimeFormat(undefined, { timeStyle: "short" })

/** `26 Oct 2026, 10:42 – 10:55`, with the date repeated only if the end is on another day. */
function timeRange(first: string | null, last: string | null): string {
  const start = parseTime(first)
  const end = parseTime(last)
  if (start && end) {
    const sameDay = start.toDateString() === end.toDateString()
    return `${DATE_TIME.format(start)} – ${(sameDay ? TIME : DATE_TIME).format(end)}`
  }
  const only = start ?? end
  return only ? DATE_TIME.format(only) : "Unknown time"
}

function parseTime(value: string | null): Date | undefined {
  if (!value) return undefined
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? undefined : date
}

function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const kb = bytes / 1024
  if (kb < 1024) return `${kb < 10 ? kb.toFixed(1) : Math.round(kb)} KB`
  return `${(kb / 1024).toFixed(1)} MB`
}

function KeepFiles() {
  const queryClient = useQueryClient()
  const settings = useQuery({ queryKey: LOG_SETTINGS_KEY, queryFn: logSettings })
  // What's typed but not saved yet; saved on Enter or leaving the field.
  const [draft, setDraft] = useState<string>()
  const save = useMutation({
    mutationFn: setLogFiles,
    onSuccess: (updated) => {
      queryClient.setQueryData(LOG_SETTINGS_KEY, updated)
      // Lowering the count deletes files.
      void queryClient.invalidateQueries({ queryKey: LOG_FILES_KEY })
      setDraft(undefined)
    },
  })
  const keep = settings.data?.keep_files
  const max = settings.data?.max_keep_files ?? 50

  const value = draft ?? (keep === undefined ? "" : String(keep))
  const n = Number(value)
  const valid = value.trim() !== "" && Number.isInteger(n) && n >= 1 && n <= max

  const commit = () => {
    if (draft === undefined || !valid) return
    if (n === keep) setDraft(undefined)
    else save.mutate(n)
  }

  return (
    <Row
      title="Log files to keep"
      description="Each start of the app begins a new file. Past this many, the oldest are deleted."
      error={
        (draft !== undefined && !valid && `Enter a whole number from 1 to ${max}.`) ||
        (settings.isError && String(settings.error)) ||
        (save.isError && String(save.error))
      }
    >
      <Input
        type="number"
        inputMode="numeric"
        min={1}
        max={max}
        step={1}
        className="w-full shrink-0 sm:w-44"
        aria-label="Log files to keep"
        aria-invalid={draft !== undefined && !valid}
        value={value}
        disabled={!settings.data || save.isPending}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit()
        }}
      />
    </Row>
  )
}

function CopyLogs() {
  const copy = useMutation({ mutationFn: copyLogs })
  return (
    <Row
      title="Copy logs"
      description="The most recent part of the log, to paste into a message or a bug report."
      status={copy.isSuccess && "Copied."}
      error={copy.isError && String(copy.error)}
    >
      <ActionButton pending={copy.isPending} onClick={() => copy.mutate()}>
        <ClipboardCopy />
        Copy
      </ActionButton>
    </Row>
  )
}

function SaveLogs() {
  const save = useMutation({ mutationFn: saveLogs })
  return (
    <Row
      title="Save logs"
      description="All of the log, as one file wherever you choose."
      status={save.data === true && "Saved."}
      error={save.isError && String(save.error)}
    >
      <ActionButton pending={save.isPending} onClick={() => save.mutate()}>
        <Download />
        Save…
      </ActionButton>
    </Row>
  )
}

function ActionButton({
  pending,
  onClick,
  children,
}: {
  pending: boolean
  onClick: () => void
  children: ReactNode
}) {
  return (
    <Button
      variant="outline"
      className="w-full shrink-0 sm:w-44"
      onClick={onClick}
      disabled={pending}
    >
      {pending ? <LoaderCircle className="animate-spin" /> : children}
    </Button>
  )
}

function Row({
  title,
  description,
  status,
  error,
  children,
}: {
  title: string
  description: string
  status?: ReactNode
  error?: ReactNode
  children: ReactNode
}) {
  return (
    <div className="flex flex-col gap-3 py-4 sm:flex-row sm:items-center sm:justify-between sm:gap-6">
      <div className="min-w-0">
        <p className="text-sm font-medium">{title}</p>
        <p className="text-sm text-muted-foreground">{description}</p>
        {status && <p className="mt-1 text-sm">{status}</p>}
        {error && <p className="mt-1 text-sm text-destructive">{error}</p>}
      </div>
      {children}
    </div>
  )
}
