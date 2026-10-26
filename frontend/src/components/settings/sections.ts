import type { MessageInitShape } from "@bufbuild/protobuf"
import { BookOpen, LibraryBig, type LucideIcon, Palette } from "lucide-react"
import type { ComponentType } from "react"
import {
  type Config,
  type ConfigSchema,
  ReaderFlow_Value,
  ReaderMargin_Value,
  ReaderTheme_Value,
  UiTheme_Value,
} from "@/gen/configpb/config_pb"
import { LibrarySection } from "./LibrarySection"

export type Setting = {
  id: string
  label: string
  description: string
  choices: { value: number; label: string }[]
  /** Unset means the proto zero value. */
  read: (config: Config) => number | undefined
  patch: (value: number) => MessageInitShape<typeof ConfigSchema>
  /** Greyed out when the setting has no effect, e.g. top margin while scrolling. */
  disabled?: (config: Config) => boolean
}

/** A nested message of `Config`, shown as its own card. */
export type Group = { label?: string; settings: Setting[] }

/**
 * Either one per `Config` field, in the same nesting, or a `content` component for settings
 * that don't live in the library config (actions, and app-only settings behind `IS_TAURI`).
 */
export type Section = {
  slug: "reader" | "ui" | "library"
  label: string
  icon: LucideIcon
} & ({ groups: Group[]; content?: never } | { content: ComponentType; groups?: never })

/** In size order; the enum's numbers put MEDIUM first only because it's the default. */
const MARGIN_CHOICES = [
  { value: ReaderMargin_Value.NONE, label: "None" },
  { value: ReaderMargin_Value.SMALL, label: "Small" },
  { value: ReaderMargin_Value.MEDIUM, label: "Medium" },
  { value: ReaderMargin_Value.LARGE, label: "Large" },
]

const scrolled = (c: Config) =>
  (c.reader?.flow ?? ReaderFlow_Value.SCROLLED) === ReaderFlow_Value.SCROLLED

export const SECTIONS: Section[] = [
  {
    slug: "reader",
    label: "Reader",
    icon: BookOpen,
    groups: [
      {
        settings: [
          {
            id: "reader.flow",
            label: "Layout",
            description: "Scroll through a chapter, or turn it page by page.",
            choices: [
              { value: ReaderFlow_Value.SCROLLED, label: "Scrolled" },
              { value: ReaderFlow_Value.PAGINATED, label: "Paginated" },
            ],
            read: (c) => c.reader?.flow,
            patch: (flow) => ({ reader: { flow } }),
          },
          {
            id: "reader.theme",
            label: "Theme",
            description: "Colours of the book's pages.",
            choices: [
              { value: ReaderTheme_Value.MATCH_UI, label: "Match interface" },
              { value: ReaderTheme_Value.LIGHT, label: "Light" },
              { value: ReaderTheme_Value.DARK, label: "Dark" },
            ],
            read: (c) => c.reader?.theme,
            patch: (theme) => ({ reader: { theme } }),
          },
        ],
      },
      {
        label: "Margins",
        settings: [
          {
            id: "reader.margins.top",
            label: "Top",
            description: "Paginated layout only.",
            choices: MARGIN_CHOICES,
            read: (c) => c.reader?.margins?.top,
            patch: (top) => ({ reader: { margins: { top } } }),
            disabled: scrolled,
          },
          {
            id: "reader.margins.bottom",
            label: "Bottom",
            description: "Paginated layout only.",
            choices: MARGIN_CHOICES,
            read: (c) => c.reader?.margins?.bottom,
            patch: (bottom) => ({ reader: { margins: { bottom } } }),
            disabled: scrolled,
          },
          {
            id: "reader.margins.side",
            label: "Left and right",
            description: "Space beside the text. Sizes scale with the screen.",
            choices: MARGIN_CHOICES,
            read: (c) => c.reader?.margins?.side,
            patch: (side) => ({ reader: { margins: { side } } }),
          },
          {
            id: "reader.margins.between_pages",
            label: "Between pages",
            description: "Paginated layout only, when two pages are side by side (landscape).",
            choices: MARGIN_CHOICES,
            read: (c) => c.reader?.margins?.betweenPages,
            patch: (betweenPages) => ({ reader: { margins: { betweenPages } } }),
            disabled: scrolled,
          },
        ],
      },
    ],
  },
  {
    slug: "ui",
    label: "Interface",
    icon: Palette,
    groups: [
      {
        settings: [
          {
            id: "ui.theme",
            label: "Theme",
            description: "Light or dark interface. System follows your device.",
            choices: [
              { value: UiTheme_Value.SYSTEM, label: "System" },
              { value: UiTheme_Value.LIGHT, label: "Light" },
              { value: UiTheme_Value.DARK, label: "Dark" },
            ],
            read: (c) => c.ui?.theme,
            patch: (theme) => ({ ui: { theme } }),
          },
        ],
      },
    ],
  },
  { slug: "library", label: "Library", icon: LibraryBig, content: LibrarySection },
]
