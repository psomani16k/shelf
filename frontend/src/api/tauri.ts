import {
  create,
  type DescMessage,
  fromJson,
  type JsonValue,
  type MessageInitShape,
  type MessageShape,
  toJson,
} from "@bufbuild/protobuf"
import { invoke } from "@tauri-apps/api/core"
import {
  ListBooksRequestSchema,
  ListBooksResponseSchema,
  ListDirectoriesRequestSchema,
  ListDirectoriesResponseSchema,
} from "@/gen/apipb/list_pb"
import {
  GetProgressRequestSchema,
  GetProgressResponseSchema,
  SetProgressRequestSchema,
} from "@/gen/apipb/progress_pb"
import {
  GetStateRequestSchema,
  GetStateResponseSchema,
  SetStateRequestSchema,
} from "@/gen/apipb/state_pb"
import { ConfigSchema } from "@/gen/configpb/config_pb"
import { ApiError } from "./error"
import type { Backend } from "./types"

const decodeOptions = { ignoreUnknownFields: true }

/** The app's commands mirror the server's routes, and speak the same proto JSON. */
export const tauriBackend: Backend = {
  listBooks: (directoryPath) =>
    command("list_books", ListBooksRequestSchema, { directoryPath }, ListBooksResponseSchema),
  listDirectories: (directoryPath) =>
    command(
      "list_directories",
      ListDirectoriesRequestSchema,
      { directoryPath },
      ListDirectoriesResponseSchema,
    ),
  getProgress: (id) =>
    command("get_progress", GetProgressRequestSchema, { id }, GetProgressResponseSchema),
  // No `keepalive`: IPC has no page to outlive.
  setProgress: async (id, epubProgress) => {
    await invokeCommand("set_progress", { req: encode(SetProgressRequestSchema, { id, epubProgress }) })
  },
  getState: () => command("get_state", GetStateRequestSchema, {}, GetStateResponseSchema),
  setState: async (lastBookId) => {
    await invokeCommand("set_state", { req: encode(SetStateRequestSchema, { lastBookId }) })
  },
  getConfig: async () =>
    fromJson(ConfigSchema, await invokeCommand<JsonValue>("get_config"), decodeOptions),
  setConfig: async (patch) => {
    const json = await invokeCommand<JsonValue>("set_config", { patch: encode(ConfigSchema, patch) })
    return fromJson(ConfigSchema, json, decodeOptions)
  },
  // These commands return raw bytes, which arrive as an ArrayBuffer.
  getCover: async (id) =>
    new Blob([await invokeCommand<ArrayBuffer>("get_cover", { id })], { type: "image/jpeg" }),
  getBookFile: async (id) =>
    new Blob([await invokeCommand<ArrayBuffer>("get_book_file", { id })], {
      type: "application/epub+zip",
    }),
}

/** `invoke`, with a rejected command's `{ status, message }` turned into an `ApiError`. */
export async function invokeCommand<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(name, args)
  } catch (e) {
    throw toApiError(e)
  }
}

function toApiError(e: unknown): ApiError {
  if (typeof e === "object" && e !== null && "status" in e && "message" in e) {
    return new ApiError(Number(e.status), String(e.message))
  }
  return new ApiError(500, String(e))
}

function encode<Desc extends DescMessage>(schema: Desc, init: MessageInitShape<Desc>): JsonValue {
  return toJson(schema, create(schema, init), { useProtoFieldName: true })
}

async function command<Req extends DescMessage, Res extends DescMessage>(
  name: string,
  reqSchema: Req,
  req: MessageInitShape<Req>,
  resSchema: Res,
): Promise<MessageShape<Res>> {
  const json = await invokeCommand<JsonValue>(name, { req: encode(reqSchema, req) })
  return fromJson(resSchema, json, decodeOptions)
}
