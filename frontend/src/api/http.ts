import {
  create,
  type DescMessage,
  fromJson,
  type JsonValue,
  type MessageInitShape,
  type MessageShape,
  toJson,
} from "@bufbuild/protobuf"
import { ApiErrorSchema } from "@/gen/apipb/error_pb"
import {
  ListBooksRequestSchema,
  type ListBooksResponse,
  ListBooksResponseSchema,
  ListDirectoriesRequestSchema,
  type ListDirectoriesResponse,
  ListDirectoriesResponseSchema,
} from "@/gen/apipb/list_pb"
import {
  GetProgressRequestSchema,
  type GetProgressResponse,
  GetProgressResponseSchema,
  SetProgressRequestSchema,
} from "@/gen/apipb/progress_pb"
import {
  GetStateRequestSchema,
  type GetStateResponse,
  GetStateResponseSchema,
  SetStateRequestSchema,
} from "@/gen/apipb/state_pb"
import { type Config, ConfigSchema } from "@/gen/configpb/config_pb"
import { ApiError } from "./error"
import type { Backend } from "./types"

export { ApiError }

// Same origin: the server serves this app, and `pnpm dev` proxies /api to it.
const BASE = "/api"
const decodeOptions = { ignoreUnknownFields: true }

export const httpBackend: Backend = {
  listBooks,
  listDirectories,
  getProgress,
  setProgress,
  getState,
  setState,
  getConfig,
  setConfig,
  getCover,
  getBookFile,
}

/** `directoryPath` is relative to the library root; `""` is the root. */
export function listBooks(directoryPath: string): Promise<ListBooksResponse> {
  return post("/books/list", ListBooksRequestSchema, { directoryPath }, ListBooksResponseSchema)
}

/** `directoryPath` is relative to the library root; `""` is the root. */
export function listDirectories(directoryPath: string): Promise<ListDirectoriesResponse> {
  return post(
    "/directories/list",
    ListDirectoriesRequestSchema,
    { directoryPath },
    ListDirectoriesResponseSchema,
  )
}

/** `epubProgress` is unset when the book hasn't been read yet. */
export function getProgress(id: string): Promise<GetProgressResponse> {
  return post("/progress/get", GetProgressRequestSchema, { id }, GetProgressResponseSchema)
}

/**
 * `epubProgress` is an EPUB CFI. `keepalive` lets the request finish after the tab closes.
 */
export async function setProgress(
  id: string,
  epubProgress: string,
  { keepalive = false } = {},
): Promise<void> {
  const init = jsonBody(SetProgressRequestSchema, { id, epubProgress })
  await send("/progress/set", { ...init, keepalive })
}

/** `lastBookId` is unset on first use, or when that book has left the library. */
export function getState(): Promise<GetStateResponse> {
  return post("/state/get", GetStateRequestSchema, {}, GetStateResponseSchema)
}

export async function setState(lastBookId: string): Promise<void> {
  await send("/state/set", jsonBody(SetStateRequestSchema, { lastBookId }))
}

export async function getConfig(): Promise<Config> {
  const res = await send("/config/get", { method: "POST" })
  return fromJson(ConfigSchema, (await res.json()) as JsonValue, decodeOptions)
}

/** Only the fields set in `patch` change; resolves to the whole config. */
export async function setConfig(patch: MessageInitShape<typeof ConfigSchema>): Promise<Config> {
  const res = await send("/config/set", jsonBody(ConfigSchema, patch))
  return fromJson(ConfigSchema, (await res.json()) as JsonValue, decodeOptions)
}

export async function getCover(id: string): Promise<Blob> {
  const res = await send(`/books/${encodeURIComponent(id)}/cover`)
  return res.blob()
}

/** The EPUB file, ready for foliate-js. */
export async function getBookFile(id: string): Promise<Blob> {
  const res = await send(`/books/${encodeURIComponent(id)}/file`)
  return res.blob()
}

async function post<Req extends DescMessage, Res extends DescMessage>(
  path: string,
  reqSchema: Req,
  req: MessageInitShape<Req>,
  resSchema: Res,
): Promise<MessageShape<Res>> {
  const res = await send(path, jsonBody(reqSchema, req))
  return fromJson(resSchema, (await res.json()) as JsonValue, decodeOptions)
}

function jsonBody<Desc extends DescMessage>(
  schema: Desc,
  init: MessageInitShape<Desc>,
): RequestInit {
  const json = toJson(schema, create(schema, init), { useProtoFieldName: true })
  return {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(json),
  }
}

async function send(path: string, init?: RequestInit): Promise<Response> {
  const res = await fetch(BASE + path, init)
  if (!res.ok) throw await toApiError(res)
  return res
}

async function toApiError(res: Response): Promise<ApiError> {
  try {
    const body = fromJson(ApiErrorSchema, (await res.json()) as JsonValue, decodeOptions)
    return new ApiError(res.status, body.message || res.statusText)
  } catch {
    // Not an `ApiError` body, e.g. a proxy's error page.
    return new ApiError(res.status, res.statusText)
  }
}
