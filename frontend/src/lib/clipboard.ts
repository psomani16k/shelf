import { copyText as copyInApp } from "@/api/app"
import { IS_TAURI } from "@/lib/platform"

/** Through Rust in the app, where the webview's clipboard API isn't dependable. */
export const copyText = (text: string): Promise<void> =>
  IS_TAURI ? copyInApp(text) : navigator.clipboard.writeText(text)
