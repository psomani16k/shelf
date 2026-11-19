import { clone, create } from "@bufbuild/protobuf"
import { type ThemeLayer, ThemeLayerSchema } from "@/gen/configpb/theme_pb"

export type LayerKind = NonNullable<ThemeLayer["kind"]["case"]>

/** The builder's names for each kind, in its *Add layer* order. */
export const LAYER_LABELS: Record<LayerKind, string> = {
  invert: "Invert",
  hueRotate: "Hue rotate",
  saturate: "Saturation",
  brightnessContrast: "Brightness and contrast",
  levels: "Levels",
  multiply: "Multiply",
  matrix: "Colour matrix",
}

/** The backend's limit (`MAX_LAYERS`). */
export const MAX_LAYERS = 32

/** A layer that changes nothing yet, except Invert, which is left off images by default. */
export function newLayer(kind: LayerKind): ThemeLayer {
  switch (kind) {
    case "invert":
      return create(ThemeLayerSchema, { skipImages: true, kind: { case: kind, value: {} } })
    case "hueRotate":
      return create(ThemeLayerSchema, { kind: { case: kind, value: { degrees: 0 } } })
    case "saturate":
      return create(ThemeLayerSchema, { kind: { case: kind, value: { amount: 1 } } })
    case "brightnessContrast":
      return create(ThemeLayerSchema, {
        kind: { case: kind, value: { brightness: 1, contrast: 1 } },
      })
    case "levels":
      return create(ThemeLayerSchema, {
        kind: { case: kind, value: { black: "#000000", white: "#ffffff" } },
      })
    case "multiply":
      return create(ThemeLayerSchema, { kind: { case: kind, value: { color: "#ffffff" } } })
    case "matrix":
      return create(ThemeLayerSchema, {
        kind: { case: kind, value: { values: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0] } },
      })
  }
}

/** A copy for the editor, so changing it never touches the cached theme it came from. */
export const copyLayer = (layer: ThemeLayer) => clone(ThemeLayerSchema, layer)

/** `items` with the one at `from` moved by `by` places, clamped to the ends. */
export function moveItem<T>(items: T[], from: number, by: number): T[] {
  const to = Math.min(items.length - 1, Math.max(0, from + by))
  const next = [...items]
  const [item] = next.splice(from, 1)
  next.splice(to, 0, item)
  return next
}
