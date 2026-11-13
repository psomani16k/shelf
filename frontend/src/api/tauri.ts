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
  ApproveUserRequestRequestSchema,
  ChangePasswordRequestSchema,
  DenyUserRequestRequestSchema,
  ListUserRequestsResponseSchema,
  ListUsersResponseSchema,
  LoginRequestSchema,
  SetUserRoleRequestSchema,
  SetUserScopeRequestSchema,
  SignUpRequestSchema,
  UserSchema,
} from "@/gen/apipb/auth_pb"
import {
  CreateDirectoryRequestSchema,
  CreateDirectoryResponseSchema,
  DeleteItemsRequestSchema,
  DeleteItemsResponseSchema,
  CleanUpProgressResponseSchema,
  RebuildIndexResponseSchema,
  type UploadEbookResponse,
  UploadEbookRequestSchema,
  UploadEbookResponseSchema,
} from "@/gen/apipb/library_pb"
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
import { GetConfigRequestSchema, SetConfigRequestSchema } from "@/gen/apipb/config_pb"
import { ConfigSchema } from "@/gen/configpb/config_pb"
import { setSignedOut } from "@/lib/session"
import { ApiError } from "./error"
import type { Backend } from "./types"

const decodeOptions = { ignoreUnknownFields: true }

/** Base64 in JSON: IPC on Linux and Android would send raw bytes as an array of numbers. */
const UPLOAD_CHUNK_BYTES = 2 * 1024 * 1024

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
  getConfig: () => command("get_config", GetConfigRequestSchema, {}, ConfigSchema),
  setConfig: (patch) => command("set_config", SetConfigRequestSchema, { patch }, ConfigSchema),
  rebuildIndex: async () =>
    fromJson(
      RebuildIndexResponseSchema,
      await invokeCommand<JsonValue>("rebuild_index"),
      decodeOptions,
    ),
  cleanUpProgress: async () =>
    fromJson(
      CleanUpProgressResponseSchema,
      await invokeCommand<JsonValue>("clean_up_progress"),
      decodeOptions,
    ),
  // These commands return raw bytes, which arrive as an ArrayBuffer.
  getCover: async (id) =>
    new Blob([await invokeCommand<ArrayBuffer>("get_cover", { id })], { type: "image/jpeg" }),
  getBookFile: async (id) =>
    new Blob([await invokeCommand<ArrayBuffer>("get_book_file", { id })], {
      type: "application/epub+zip",
    }),
  uploadBook,
  createDirectory: async (parent, name) => {
    await command(
      "create_directory",
      CreateDirectoryRequestSchema,
      { parentDirectory: parent, directoryName: name },
      CreateDirectoryResponseSchema,
    )
  },
  deleteItems: (items) =>
    command("delete_items", DeleteItemsRequestSchema, items, DeleteItemsResponseSchema),
  login: async (username, password) => {
    const req = encode(LoginRequestSchema, { username, password })
    const user = await invokeCommand<JsonValue>("login", { req })
    setSignedOut(false)
    return fromJson(UserSchema, user, decodeOptions)
  },
  logout: async () => {
    try {
      await invokeCommand("logout")
    } finally {
      setSignedOut(true)
    }
  },
  me: async () => {
    const user = await invokeCommand<JsonValue | null>("me")
    return user === null ? null : fromJson(UserSchema, user, decodeOptions)
  },
  changePassword: async (currentPassword, newPassword) => {
    const req = encode(ChangePasswordRequestSchema, { currentPassword, newPassword })
    await invokeCommand("change_password", { req })
  },
  signUp: async (username, password) => {
    await invokeCommand("sign_up", { req: encode(SignUpRequestSchema, { username, password }) })
  },
  listUserRequests: async () => {
    const res = await invokeCommand<JsonValue>("list_user_requests")
    return fromJson(ListUserRequestsResponseSchema, res, decodeOptions).requests
  },
  approveUserRequest: async (id, role) => {
    const req = encode(ApproveUserRequestRequestSchema, { id, role })
    await invokeCommand("approve_user_request", { req })
  },
  denyUserRequest: async (id) => {
    await invokeCommand("deny_user_request", { req: encode(DenyUserRequestRequestSchema, { id }) })
  },
  listUsers: async () => {
    const res = await invokeCommand<JsonValue>("list_users")
    return fromJson(ListUsersResponseSchema, res, decodeOptions)
  },
  setUserRole: async (username, role) => {
    const req = encode(SetUserRoleRequestSchema, { username, role })
    await invokeCommand("set_user_role", { req })
  },
  setUserScope: async (username, scope) => {
    const req = encode(SetUserScopeRequestSchema, { username, scope })
    await invokeCommand("set_user_scope", { req })
  },
  listScopeDirectories: async (directoryPath) => {
    const res = await command(
      "list_scope_directories",
      ListDirectoriesRequestSchema,
      { directoryPath },
      ListDirectoriesResponseSchema,
    )
    return res.subDirectories
  },
}

/** Sends the file in order, one chunk per command; the last one stores the book. */
async function uploadBook(
  directory: string,
  file: File,
  onProgress?: (sentBytes: number) => void,
): Promise<UploadEbookResponse> {
  const req = encode(UploadEbookRequestSchema, { parentDirectory: directory, fileName: file.name })
  const uploadId = crypto.randomUUID()
  let offset = 0
  for (;;) {
    const end = Math.min(offset + UPLOAD_CHUNK_BYTES, file.size)
    const done = end === file.size
    const data = await base64(file.slice(offset, end))
    const res = await invokeCommand<JsonValue>("upload_book", { req, uploadId, offset, data, done })
    offset = end
    onProgress?.(offset)
    if (done) return fromJson(UploadEbookResponseSchema, res, decodeOptions)
  }
}

/** Through a data URL, which WebKitGTK and Android's WebView both encode natively. */
function base64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => {
      const url = reader.result as string
      resolve(url.slice(url.indexOf(",") + 1))
    }
    reader.onerror = () => reject(reader.error ?? new Error("couldn't read the file"))
    reader.readAsDataURL(blob)
  })
}

/** `invoke`, with a rejected command's `{ status, message }` turned into an `ApiError`. */
/** A 401 means the remote library's key is no longer valid (except from `login` itself). */
export async function invokeCommand<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(name, args)
  } catch (e) {
    const error = toApiError(e)
    if (error.status === 401 && name !== "login") setSignedOut(true)
    throw error
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
