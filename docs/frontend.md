# Frontend

`frontend/` is one React SPA used by both the web build (served by `shelf-server`) and the Tauri app.

Stack: React 19, Vite 8, TypeScript 6, Tailwind CSS v4, shadcn/ui on Base UI (Nova preset), TanStack Query v5, React Router v8, protobuf-es v2, lucide icons, foliate-js for rendering EPUBs.

## Layout (`frontend/src`)

| Path | What |
|---|---|
| `main.tsx` | Creates the `QueryClient`, hooks up cover URL revocation, renders `App` |
| `App.tsx` | Web: `<BrowserRouter><Home/>`. App: `AppFlow` (pick → open the current library → `<MemoryRouter key={name}><Home/>`) |
| `api/` | The backend layer (below) |
| `screens/Home.tsx` | The main screen for both: library drawer, reader, contents drawer, settings route |
| `screens/Settings.tsx` | Settings overlay: sidebar on wide screens, list → section on phones |
| `screens/PickLibrary.tsx`, `OpenFailed.tsx`, `Syncing.tsx` | App-only screens before a library is open |
| `components/AddLibraryForm.tsx`, `LibrarySwitcher.tsx`, `RemoveLibraryConfirm.tsx` | App-only library management: add (name + folder or server), switch, remove with an inline confirm |
| `components/Reader.tsx` | foliate-js wrapper: open at saved position, input handling, progress saving |
| `components/LibraryPanel.tsx` | Folder/book grid with breadcrumb; in the app, the library switcher above its bottom bar |
| `components/TocPanel.tsx` | Table of contents tree |
| `components/SlideDrawer.tsx` | Two-sided drawer that follows the finger; drives transforms through one CSS variable `--p`. Touch and pen only: a drag starts from a 16px strip at either edge, the header, or an open panel or its scrim, and `lib/settle.ts` decides from position and flick speed whether it ends open. Escape, the scrim and Android back close it. Its header covers the top of the page and slides away with `headerHidden` |
| `components/BookCover.tsx` | Cover in a 2:3 box with a blurred backdrop; icon when there's no cover |
| `components/CenteredScreen.tsx`, `ScreenHeader.tsx`, `ProgressBar.tsx` | Shared pieces: a full-screen centred layout (with `safe-area`), the 48px title bar at the top of a screen, a 3px position bar (the `header` variant uses the UI's background and foreground colours) |
| `components/ChooseFolder.tsx` | App-only folder picker button (dialog plugin on desktop, `pick_folder` on Android) |
| `components/settings/` | `sections.ts` (settings definitions), `ChoiceRow.tsx` (a label and description beside a `Select`), `InterfaceSection.tsx` (app only: the interface scale), `LibrarySection.tsx` (library list in the app, *Rebuild index* everywhere), `LogsSection.tsx` (app only: log-file count, copy, save) |
| `components/ui/` | shadcn components (button, input, popover, select, sheet). Add more with `pnpm dlx shadcn@latest add <name>` from `frontend/` |
| `lib/` | Hooks and pure logic (appearance, foliate-js layout and page turns in `foliate.ts`, gestures, drawer settling, TOC, book titles, progress saving, reader zoom, the interface scale in `uiScale.ts`, platform flags, app libraries in `useAppLibrary.ts`, logging in `log.ts`, the Android back button in `backButton.ts`) |
| `gen/` | Generated proto code. Git-ignored, never hand-edit |
| `foliate-js.d.ts` | Types for the parts of foliate-js that Shelf uses |

The `@/*` import alias points at `src/` (`paths` in `tsconfig.app.json` plus `resolve.alias` in `vite.config.ts`; no `baseUrl`). `foliate-js` is aliased to `vendor/foliate-js`.

## Generated proto code

`pnpm gen:proto` runs `buf generate` from the repo root with `protoc-gen-es` (`buf.gen.yaml`, `target=js+dts`) into `src/gen/`. `dev`, `build` and `test` run it first. JS + `.d.ts` rather than TS because protobuf-es emits TS `enum`s, which `erasableSyntaxOnly` rejects. Messages are used as `create(Schema, init)`, `toJson`, `fromJson`; enum values as `ReaderFlow_Value.PAGINATED`.

## The backend layer (`src/api`)

| File | What |
|---|---|
| `types.ts` | `Backend`: `listBooks`, `listDirectories`, `getProgress`, `setProgress`, `getState`, `setState`, `getConfig`, `setConfig`, `rebuildIndex`, `getCover`, `getBookFile` |
| `http.ts` | `httpBackend`: `fetch` to same-origin `/api`. Encodes with `toJson(..., { useProtoFieldName: true })`, decodes with `ignoreUnknownFields` |
| `tauri.ts` | `tauriBackend`: the same calls over `invoke`. Also exports `invokeCommand`, which turns command rejections into `ApiError` |
| `index.ts` | `api = IS_TAURI ? tauriBackend : httpBackend`, plus each function as a named export |
| `error.ts` | `ApiError(status, message)`, thrown by both backends |
| `app.ts` | App-only commands: library list management, log settings and export, storage permission, folder picker, insets, `onLibraryChanged` |

Components import from `@/api` and don't know which backend they're on. App-only code imports from `@/api/app`.

`getCover` and `getBookFile` return `Blob`s in both backends.

## Platform flags (`src/lib/platform.ts`)

```ts
export const IS_TAURI = isTauri()                                         // inside the app
export const IS_ANDROID = IS_TAURI && /android/i.test(navigator.userAgent)
```

Evaluated once at load. This is the switch for app-specific behaviour. Current uses: backend choice, the router, `AppFlow`, system insets, the library switcher in the library slideover, the library list in Settings → Library, the interface scale (Settings → Interface and the reader), the *Developer options* settings section, forwarding logs to Rust, the folder picker, the storage-permission step and the back button on Android.

## App flow (`App.tsx`, app only)

1. `useSystemInsets()` publishes system bar sizes as CSS variables.
2. Subscribes to `library-changed` → invalidates `["dirs"]` and `["books"]`.
3. `useLibraries()` (`list_libraries`) and `useCurrentLibrary()`: the stored `current` if it's still in the list, else the first library. Pending → spinner; error → `OpenFailed`; no library → `PickLibrary`.
4. Opens the current library with a query keyed by `["open-library", name]` (`staleTime`/`gcTime: Infinity`, no retry). Pending → `Syncing`; error → `OpenFailed` (*Try again*, *Remove this library*, or switch to another).
5. `<MemoryRouter key={name}><Home/></MemoryRouter>`: in memory and keyed, so each library starts at `/` and no URL or folder survives a switch.

### Libraries (`lib/useAppLibrary.ts`)

- **Add** (`AddLibraryForm`: name, then a folder or a server URL; used by `PickLibrary` on first run and by Settings → Library → *Add library*). The name is required and checked against the list (ignoring case) before Rust checks it again. Choosing a folder or leaving the URL field pre-fills an empty name with the folder name or host. Adding switches to the new library.
- **Switch** (`LibrarySwitcher`, in its own strip just above the library slideover's bottom bar (settings gear), opening upward; also on `OpenFailed`): each entry shows the name and its source (folder path or server URL).
- **Remove** (red icon per row in Settings → Library, or *Remove this library* on `OpenFailed`): an inline `RemoveLibraryConfirm` explains that nothing in the library is deleted.

Switching, adding, and removing the open library all call `resetLibraryCache`: it removes **every cached query except `["libraries"]`** (books, folders, last book, covers, the open-book file and `["library-config"]`), then refetches the list. `AppFlow` then opens the new current library, and Home remounts and loads that library's own data and **config** (theme, layout, margins). Removing a library that isn't open only refetches the list.

## Home and reading

- **State**: `["state"]` → `getState`. `lastBookId` is the open book. Picking a book calls `setState` and writes the result into the cache.
- **Library drawer** (left): `LibraryPanel` with queries `["dirs", folder]` and `["books", folder]`. The current folder is in memory only.
- **Contents drawer** (right): only when a book is open.
- **Reader**: query `["open-book", id]` loads the file and the saved progress together (progress is read once, when the book opens; nothing refetches it while reading). foliate's `View` opens a `File` (it needs a name ending in `.epub`), then `init({ lastLocation: cfi, showTextStart: true })`.
- **Progress**: `progressSaver` saves the CFI 2 s after page turns stop, and flushes immediately on `pagehide`, when the page becomes hidden, and on unmount. On the web the flush uses `fetch(..., { keepalive: true })` so it survives the tab closing; IPC has no equivalent and ignores it.
- **Input**: paginated — foliate handles swipes; ← → and the buttons turn pages. Scrolled — native scroll with a 3px section line under the header instead of a scrollbar; a one-finger horizontal swipe (not while pinch-zoomed), horizontal wheel, ← → or the buttons change section. Keys, swipes and the wheel are ignored while a drawer or settings is open. Buttons float at the sides for mouse users and are always in the bottom bar.
- **Bars**: the header (`SlideDrawer`'s `header`, which covers the top of the page) and the reader's bottom bar (the 3px book line along its top edge, then ‹, the current chapter, % through the book, the zoom button, ›) float over the book, so showing or hiding them never re-lays it out. `Home` holds whether they show (`bars`), starting shown and reset when another book opens. The header hides only while a book is open. Reading hides them: a touch that moves 10px or more, the wheel in scrolled mode, a reading key (arrows, Page Up/Down, Space) outside a control, or a floating button. A tap or click on the page (not on a link, not ending a text selection) toggles them. The bar's own buttons leave them up. While hidden, the 3px lines stay at the edges: the section line moves from under the header to the top edge, the book line to the bottom edge; the book's padding keeps text clear of them. The lines use the UI's background and foreground colours, so they show whatever the reader's colours.
- **Zoom** (`lib/readerZoom.ts`): the bar's *Aa* button opens a popup with −, the level, + and reset, stepping 10% at a time from 50% to 200%. A level saved under older steps snaps to the nearest one (`snapZoom`). It's per device, like browser zoom, so it's kept in `localStorage` (`shelf.reader-zoom`, falling back to 100% when storage is blocked) rather than the library's config. `readerCss` sets CSS `zoom` on each page's `<body>` (not `<html>`, whose columns foliate sizes in px), so text and spacing scale and the book re-lays out. The zoom applied is the interface scale times this level, rounded to a hundredth, so the popup's % is relative to the interface. Images, videos and top-level SVGs get the inverse zoom, because foliate caps their height to the page in px and a zoomed body would push them past it: they keep their size, fitted to the page. The popup closes with the bars, since taps inside the book's iframes don't reach its outside-click check.
- **Settings** is a route (`/settings/:section?`) rendered over Home, which stays mounted (`inert`) behind it. Escape closes it, unless an open select took the Escape.

## Settings and appearance

- `["library-config"]` → `getConfig`, read once; `setConfig` results are written straight into that cache.
- `resolveAppearance` (`lib/appearance.ts`) turns `Config` into concrete values; unset fields mean the proto zero value. `useAppearance` also toggles the `dark` class on `<html>`.
- `components/settings/sections.ts` defines sections. A section has `groups` of `Setting`s (each bound to a `Config` field via `read`/`patch`, shown as a `ChoiceRow`), a `content` component for settings that aren't in the library config, or both (`content` renders after the groups). *Library* (`LibrarySection.tsx`) is shown everywhere: *Rebuild index* for all (on success it invalidates `["dirs"]`, `["books"]` and `["state"]`, since IDs can change), and, only when `IS_TAURI`, the list of libraries (red remove icon per row, *Add library* at the bottom). App-only settings go behind `IS_TAURI` inside a section's component, with their hooks in a separate component so the web never calls app commands.
- A section marked `appOnly` (today *Developer options*, slug `developer`, `CodeXml` icon) is left out of `VISIBLE_SECTIONS`, which `Settings.tsx` renders, so the web never shows it or calls its commands.
- *Developer options* (`LogsSection.tsx`), under a *Logs* heading: *Log files to keep*, a number field (1 to `max_keep_files`) saved on Enter or when the field loses focus, with an inline error for anything else (query `["log-settings"]`); *Copy logs* and *Save logs…*; and *Files*, every log file newest first (query `["log-files"]`, refetched after the count changes), shown by the time span it covers (start to last write) and its size, with the file name small underneath, a save button per row (`save_log_file`), and the folder on desktop. The count is stored on the device, not in the library config.
- **Interface scale** (`lib/uiScale.ts`, app only): Settings → Interface → *This device* → *Scale*, 80–150% in 10% steps (`InterfaceSection.tsx`). It's stored in the app data on the device (`ui_scale` / `set_ui_scale`), not in the library config. It sets the root font size (`<html style="font-size: 120%">`). Tailwind and shadcn size in rem, so the whole UI scales, while px borders, the 3px lines, the `--inset-*` safe-area padding and viewport units don't. CSS `zoom` on the root would scale viewport units too, and Tauri's webview zoom doesn't work on Android. `main.tsx` awaits `loadUiScale` before the first render, so nothing paints at 100% first, and seeds `["ui-scale"]`, which a library switch keeps. `useUiScale()` gives the factor (1 on the web). The reader multiplies its zoom by it and widens the floating buttons' text inset (`BUTTON_INSET`) to match.
- Library settings live in the library's `.shelf/config.json`, so they're shared by every device using that library.

## Logging (`lib/log.ts`)

- `log.debug/info/warn/error(message, fields?)` always writes to the console. In the app, it also sends `message key=value …` to Rust through `@tauri-apps/plugin-log`, which lands in the app's log files. Forwarding failures are swallowed, so logging never throws. Never pass secrets.
- `installErrorHandlers()` (called in `main.tsx`) forwards uncaught `error` and `unhandledrejection` events, with the stack. It doesn't log them to the console again, since the browser already does.
- Use `log` instead of `console.*` for anything worth keeping.

## Covers

`useCover(id)` (`lib/useCover.ts`) fetches the cover blob via `getCover` into query `["cover", id]` as an object URL (`staleTime: Infinity`, no retry; an error means "no cover", shown as an icon). `revokeCoversOnRemoval` (called in `main.tsx`) revokes the object URL when the query cache drops the entry, so blobs don't leak.

## Safe area (Android system bars)

The Android app draws edge to edge, so the page needs padding for the status bar, navigation bar and cutout:

- `index.css` defines `--inset-top/right/bottom/left` (default `0px`) and a Tailwind utility `safe-area` that pads by them.
- `useSystemInsets` (app only) sets those variables on `<html>` from the `system_insets` command on mount and on every `resize` (covers rotation).
- `safe-area` is applied to every full-screen surface: `SlideDrawer`'s root (with an inner `relative size-full` box so its absolutely positioned children respect the padding), the `Settings` root, and `CenteredScreen`. **Any new full-screen or `fixed inset-0` surface needs it too.**

On the web and desktop the variables stay `0px`.

## Android back button (`lib/backButton.ts`)

The app uses a `MemoryRouter`, so the WebView has no history of its own. Instead, each open layer registers a handler with `useBackButton(onBack, active)`, and back runs the newest one:

- An open drawer (`SlideDrawer`) closes.
- Settings (`Settings.tsx`) does what its header arrow does: a section on a phone goes back to the list; otherwise settings closes.

Tauri's `onBackButtonPress` takes the button over completely, so the listener is registered only while at least one layer is open and removed when the last one closes. With nothing open, Android's own behaviour applies (the app goes to the background). Opening settings from the library drawer stacks settings on top, so back closes settings and then the drawer. Pop-ups such as an open select aren't layers: back closes the screen under them. The permission comes with `core:default`.

## Tests and lint

```fish
pnpm --dir frontend test    # vitest; regenerates proto code first
pnpm --dir frontend lint    # oxlint (config: .oxlintrc.json; ignores vendor/, src/gen/, dist/)
pnpm --dir frontend exec tsc -b
pnpm --dir frontend build
```

Tests: `api/http.test.ts` (wire format, errors, ID escaping), `lib/appearance.test.ts`, `backButton.test.ts`, `gestures.test.ts`, `log.test.ts`, `progressSaver.test.ts`, `readerZoom.test.ts`, `settle.test.ts`, `toc.test.ts`. Linting is Oxlint, not ESLint.

## Favicon

`public/favicon.svg`, linked from `index.html`. It's the app icon drawing cropped for small sizes; the sources and how to regenerate the app icons are in [app.md](app.md#icons).

## Dev server

`pnpm --dir frontend dev` serves on port 5173 (fixed, `strictPort`) and proxies `/api` to `http://localhost:8080`, so in a browser it talks to a locally running `shelf-server`. When `TAURI_DEV_HOST` is set (by `cargo tauri android dev`), Vite listens on that host and serves HMR on port 1421, so the phone can reach it.
