import { ArrowLeft, ChevronRight, LoaderCircle, X } from "lucide-react"
import { useEffect, useRef, useState } from "react"
import { Navigate, useLocation, useNavigate, useParams } from "react-router"
import { ScreenHeader } from "@/components/ScreenHeader"
import { ChoiceRow } from "@/components/settings/ChoiceRow"
import {
  type ConfigPatch,
  type CustomSetting,
  type Group,
  type Section,
  useVisibleSections,
} from "@/components/settings/sections"
import { SettingsGroup } from "@/components/settings/SettingsGroup"
import { type NavState, SETTINGS_PATH, useSettingsPage } from "@/components/settings/settingsNav"
import { Button } from "@/components/ui/button"
import { useBackButton } from "@/lib/backButton"
import { useLibraryConfig, useSetConfig } from "@/lib/useAppearance"
import { useLatest } from "@/lib/useLatest"
import { useMediaQuery } from "@/lib/useMediaQuery"
import { cn } from "cn"

export { SETTINGS_PATH }

/** Tailwind's `md`. */
const WIDE = "(min-width: 48rem)"

/** Phones get a list of sections, each opening its own screen; wider screens get a sidebar. */
export function Settings() {
  const { section: slug, page: pageSlug } = useParams()
  const navigate = useNavigate()
  const location = useLocation()
  const state = location.state as NavState
  const depth = state?.depth
  const wide = useMediaQuery(WIDE)
  const { sections, pending } = useVisibleSections()
  const rootRef = useRef<HTMLDivElement>(null)
  // Read during the first render: once the page behind turns inert, the browser drops its focus.
  const [returnFocus] = useState(() => document.activeElement as HTMLElement | null)

  const close = () => (depth ? navigate(-depth) : navigate("/", { replace: true }))
  const closeRef = useLatest(close)

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
  }, [returnFocus, closeRef])

  const section = sections.find((s) => s.slug === slug)
  const page = pageSlug ? section?.pages?.[pageSlug] : undefined
  const parentTitle = (page?.parent && section?.pages?.[page.parent]?.title) || section?.label
  const pages = useSettingsPage()
  const backToList = () =>
    state?.fromList ? navigate(-1) : navigate(SETTINGS_PATH, { replace: true, state: { depth } })
  // Same as the header's back arrow: a page goes up a level, a section on a phone to the list.
  useBackButton(() => {
    if (page) pages.back(page.parent)
    else void (!wide && section ? backToList() : close())
  })

  if (slug && !section) {
    // A reload on an admin-only section: wait for the role before deciding it isn't there.
    if (pending) return null
    return <Navigate to={SETTINGS_PATH} replace state={state} />
  }
  if (section && pageSlug && !page) {
    return <Navigate to={`${SETTINGS_PATH}/${section.slug}`} replace state={state} />
  }
  const shown = section ?? (wide ? sections[0] : undefined)

  const openSection = (s: Section) => {
    const to = `${SETTINGS_PATH}/${s.slug}`
    if (wide) void navigate(to, { replace: true, state })
    else void navigate(to, { state: { depth: depth && depth + 1, fromList: true } })
  }

  const closeButton = (
    <Button variant="ghost" size="icon" onClick={close} aria-label="Close settings">
      {wide ? <X /> : <ArrowLeft />}
    </Button>
  )

  return (
    // tabIndex: clicks on plain text keep focus inside, so Escape still reaches the handler.
    <div ref={rootRef} tabIndex={-1} className="safe-area fixed inset-0 z-50 flex bg-background outline-none">
      {(wide || !shown) && (
        <nav className="flex w-full flex-col md:w-64 md:shrink-0 md:border-r">
          <ScreenHeader
            title="Settings"
            className="md:border-b-0"
            start={!wide && closeButton}
            end={wide && closeButton}
          />
          <ul className="flex flex-col gap-1 p-2">
            {sections.map((s) => (
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
            title={page?.title ?? shown.label}
            className="md:border-b-0"
            start={
              page ? (
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => pages.back(page.parent)}
                  aria-label={`Back to ${parentTitle}`}
                >
                  <ArrowLeft />
                </Button>
              ) : (
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
              )
            }
          />
          <div className="flex-1 overflow-y-auto overscroll-contain p-4 md:px-8 md:pb-8 md:pt-2">
            <div className="mx-auto max-w-2xl">
              {page ? (
                <page.component />
              ) : (
                <>
                  {shown.groups && <ConfigSettings groups={shown.groups} />}
                  {shown.content && <shown.content />}
                </>
              )}
            </div>
          </div>
        </main>
      )}
    </div>
  )
}

/** The user's own settings, stored in the library (`users/<user>/config.json`), one `Select` each. */
function ConfigSettings({ groups }: { groups: Group[] }) {
  const config = useLibraryConfig()
  const save = useSetConfig()

  if (config.isPending) {
    return <LoaderCircle className="mx-auto mt-8 size-6 animate-spin text-muted-foreground" />
  }
  if (config.isError) return <p className="text-destructive">{String(config.error)}</p>

  return (
    <>
      {save.isError && <p className="mb-4 text-destructive">{String(save.error)}</p>}
      <div className="flex flex-col gap-8">
        {groups.map((group, i) => (
          <SettingsGroup key={group.label ?? i} label={group.label}>
            {group.settings.map((setting, j) =>
              typeof setting === "function" ? (
                <CustomRow key={j} Row={setting} save={save.mutate} />
              ) : (
                <ChoiceRow
                  key={setting.id}
                  label={setting.label}
                  description={setting.description}
                  choices={setting.choices}
                  value={setting.read(config.data) ?? 0}
                  disabled={setting.disabled?.(config.data) ?? false}
                  onChange={(value) => save.mutate(setting.patch(value))}
                />
              ),
            )}
          </SettingsGroup>
        ))}
      </div>
    </>
  )
}

/** Renders a group's `CustomSetting`; a component named in JSX must start with a capital. */
function CustomRow({ Row, save }: { Row: CustomSetting; save: ConfigPatch }) {
  return <Row save={save} />
}
