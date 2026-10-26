import * as plugin from "@tauri-apps/plugin-log"
import { IS_TAURI } from "@/lib/platform"

type Level = "debug" | "info" | "warn" | "error"
type Fields = Record<string, unknown>

/**
 * Logs to the console and, in the app, to the app's log files through Rust.
 * Never pass passwords, tokens or anything else secret.
 */
export const log = {
  debug: (message: string, fields?: Fields) => write("debug", message, fields),
  info: (message: string, fields?: Fields) => write("info", message, fields),
  warn: (message: string, fields?: Fields) => write("warn", message, fields),
  error: (message: string, fields?: Fields) => write("error", message, fields),
}

/** Logs errors nothing else caught. The browser already prints them to the console. */
export function installErrorHandlers() {
  window.addEventListener("error", (e) => {
    const error: unknown = e.error ?? e.message
    forward("error", "uncaught error", {
      error,
      stack: error instanceof Error ? error.stack : undefined,
      source: `${e.filename}:${e.lineno}:${e.colno}`,
    })
  })
  window.addEventListener("unhandledrejection", (e) => {
    const reason: unknown = e.reason
    forward("error", "unhandled rejection", {
      error: reason,
      stack: reason instanceof Error ? reason.stack : undefined,
    })
  })
}

function write(level: Level, message: string, fields?: Fields) {
  if (fields) console[level](message, fields)
  else console[level](message)
  forward(level, message, fields)
}

function forward(level: Level, message: string, fields?: Fields) {
  if (!IS_TAURI) return
  // Logging must never throw or reject into the caller.
  plugin[level](withFields(message, fields)).catch(() => {})
}

/** `message key=value …`: the Rust side keeps only the text of a frontend record. */
export function withFields(message: string, fields?: Fields): string {
  if (!fields) return message
  const parts = Object.entries(fields)
    .filter(([, value]) => value !== undefined)
    .map(([key, value]) => `${key}=${describe(value)}`)
  return [message, ...parts].join(" ")
}

function describe(value: unknown): string {
  if (value instanceof Error) return JSON.stringify(`${value.name}: ${value.message}`)
  try {
    return JSON.stringify(value) ?? String(value)
  } catch {
    return String(value)
  }
}
