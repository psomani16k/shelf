import {
  create,
  type DescMessage,
  fromJson,
  type JsonValue,
  type MessageInitShape,
  type MessageShape,
  toJson,
} from "@bufbuild/protobuf"
import {
  ApproveUserRequestRequestSchema,
  ChangePasswordRequestSchema,
  DenyUserRequestRequestSchema,
  ListUserRequestsRequestSchema,
  ListUserRequestsResponseSchema,
  ListUsersRequestSchema,
  type ListUsersResponse,
  ListUsersResponseSchema,
  LoginRequestSchema,
  LoginResponseSchema,
  type Role_Value,
  SetUserRoleRequestSchema,
  SetUserScopeRequestSchema,
  SignUpRequestSchema,
  type User,
  type UserRequest,
  type UserScopeSchema,
  UserSchema,
} from "@/gen/apipb/auth_pb"
import { ApiErrorSchema } from "@/gen/apipb/error_pb"
import {
  CreateDirectoryRequestSchema,
  CreateDirectoryResponseSchema,
  DeleteItemsRequestSchema,
  type DeleteItemsResponse,
  DeleteItemsResponseSchema,
  CleanUpProgressRequestSchema,
  type CleanUpProgressResponse,
  CleanUpProgressResponseSchema,
  RebuildIndexRequestSchema,
  type RebuildIndexResponse,
  RebuildIndexResponseSchema,
  type UploadEbookResponse,
  UploadEbookResponseSchema,
} from "@/gen/apipb/library_pb"
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
import { GetConfigRequestSchema, SetConfigRequestSchema } from "@/gen/apipb/config_pb"
import { type Config, ConfigSchema } from "@/gen/configpb/config_pb"
import { endSession, getToken, setSignedOut, setToken } from "@/lib/session"
import { ApiError } from "./error"
import type { Backend, DeleteItems } from "./types"

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
  rebuildIndex,
  cleanUpProgress,
  getCover,
  getBookFile,
  uploadBook,
  createDirectory,
  deleteItems,
  login,
  logout,
  me,
  changePassword,
  signUp,
  listUserRequests,
  approveUserRequest,
  denyUserRequest,
  listUsers,
  setUserRole,
  setUserScope,
  listScopeDirectories,
}

/** Keeps the key, so this browser stays signed in. */
export async function login(username: string, password: string): Promise<User> {
  const res = await post(
    "/auth/login",
    LoginRequestSchema,
    { username, password },
    LoginResponseSchema,
  )
  setToken(res.token)
  setSignedOut(false)
  return res.user ?? create(UserSchema, { username })
}

/** Forgets the key even when the server can't be reached. */
export async function logout(): Promise<void> {
  try {
    await send("/auth/logout", { method: "POST" })
  } finally {
    endSession()
  }
}

export async function me(): Promise<User> {
  const res = await send("/auth/me")
  return fromJson(UserSchema, (await res.json()) as JsonValue, decodeOptions)
}

/** Every other device signed in as this user is signed out. */
export async function changePassword(currentPassword: string, newPassword: string): Promise<void> {
  await send("/auth/password", jsonBody(ChangePasswordRequestSchema, { currentPassword, newPassword }))
}

export async function signUp(username: string, password: string): Promise<void> {
  await send("/auth/signup", jsonBody(SignUpRequestSchema, { username, password }))
}

export async function listUserRequests(): Promise<UserRequest[]> {
  const res = await post(
    "/users/requests/list",
    ListUserRequestsRequestSchema,
    {},
    ListUserRequestsResponseSchema,
  )
  return res.requests
}

export async function approveUserRequest(id: string, role: Role_Value): Promise<void> {
  await send("/users/requests/approve", jsonBody(ApproveUserRequestRequestSchema, { id, role }))
}

export async function denyUserRequest(id: string): Promise<void> {
  await send("/users/requests/deny", jsonBody(DenyUserRequestRequestSchema, { id }))
}

export function listUsers(): Promise<ListUsersResponse> {
  return post("/users/list", ListUsersRequestSchema, {}, ListUsersResponseSchema)
}

export async function setUserRole(username: string, role: Role_Value): Promise<void> {
  await send("/users/role/set", jsonBody(SetUserRoleRequestSchema, { username, role }))
}

export async function setUserScope(
  username: string,
  scope: MessageInitShape<typeof UserScopeSchema>,
): Promise<void> {
  await send("/users/scope/set", jsonBody(SetUserScopeRequestSchema, { username, scope }))
}

export async function listScopeDirectories(directoryPath: string): Promise<string[]> {
  const res = await post(
    "/users/scope/directories",
    ListDirectoriesRequestSchema,
    { directoryPath },
    ListDirectoriesResponseSchema,
  )
  return res.subDirectories
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

/** The signed-in user's own settings. */
export function getConfig(): Promise<Config> {
  return post("/config/get", GetConfigRequestSchema, {}, ConfigSchema)
}

/** Only the fields set in `patch` change; resolves to the user's whole config. */
export function setConfig(patch: MessageInitShape<typeof ConfigSchema>): Promise<Config> {
  return post("/config/set", SetConfigRequestSchema, { patch }, ConfigSchema)
}

export function rebuildIndex(): Promise<RebuildIndexResponse> {
  return post("/library/rebuild", RebuildIndexRequestSchema, {}, RebuildIndexResponseSchema)
}

export function cleanUpProgress(): Promise<CleanUpProgressResponse> {
  return post(
    "/library/progress/clean",
    CleanUpProgressRequestSchema,
    {},
    CleanUpProgressResponseSchema,
  )
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

/**
 * The book is the raw body and the request is the query. `XMLHttpRequest`, because `fetch`
 * can't report upload progress.
 */
export function uploadBook(
  directory: string,
  file: File,
  onProgress?: (sentBytes: number) => void,
): Promise<UploadEbookResponse> {
  const query = new URLSearchParams({ parent_directory: directory, file_name: file.name })
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest()
    xhr.open("POST", `${BASE}/books/upload?${query}`)
    xhr.setRequestHeader("content-type", "application/epub+zip")
    const token = getToken()
    if (token) xhr.setRequestHeader("authorization", `Bearer ${token}`)
    xhr.upload.onprogress = (e) => onProgress?.(e.loaded)
    xhr.onerror = () => reject(new ApiError(0, "the upload failed: network error"))
    xhr.onload = () => {
      if (xhr.status < 200 || xhr.status >= 300) {
        if (xhr.status === 401) endSession()
        reject(apiErrorFrom(xhr.status, xhr.statusText, xhr.responseText))
        return
      }
      try {
        const json = JSON.parse(xhr.responseText) as JsonValue
        resolve(fromJson(UploadEbookResponseSchema, json, decodeOptions))
      } catch (e) {
        reject(e)
      }
    }
    xhr.send(file)
  })
}

export async function createDirectory(parent: string, name: string): Promise<void> {
  await post(
    "/directories/create",
    CreateDirectoryRequestSchema,
    { parentDirectory: parent, directoryName: name },
    CreateDirectoryResponseSchema,
  )
}

export function deleteItems(items: DeleteItems): Promise<DeleteItemsResponse> {
  return post("/library/delete", DeleteItemsRequestSchema, items, DeleteItemsResponseSchema)
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

/** Sends the login key; a 401 anywhere but login means it's no longer valid. */
async function send(path: string, init?: RequestInit): Promise<Response> {
  const headers = new Headers(init?.headers)
  const token = getToken()
  if (token) headers.set("authorization", `Bearer ${token}`)
  const res = await fetch(BASE + path, { ...init, headers })
  if (!res.ok) {
    if (res.status === 401 && path !== "/auth/login") endSession()
    throw await toApiError(res)
  }
  return res
}

async function toApiError(res: Response): Promise<ApiError> {
  return apiErrorFrom(res.status, res.statusText, await res.text().catch(() => ""))
}

function apiErrorFrom(status: number, statusText: string, body: string): ApiError {
  try {
    const error = fromJson(ApiErrorSchema, JSON.parse(body) as JsonValue, decodeOptions)
    return new ApiError(status, error.message || statusText)
  } catch {
    // Not an `ApiError` body, e.g. a proxy's error page.
    return new ApiError(status, statusText)
  }
}
