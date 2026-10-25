import { useMutation, useQueryClient } from "@tanstack/react-query"
import { ArrowLeft, ChevronRight, LoaderCircle, X } from "lucide-react"
import { useEffect, useRef, useState } from "react"
import { Navigate, useLocation, useNavigate, useParams } from "react-router"
import { setConfig } from "@/api"
import { ScreenHeader } from "@/components/ScreenHeader"
import { type Group, type Section, SECTIONS, type Setting } from "@/components/settings/sections"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import type { Config } from "@/gen/configpb/config_pb"
import { LIBRARY_CONFIG_KEY, useLibraryConfig } from "@/lib/useAppearance"
import { useMediaQuery } from "@/lib/useMediaQuery"
import { cn } from "cn"

/**
 * `depth` counts the history entries settings has pushed, so closing can step back past all of
 * them. It's missing after a reload or a deep link. `fromList` marks a section opened from the
 * phone list, so its back arrow can just go back.
 */
type NavState = { depth?: number; fromList?: boolean } | null

export const SETTINGS_PATH = "/settings"

/** Tailwind's `md`. */
const WIDE = "(min-width: 48rem)"

/** Phones get a list of sections, each opening its own screen; wider screens get a sidebar. */
export function Settings() {
  const { section: slug } = useParams()
  const navigate = useNavigate()
  const location = useLocation()
  const state = location.state as NavState
  const depth = state?.depth
  const wide = useMediaQuery(WIDE)
  const rootRef = useRef<HTMLDivElement>(null)
  // Read during the first render: once the page behind turns inert, the browser drops its focus.
  const [returnFocus] = useState(() => document.activeElement as HTMLElement | null)

  const close = () => (depth ? navigate(-depth) : navigate("/", { replace: true }))
  const closeRef = useRef(close)
  useEffect(() => {
    closeRef.current = close
  })

  useEffect(() => {
    rootRef.current?.focus()
    // A select closing on Escape marks the event handled; only a free Escape closes settings.
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !e.defaultPrevented) closeRef.current()
    }
    window.addEventListener("keydown", onKey)
    return () => {
      window.removeEventListener("keydown", onKey)
      returnFocus?.focus({ preventScroll: true })
    }
  }, [returnFocus])

  const section = SECTIONS.find((s) => s.slug === slug)
  if (slug && !section) return <Navigate to={SETTINGS_PATH} replace state={state} />
  const shown = section ?? (wide ? SECTIONS[0] : undefined)

  const openSection = (s: Section) => {
    const to = `${SETTINGS_PATH}/${s.slug}`
    if (wide) void navigate(to, { replace: true, state })
    else void navigate(to, { state: { depth: depth && depth + 1, fromList: true } })
  }
  const backToList = () =>
    state?.fromList ? navigate(-1) : navigate(SETTINGS_PATH, { replace: true, state: { depth } })

  return (
    // tabIndex: clicks on plain text keep focus inside, so Escape still reaches the handler.
    <div ref={rootRef} tabIndex={-1} className="fixed inset-0 z-50 flex bg-background outline-none">
      {(wide || !shown) && (
        <nav className="flex w-full flex-col md:w-64 md:shrink-0 md:border-r">
          <ScreenHeader
            title="Settings"
            className="md:border-b-0"
            start={
              !wide && (
                <Button variant="ghost" size="icon" onClick={close} aria-label="Close settings">
                  <ArrowLeft />
                </Button>
              )
            }
            end={
              wide && (
                <Button variant="ghost" size="icon" onClick={close} aria-label="Close settings">
                  <X />
                </Button>
              )
            }
          />
          <ul className="flex flex-col gap-1 p-2">
            {SECTIONS.map((s) => (
              <li key={s.slug}>
                <button
                  type="button"
                  onClick={() => openSection(s)}
                  aria-current={s === shown ? "page" : undefined}
                  className={cn(
                    "flex w-full items-center gap-3 rounded-md px-3 py-2.5 text-left text-sm hover:bg-muted",
                    s === shown && "bg-accent font-medium text-accent-foreground",
                  )}
                >
                  <s.icon className="size-4 text-muted-foreground" />
                  <span className="flex-1">{s.label}</span>
                  {!wide && <ChevronRight className="size-4 text-muted-foreground" />}
                </button>
              </li>
            ))}
          </ul>
        </nav>
      )}

      {shown && (
        <main className="flex min-w-0 flex-1 flex-col">
          <ScreenHeader
            title={shown.label}
            className="md:border-b-0"
            start={
              !wide && (
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={backToList}
                  aria-label="Back to settings"
                >
                  <ArrowLeft />
                </Button>
              )
            }
          />
          <div className="flex-1 overflow-y-auto overscroll-contain p-4 md:px-8 md:pb-8 md:pt-2">
            <div className="mx-auto max-w-2xl">
              <SectionSettings section={shown} />
            </div>
          </div>
        </main>
      )}
    </div>
  )
}

function SectionSettings({ section }: { section: Section }) {
  if (section.content) return <section.content />
  return <ConfigSettings groups={section.groups} />
}

/** Settings stored in the library's config, one `Select` each. */
function ConfigSettings({ groups }: { groups: Group[] }) {
  const queryClient = useQueryClient()
  const config = useLibraryConfig()
  const save = useMutation({
    mutationFn: setConfig,
    onSuccess: (updated) => queryClient.setQueryData(LIBRARY_CONFIG_KEY, updated),
  })

  if (config.isPending) {
    return <LoaderCircle className="mx-auto mt-8 size-6 animate-spin text-muted-foreground" />
  }
  if (config.isError) return <p className="text-destructive">{String(config.error)}</p>

  return (
    <>
      {save.isError && <p className="mb-4 text-destructive">{String(save.error)}</p>}
      <div className="flex flex-col gap-8">
        {groups.map((group, i) => (
          <section key={group.label ?? i} aria-label={group.label}>
            {group.label && <h3 className="mb-2 px-1 font-medium">{group.label}</h3>}
            <div className="divide-y rounded-lg border px-4">
              {group.settings.map((setting) => (
                <SettingRow
                  key={setting.id}
                  setting={setting}
                  config={config.data}
                  onChange={(value) => save.mutate(setting.patch(value))}
                />
              ))}
            </div>
          </section>
        ))}
      </div>
    </>
  )
}

function SettingRow({
  setting,
  config,
  onChange,
}: {
  setting: Setting
  config: Config
  onChange: (value: number) => void
}) {
  const disabled = setting.disabled?.(config) ?? false
  return (
    <div
      className={cn(
        "flex flex-col gap-3 py-4 sm:flex-row sm:items-center sm:justify-between sm:gap-6",
        disabled && "opacity-60",
      )}
    >
      <div className="min-w-0">
        <p className="text-sm font-medium">{setting.label}</p>
        <p className="text-sm text-muted-foreground">{setting.description}</p>
      </div>
      <Select
        items={setting.choices}
        value={setting.read(config) ?? 0}
        disabled={disabled}
        onValueChange={(value) => {
          if (value !== null) onChange(value)
        }}
      >
        <SelectTrigger className="w-full shrink-0 sm:w-44" aria-label={setting.label}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {setting.choices.map((choice) => (
            <SelectItem key={choice.value} value={choice.value}>
              {choice.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}
