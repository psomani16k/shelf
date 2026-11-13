import { useSyncExternalStore } from "react"
import { IS_TAURI } from "@/lib/platform"

/**
 * Whether the user has to sign in. On the web the server's login key is kept in this browser's
 * storage, so it stays signed in; the app keeps a remote library's key in Rust. Any 401 marks the
 * session signed out, which shows the login screen.
 */

const TOKEN_KEY = "shelf.token"

/** Web only. */
export function getToken(): string | null {
  try {
    return localStorage.getItem(TOKEN_KEY)
  } catch {
    return null
  }
}

/** Web only. `null` forgets the key. */
export function setToken(token: string | null) {
  try {
    if (token === null) localStorage.removeItem(TOKEN_KEY)
    else localStorage.setItem(TOKEN_KEY, token)
  } catch {
    // Private mode or blocked storage: signed in until the page reloads.
  }
}

// The app starts from its library list's `signed_in` instead.
let signedOut = !IS_TAURI && getToken() === null
const listeners = new Set<() => void>()

export function isSignedOut(): boolean {
  return signedOut
}

export function setSignedOut(value: boolean) {
  if (signedOut === value) return
  signedOut = value
  for (const listener of listeners) listener()
}

/** The web forgets its key too, so a reload doesn't send it again. */
export function endSession() {
  if (!IS_TAURI) setToken(null)
  setSignedOut(true)
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function useSignedOut(): boolean {
  return useSyncExternalStore(subscribe, isSignedOut)
}
