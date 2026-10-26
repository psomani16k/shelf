# Frontend

`frontend/` is one React SPA used by both the web build (served by `shelf-server`) and the Tauri app.

Stack: React 19, Vite 8, TypeScript 6, Tailwind CSS v4, shadcn/ui on Base UI (Nova preset), TanStack Query v5, React Router v8, protobuf-es v2, lucide icons, foliate-js for rendering EPUBs.

## Layout (`frontend/src`)

| Path | What |
|---|---|
| `main.tsx` | Creates the `QueryClient`, hooks up cover URL revocation, renders `App` |
| `App.tsx` | Web: `<BrowserRouter><Home/>`. App: `AppFlow` (pick → open → `<MemoryRouter><Home/>`) |
| `api/` | The backend layer (below) |
| `screens/Home.tsx` | The main screen for both: library drawer, reader, contents drawer, settings route |
| `screens/Settings.tsx` | Settings overlay: sidebar on wide screens, list → section on phones |
| `screens/PickLibrary.tsx`, `OpenFailed.tsx`, `Syncing.tsx` | App-only screens before a library is open |
| `components/Reader.tsx` | foliate-js wrapper: open at saved position, input handling, progress saving |
| `components/LibraryPanel.tsx` | Folder/book grid with breadcrumb |
| `components/TocPanel.tsx` | Table of contents tree |
| `components/SlideDrawer.tsx` | Two-sided drawer that follows the finger; drives transforms through one CSS variable `--p` |
| `components/BookCover.tsx` | Cover in a 2:3 box with a blurred backdrop; icon when there's no cover |
| `components/ChooseFolder.tsx` | App-only folder picker button (dialog plugin on desktop, `pick_folder` on Android) |
| `components/settings/` | `sections.ts` (settings definitions), `LibrarySection.tsx` (app-only) |
| `components/ui/` | shadcn components (button, input, select, sheet). Add more with `pnpm dlx shadcn@latest add <name>` from `frontend/` |
| `lib/` | Hooks and pure logic (appearance, gestures, TOC, progress saving, platform flags) |
| `gen/` | Generated proto code. Git-ignored, never hand-edit |
| `foliate-js.d.ts` | Types for the parts of foliate-js that Shelf uses |

The `@/*` import alias points at `src/` (`paths` in `tsconfig.app.json` plus `resolve.alias` in `vite.config.ts`; no `baseUrl`). `foliate-js` is aliased to `vendor/foliate-js`.

## Generated proto code

`pnpm gen:proto` runs `buf generate` from the repo root with `protoc-gen-es` (`buf.gen.yaml`, `target=js+dts`) into `src/gen/`. `dev`, `build` and `test` run it first. JS + `.d.ts` rather than TS because protobuf-es emits TS `enum`s, which `erasableSyntaxOnly` rejects. Messages are used as `create(Schema, init)`, `toJson`, `fromJson`; enum values as `ReaderFlow_Value.PAGINATED`.

## The backend layer (`src/api`)

| File | What |
|---|---|
| `types.ts` | `Backend`: `listBooks`, `listDirectories`, `getProgress`, `setProgress`, `getState`, `setState`, `getConfig`, `setConfig`, `getCover`, `getBookFile` |
| `http.ts` | `httpBackend`: `fetch` to same-origin `/api`. Encodes with `toJson(..., { useProtoFieldName: true })`, decodes with `ignoreUnknownFields` |
| `tauri.ts` | `tauriBackend`: the same calls over `invoke`. Also exports `invokeCommand`, which turns command rejections into `ApiError` |
| `index.ts` | `api = IS_TAURI ? tauriBackend : httpBackend`, plus each function as a named export |
| `error.ts` | `ApiError(status, message)`, thrown by both backends |
| `app.ts` | App-only commands: library list management, storage permission, folder picker, insets, `onLibraryChanged` |

Components import from `@/api` and don't know which backend they're on. App-only code imports from `@/api/app`.

`getCover` and `getBookFile` return `Blob`s in both backends.

## Platform flags (`src/lib/platform.ts`)

```ts
export const IS_TAURI = isTauri()                                         // inside the app
export const IS_ANDROID = IS_TAURI && /android/i.test(navigator.userAgent)
```

Evaluated once at load. This is the switch for app-specific behaviour. Current uses: backend choice, the router, `AppFlow`, the app-only *Library* settings section, the folder picker and the storage-permission step on Android.

## App flow (`App.tsx`, app only)

1. `useSystemInsets()` publishes system bar sizes as CSS variables.
2. Subscribes to `library-changed` → invalidates `["dirs"]` and `["books"]`.
3. `useLibraries()` (`list_libraries`). Pending → spinner; error → `OpenFailed`; none → `PickLibrary`.
4. Opens library 0 with a query keyed by `["open-library", kind, location]` (`staleTime`/`gcTime: Infinity`, no retry). Pending → `Syncing`; error → `OpenFailed`.
5. `<MemoryRouter><Home/></MemoryRouter>`: in memory so each library starts at `/` and no URL survives a change of library.

`PickLibrary` offers a local folder (on Android it first asks for storage access) or a server URL. `useForgetLibrary()` (`OpenFailed`, Settings → Library → *Change library*) calls `removeLibrary(0)`, drops every cached query except `["libraries"]`, and refetches the list, which sends the app back to `PickLibrary`.

## Home and reading

- **State**: `["state"]` → `getState`. `lastBookId` is the open book. Picking a book calls `setState` and writes the result into the cache.
- **Library drawer** (left): `LibraryPanel` with queries `["dirs", folder]` and `["books", folder]`. The current folder is in memory only.
- **Contents drawer** (right): only when a book is open.
- **Reader**: query `["open-book", id]` loads the file and the saved progress together (progress is read once, when the book opens; nothing refetches it while reading). foliate's `View` opens a `File` (it needs a name ending in `.epub`), then `init({ lastLocation: cfi })`.
- **Progress**: `progressSaver` saves the CFI 2 s after page turns stop, and flushes immediately on `pagehide`, when the page becomes hidden, and on unmount. On the web the flush uses `fetch(..., { keepalive: true })` so it survives the tab closing; IPC has no equivalent and ignores it.
- **Input**: paginated — foliate handles swipes; arrows and buttons turn pages. Scrolled — native scroll with a section progress bar; horizontal swipe, horizontal wheel, arrows or buttons change section. Buttons float at the sides for mouse users, sit in a footer on touch screens when paginated, and are hidden when scrolled on touch.
- **Settings** is a route (`/settings/:section?`) rendered over Home, which stays mounted (`inert`) behind it.

## Settings and appearance

- `["library-config"]` → `getConfig`, read once; `setConfig` results are written straight into that cache.
- `resolveAppearance` (`lib/appearance.ts`) turns `Config` into concrete values; unset fields mean the proto zero value. `useAppearance` also toggles the `dark` class on `<html>`.
- `components/settings/sections.ts` defines sections. A section is either `groups` of `Setting`s (each bound to a `Config` field via `read`/`patch`, shown as a `Select`) or a `content` component for settings that aren't in the library config. `APP_SECTIONS` (currently *Library*) is appended only when `IS_TAURI`. Add future app-only settings (e.g. Logs) there.
- Library settings live in the library's `.shelf/config.json`, so they're shared by every device using that library.

## Covers

`useCover(id)` (`lib/useCover.ts`) fetches the cover blob via `getCover` into query `["cover", id]` as an object URL (`staleTime: Infinity`, no retry; an error means "no cover", shown as an icon). `revokeCoversOnRemoval` (called in `main.tsx`) revokes the object URL when the query cache drops the entry, so blobs don't leak.

## Safe area (Android system bars)

The Android app draws edge to edge, so the page needs padding for the status bar, navigation bar and cutout:

- `index.css` defines `--inset-top/right/bottom/left` (default `0px`) and a Tailwind utility `safe-area` that pads by them.
- `useSystemInsets` (app only) sets those variables on `<html>` from the `system_insets` command on mount and on every `resize` (covers rotation).
- `safe-area` is applied to every full-screen surface: `SlideDrawer`'s root (with an inner `relative size-full` box so its absolutely positioned children respect the padding), the `Settings` root, and `CenteredScreen`. **Any new full-screen or `fixed inset-0` surface needs it too.**

On the web and desktop the variables stay `0px`.

## Tests and lint

```fish
pnpm --dir frontend test    # vitest; regenerates proto code first
pnpm --dir frontend lint    # oxlint (config: .oxlintrc.json; ignores vendor/, src/gen/, dist/)
pnpm --dir frontend exec tsc -b
pnpm --dir frontend build
```

Tests: `api/http.test.ts` (wire format, errors, ID escaping), `lib/appearance.test.ts`, `gestures.test.ts`, `progressSaver.test.ts`, `settle.test.ts`, `toc.test.ts`. Linting is Oxlint, not ESLint.

## Favicon

`public/favicon.svg`, linked from `index.html`. It's the app icon drawing cropped for small sizes; the sources and how to regenerate the app icons are in [app.md](app.md#icons).

## Dev server

`pnpm --dir frontend dev` serves on port 5173 (fixed, `strictPort`) and proxies `/api` to `http://localhost:8080`, so in a browser it talks to a locally running `shelf-server`. When `TAURI_DEV_HOST` is set (by `cargo tauri android dev`), Vite listens on that host and serves HMR on port 1421, so the phone can reach it.
