import {
  type DescMessage,
  type JsonValue,
  type MessageInitShape,
  type MessageShape,
} from "@bufbuild/protobuf"
import { invoke } from "@tauri-apps/api/core"
import {
  ApproveUserRequestRequestSchema,
  ChangePasswordRequestSchema,
  DenyUserRequestRequestSchema,
  EndSessionRequestSchema,
  ListSessionsResponseSchema,
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
import {
  AddAnnotationRequestSchema,
  DeleteAnnotationRequestSchema,
  ListAnnotationsRequestSchema,
  ListAnnotationsResponseSchema,
  UpdateAnnotationRequestSchema,
} from "@/gen/apipb/annotation_pb"
import {
  DeleteThemeRequestSchema,
  ListThemesRequestSchema,
  ListThemesResponseSchema,
  RenameThemeRequestSchema,
  SetThemeRequestSchema,
} from "@/gen/apipb/theme_pb"
import {
  ExportUserDataRequestSchema,
  ImportUserDataRequestSchema,
  ImportUserDataResponseSchema,
  UserDataSchema,
} from "@/gen/apipb/user_data_pb"
import { AnnotationSchema } from "@/gen/bookkeeperpb/annotation_pb"
import { ConfigSchema } from "@/gen/configpb/config_pb"
import { endSession, setSignedOut } from "@/lib/session"
import { ApiError } from "./error"
import { decode, encode } from "./proto"
import type { Backend } from "./types"

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
  setProgress: (id, epubProgress) =>
    exec("set_progress", SetProgressRequestSchema, { id, epubProgress }),
  listAnnotations: (id) =>
    command(
      "list_annotations",
      ListAnnotationsRequestSchema,
      { id },
      ListAnnotationsResponseSchema,
    ),
  addAnnotation: (id, highlight) =>
    command("add_annotation", AddAnnotationRequestSchema, { id, ...highlight }, AnnotationSchema),
  updateAnnotation: (id, annotationId, patch) =>
    command(
      "update_annotation",
      UpdateAnnotationRequestSchema,
      { id, annotationId, ...patch },
      AnnotationSchema,
    ),
  deleteAnnotation: (id, annotationId) =>
    exec("delete_annotation", DeleteAnnotationRequestSchema, { id, annotationId }),
  getState: () => command("get_state", GetStateRequestSchema, {}, GetStateResponseSchema),
  setState: (lastBookId) => exec("set_state", SetStateRequestSchema, { lastBookId }),
  getConfig: () => command("get_config", GetConfigRequestSchema, {}, ConfigSchema),
  setConfig: (patch) => command("set_config", SetConfigRequestSchema, { patch }, ConfigSchema),
  listThemes: () => command("list_themes", ListThemesRequestSchema, {}, ListThemesResponseSchema),
  setTheme: (spec) =>
    command("set_theme", SetThemeRequestSchema, { spec }, ListThemesResponseSchema),
  deleteTheme: (name) =>
    command("delete_theme", DeleteThemeRequestSchema, { name }, ListThemesResponseSchema),
  renameTheme: (from, to) =>
    command("rename_theme", RenameThemeRequestSchema, { from, to }, ListThemesResponseSchema),
  exportUserData: () =>
    command("export_user_data", ExportUserDataRequestSchema, {}, UserDataSchema),
  importUserData: (data) =>
    command("import_user_data", ImportUserDataRequestSchema, { data }, ImportUserDataResponseSchema),
  rebuildIndex: () => query("rebuild_index", RebuildIndexResponseSchema),
  cleanUpProgress: () => query("clean_up_progress", CleanUpProgressResponseSchema),
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
    return decode(UserSchema, user)
  },
  logout: async () => {
    try {
      await invokeCommand("logout")
    } finally {
      endSession()
    }
  },
  me: async () => {
    const user = await invokeCommand<JsonValue | null>("me")
    return user === null ? null : decode(UserSchema, user)
  },
  changePassword: (currentPassword, newPassword) =>
    exec("change_password", ChangePasswordRequestSchema, { currentPassword, newPassword }),
  listSessions: async () => (await query("list_sessions", ListSessionsResponseSchema)).sessions,
  signOutSession: async (id) =>
    (await command("end_session", EndSessionRequestSchema, { id }, ListSessionsResponseSchema))
      .sessions,
  signUp: (username, password) => exec("sign_up", SignUpRequestSchema, { username, password }),
  listUserRequests: async () =>
    (await query("list_user_requests", ListUserRequestsResponseSchema)).requests,
  approveUserRequest: (id, role) =>
    exec("approve_user_request", ApproveUserRequestRequestSchema, { id, role }),
  denyUserRequest: (id) => exec("deny_user_request", DenyUserRequestRequestSchema, { id }),
  listUsers: () => query("list_users", ListUsersResponseSchema),
  setUserRole: (username, role) => exec("set_user_role", SetUserRoleRequestSchema, { username, role }),
  setUserScope: (username, scope) =>
    exec("set_user_scope", SetUserScopeRequestSchema, { username, scope }),
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
    if (done) return decode(UploadEbookResponseSchema, res)
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

/**
 * `invoke`, with a rejected command's `{ status, message }` turned into an `ApiError`. A 401 means
 * the remote library's key is no longer valid (except from `login` itself).
 */
export async function invokeCommand<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(name, args)
  } catch (e) {
    const error = toApiError(e)
    if (error.status === 401 && name !== "login") endSession()
    throw error
  }
}

function toApiError(e: unknown): ApiError {
  if (typeof e === "object" && e !== null && "status" in e && "message" in e) {
    return new ApiError(Number(e.status), String(e.message))
  }
  return new ApiError(500, String(e))
}

async function command<Req extends DescMessage, Res extends DescMessage>(
  name: string,
  reqSchema: Req,
  req: MessageInitShape<Req>,
  resSchema: Res,
): Promise<MessageShape<Res>> {
  const json = await invokeCommand<JsonValue>(name, { req: encode(reqSchema, req) })
  return decode(resSchema, json)
}

async function exec<Desc extends DescMessage>(
  name: string,
  schema: Desc,
  init: MessageInitShape<Desc>,
): Promise<void> {
  await invokeCommand(name, { req: encode(schema, init) })
}

async function query<Res extends DescMessage>(name: string, resSchema: Res): Promise<MessageShape<Res>> {
  return decode(resSchema, await invokeCommand<JsonValue>(name))
}
