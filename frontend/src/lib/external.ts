import { openUrl } from "@tauri-apps/plugin-opener"
import { IS_TAURI } from "@/lib/platform"

/** Opens a web page in the device's default browser; on the web, in a new tab. */
export async function openExternal(url: string): Promise<void> {
  if (IS_TAURI) return openUrl(url)
  window.open(url, "_blank", "noopener,noreferrer")
}
