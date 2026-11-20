import { create } from "@bufbuild/protobuf"
import { ArrowDown, ArrowUp, LoaderCircle, Trash2 } from "lucide-react"
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react"
import { ActionRow } from "@/components/settings/ActionRow"
import { ChoiceSelect } from "@/components/settings/ChoiceSelect"
import { SettingsGroup } from "@/components/settings/SettingsGroup"
import { useSettingsPage } from "@/components/settings/settingsNav"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Slider } from "@/components/ui/slider"
import { Switch } from "@/components/ui/switch"
import { ThemeSource_Value } from "@/gen/apipb/theme_pb"
import { ReaderImages_Value } from "@/gen/configpb/config_pb"
import { ReaderThemeSpecSchema, type ThemeLayer } from "@/gen/configpb/theme_pb"
import {
  applyLook,
  type ReaderImages,
  type ThemeLook,
  themeId,
  themeLook,
} from "@/lib/readerTheme"
import {
  copyLayer,
  LAYER_LABELS,
  type LayerKind,
  MAX_LAYERS,
  moveItem,
  newLayer,
} from "@/lib/themeLayers"
import { PREVIEW_HTML } from "@/lib/themePreview"
import { useLibraryConfig, useReaderThemes, useSaveTheme } from "@/lib/useAppearance"
import { cn } from "cn"

/** A layer and a key that stays with it as layers move, for React. */
type Item = { key: number; layer: ThemeLayer }
let nextKey = 0
const toItem = (layer: ThemeLayer): Item => ({ key: nextKey++, layer })

const NONE = "none"
const ADD = "add"

/** Settings → Reader → Themes → *New theme*: a stack of layers, a name and a live preview. */
export function ThemeBuilder() {
  const themes = useReaderThemes().data?.themes ?? []
  const keepImages = useLibraryConfig().data?.reader?.images !== ReaderImages_Value.LIKE_PAGE
  const save = useSaveTheme()
  const { back } = useSettingsPage()
  const [name, setName] = useState("")
  const [startFrom, setStartFrom] = useState(NONE)
  const [items, setItems] = useState<Item[]>([])
  const [images, setImages] = useState<ReaderImages>(keepImages ? "keep" : "page")
  const [confirming, setConfirming] = useState(false)

  const look = useMemo(() => {
    const spec = create(ReaderThemeSpecSchema, { layers: items.map((i) => i.layer) })
    return themeLook(spec, images)
  }, [items, images])

  const id = themeId(name)
  const clash = id === null ? undefined : themes.find((t) => t.id === id)
  const clashName = clash?.spec?.name ?? id
  const nameNote = !name.trim()
    ? "Needed to save; it's what the Theme list shows."
    : id === null
      ? "1 to 32 of letters (A–Z), digits, spaces, - and _."
      : clash?.source === ThemeSource_Value.BUILT_IN
        ? `Saving replaces the built-in “${clashName}” until you delete yours.`
        : clash
          ? `Saving replaces your theme “${clashName}”.`
          : undefined
  const unknown = items.some((i) => i.layer.kind.case === undefined)

  const update = (key: number, layer: ThemeLayer) =>
    setItems((all) => all.map((i) => (i.key === key ? { key, layer } : i)))

  const startWith = (from: string) => {
    setStartFrom(from)
    const layers = themes.find((t) => t.id === from)?.spec?.layers ?? []
    setItems(layers.map((layer) => toItem(copyLayer(layer))))
  }

  const onSave = () => {
    if (clash && !confirming) return setConfirming(true)
    const layers = items.map((i) => i.layer)
    save.mutate({ name: name.trim(), layers }, { onSuccess: () => back("themes") })
  }

  return (
    <div className="flex flex-col gap-6">
      <Preview look={look} images={images} onImages={setImages} />

      <SettingsGroup label="Theme">
        <ActionRow title="Name" description={nameNote}>
          <Input
            value={name}
            onChange={(e) => {
              setName(e.target.value)
              setConfirming(false)
            }}
            placeholder="My theme"
            aria-invalid={name.trim() !== "" && id === null}
            maxLength={40}
            className="w-full shrink-0 sm:w-56"
          />
        </ActionRow>
        <ActionRow
          title="Start from"
          description="Copies a theme's layers to change from here. Your theme gets its own name."
        >
          <ChoiceSelect
            choices={[
              { value: NONE, label: "Book as published" },
              ...themes.map((t) => ({ value: t.id, label: t.spec?.name ?? t.id })),
            ]}
            value={startFrom}
            disabled={false}
            label="Start from"
            className="w-full shrink-0 sm:w-56"
            onChange={startWith}
          />
        </ActionRow>
      </SettingsGroup>

      <SettingsGroup
        label="Layers"
        list
        description="Applied to the page from the top down. No layers leaves the book as published."
        footer={
          <ChoiceSelect<string>
            choices={[
              { value: ADD, label: "Add a layer…" },
              ...(Object.keys(LAYER_LABELS) as LayerKind[]).map((kind) => ({
                value: kind,
                label: LAYER_LABELS[kind],
              })),
            ]}
            value={ADD}
            disabled={items.length >= MAX_LAYERS}
            label="Add a layer"
            className="mt-3 w-full sm:w-56"
            onChange={(kind) => {
              if (kind !== ADD) setItems((all) => [...all, toItem(newLayer(kind as LayerKind))])
            }}
          />
        }
      >
        {items.length === 0 && (
          <li className="py-4 text-sm text-muted-foreground">No layers yet.</li>
        )}
        {items.map((item, i) => (
          <LayerCard
            key={item.key}
            layer={item.layer}
            index={i}
            count={items.length}
            onChange={(layer) => update(item.key, layer)}
            onMove={(by) => setItems((all) => moveItem(all, i, by))}
            onRemove={() => setItems((all) => all.filter((x) => x.key !== item.key))}
          />
        ))}
      </SettingsGroup>

      <div className="flex flex-col items-end gap-2">
        {save.isError && <p className="text-sm text-destructive">{String(save.error)}</p>}
        {unknown && (
          <p className="text-sm text-destructive">Delete the unknown layers to save.</p>
        )}
        <div className="flex items-center gap-2">
          {confirming && (
            <>
              <span className="text-sm">Replace “{clashName}”?</span>
              <Button variant="ghost" onClick={() => setConfirming(false)}>
                Cancel
              </Button>
            </>
          )}
          <Button onClick={onSave} disabled={id === null || unknown || save.isPending}>
            {save.isPending && <LoaderCircle className="animate-spin" />}
            {confirming ? "Replace" : "Save theme"}
          </Button>
        </div>
      </div>
    </div>
  )
}

/**
 * The sample page in an iframe, styled and themed the way the reader styles a book's pages, so
 * it shows exactly what a book would.
 */
function Preview({
  look,
  images,
  onImages,
}: {
  look: ThemeLook
  images: ReaderImages
  onImages: (images: ReaderImages) => void
}) {
  const frame = useRef<HTMLIFrameElement>(null)
  const [loaded, setLoaded] = useState(false)

  useEffect(() => {
    const doc = frame.current?.contentDocument
    if (loaded && doc) applyLook(doc, look)
  }, [look, loaded])

  return (
    // Sticks under the header while the layers scroll, so changes show as they're made.
    <div className="sticky -top-4 z-10 -mx-4 bg-background px-4 pb-2 pt-4 md:-top-2 md:pt-2">
      <div
        className="h-64 overflow-hidden rounded-lg border md:h-80"
        style={{ background: look.page }}
      >
        <iframe
          ref={frame}
          title="Preview"
          srcDoc={PREVIEW_HTML}
          onLoad={() => setLoaded(true)}
          className="size-full"
          // Light like the page inside, or the iframe is painted opaque over the page colour.
          style={{ colorScheme: "light" }}
        />
      </div>
      <div className="mt-2 flex items-center gap-2 px-1">
        <span className="text-sm text-muted-foreground">Images</span>
        <div
          role="group"
          aria-label="Images in the preview"
          className="inline-flex rounded-lg bg-muted p-0.5"
        >
          {(
            [
              ["keep", "Keep colours"],
              ["page", "Like the page"],
            ] as const
          ).map(([value, text]) => (
            <button
              key={value}
              type="button"
              aria-pressed={images === value}
              onClick={() => onImages(value)}
              className={cn(
                "rounded-md px-2.5 py-1 text-sm transition-colors outline-none focus-visible:ring-3 focus-visible:ring-ring/50",
                images === value
                  ? "bg-background font-medium text-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              {text}
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}

function LayerCard({
  layer,
  index,
  count,
  onChange,
  onMove,
  onRemove,
}: {
  layer: ThemeLayer
  index: number
  count: number
  onChange: (layer: ThemeLayer) => void
  onMove: (by: number) => void
  onRemove: () => void
}) {
  const kind = layer.kind.case
  const label = kind ? LAYER_LABELS[kind] : "Unknown layer"
  return (
    <li className="flex flex-col gap-3 py-3">
      <div className="flex items-center gap-1">
        <span className="flex-1 text-sm font-medium">
          {index + 1}. {label}
        </span>
        <Button
          variant="ghost"
          size="icon"
          aria-label={`Move ${label} up`}
          disabled={index === 0}
          onClick={() => onMove(-1)}
        >
          <ArrowUp />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          aria-label={`Move ${label} down`}
          disabled={index === count - 1}
          onClick={() => onMove(1)}
        >
          <ArrowDown />
        </Button>
        <Button variant="ghost" size="icon" aria-label={`Delete ${label}`} onClick={onRemove}>
          <Trash2 />
        </Button>
      </div>
      {kind ? (
        <>
          <LayerControls layer={layer} onChange={onChange} />
          <label className="flex items-center gap-3 text-sm">
            <Switch
              checked={layer.skipImages}
              onCheckedChange={(skipImages) => onChange({ ...layer, skipImages })}
            />
            Leave images out, when they keep their colours
          </label>
        </>
      ) : (
        <p className="text-sm text-muted-foreground">
          Made by a newer version of Shelf, which this one can't show or save.
        </p>
      )}
    </li>
  )
}

function LayerControls({
  layer,
  onChange,
}: {
  layer: ThemeLayer
  onChange: (layer: ThemeLayer) => void
}) {
  const { kind } = layer
  const set = (next: ThemeLayer["kind"]) => onChange({ ...layer, kind: next })
  switch (kind.case) {
    case "invert":
      return <p className="text-sm text-muted-foreground">White becomes black, and black white.</p>
    case "hueRotate":
      return (
        <NumberSlider
          label="Angle"
          unit="°"
          min={-180}
          max={180}
          step={1}
          value={kind.value.degrees}
          onChange={(degrees) => set({ case: kind.case, value: { ...kind.value, degrees } })}
        />
      )
    case "saturate":
      return (
        <NumberSlider
          label="Amount"
          min={0}
          max={3}
          step={0.05}
          value={kind.value.amount ?? 1}
          onChange={(amount) => set({ case: kind.case, value: { ...kind.value, amount } })}
        />
      )
    case "brightnessContrast":
      return (
        <>
          <NumberSlider
            label="Brightness"
            min={0}
            max={2}
            step={0.05}
            value={kind.value.brightness ?? 1}
            onChange={(brightness) =>
              set({ case: kind.case, value: { ...kind.value, brightness } })
            }
          />
          <NumberSlider
            label="Contrast"
            min={0}
            max={2}
            step={0.05}
            value={kind.value.contrast ?? 1}
            onChange={(contrast) => set({ case: kind.case, value: { ...kind.value, contrast } })}
          />
        </>
      )
    case "levels":
      return (
        <>
          <ColorField
            label="Black becomes"
            value={kind.value.black || "#000000"}
            onChange={(black) => set({ case: kind.case, value: { ...kind.value, black } })}
          />
          <ColorField
            label="White becomes"
            value={kind.value.white || "#ffffff"}
            onChange={(white) => set({ case: kind.case, value: { ...kind.value, white } })}
          />
        </>
      )
    case "multiply":
      return (
        <ColorField
          label="Colour"
          value={kind.value.color}
          onChange={(color) => set({ case: kind.case, value: { ...kind.value, color } })}
        />
      )
    case "matrix":
      return (
        <MatrixFields
          values={kind.value.values}
          onChange={(values) => set({ case: kind.case, value: { ...kind.value, values } })}
        />
      )
    default:
      return null
  }
}

function NumberSlider({
  label,
  value,
  min,
  max,
  step,
  unit = "",
  onChange,
}: {
  label: string
  value: number
  min: number
  max: number
  step: number
  unit?: string
  onChange: (value: number) => void
}) {
  return (
    <div className="flex items-center gap-3">
      <span className="w-24 shrink-0 text-sm">{label}</span>
      <Slider
        className="flex-1"
        min={min}
        max={max}
        step={step}
        value={[value]}
        aria-label={label}
        onValueChange={(v) => onChange(typeof v === "number" ? v : v[0])}
      />
      <span className="w-14 shrink-0 text-right text-sm tabular-nums">
        {+value.toFixed(2)}
        {unit}
      </span>
    </div>
  )
}

const HEX = /^#[0-9a-f]{6}$/i

function ColorField({
  label,
  value,
  onChange,
}: {
  label: string
  value: string
  onChange: (value: string) => void
}) {
  const valid = HEX.test(value)
  return (
    <div className="flex items-center gap-3">
      <span className="w-24 shrink-0 text-sm">{label}</span>
      <input
        type="color"
        aria-label={label}
        value={valid ? value.toLowerCase() : "#000000"}
        onChange={(e) => onChange(e.target.value)}
        className="h-8 w-10 shrink-0 cursor-pointer rounded-md border bg-transparent p-0.5"
      />
      <Input
        aria-label={`${label}, as #rrggbb`}
        value={value}
        onChange={(e) => onChange(e.target.value.trim())}
        aria-invalid={!valid}
        spellCheck={false}
        className="w-28 font-mono"
      />
    </div>
  )
}

const ROWS = ["Red", "Green", "Blue"]
const COLUMNS = ["from red", "from green", "from blue", "plus"]

/** The 12 values as 3 rows of `r g b offset`, each output channel from the input ones. */
function MatrixFields({
  values,
  onChange,
}: {
  values: number[]
  onChange: (values: number[]) => void
}) {
  return (
    <div className="grid grid-cols-[auto_repeat(4,minmax(0,1fr))] items-center gap-2 text-sm">
      <span />
      {COLUMNS.map((column) => (
        <span key={column} className="text-center text-xs text-muted-foreground">
          {column}
        </span>
      ))}
      {ROWS.map((row, r) => (
        <MatrixRow key={row} row={row}>
          {COLUMNS.map((column, c) => (
            <NumberInput
              key={column}
              label={`${row} ${column}`}
              value={values[r * 4 + c] ?? 0}
              onChange={(v) => onChange(values.map((old, i) => (i === r * 4 + c ? v : old)))}
            />
          ))}
        </MatrixRow>
      ))}
    </div>
  )
}

function MatrixRow({ row, children }: { row: string; children: ReactNode }) {
  return (
    <>
      <span className="pr-1">{row}</span>
      {children}
    </>
  )
}

/** Keeps what's typed (`-`, `0.`) while only finite numbers reach the layer. */
function NumberInput({
  label,
  value,
  onChange,
}: {
  label: string
  value: number
  onChange: (value: number) => void
}) {
  const [text, setText] = useState(String(value))
  const n = Number(text)
  return (
    <Input
      aria-label={label}
      inputMode="decimal"
      value={text}
      aria-invalid={text.trim() === "" || !Number.isFinite(n)}
      onChange={(e) => {
        setText(e.target.value)
        const next = Number(e.target.value)
        if (e.target.value.trim() !== "" && Number.isFinite(next)) onChange(next)
      }}
      className="min-w-0 px-1.5 text-center font-mono"
    />
  )
}
