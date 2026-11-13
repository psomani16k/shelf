import {
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"
import { ChevronRight, Folder, LoaderCircle } from "lucide-react"
import { type ReactNode, useState } from "react"
import {
  approveUserRequest,
  denyUserRequest,
  listScopeDirectories,
  listUserRequests,
  listUsers,
  setUserRole,
  setUserScope,
} from "@/api"
import { Breadcrumb } from "@/components/Breadcrumb"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import {
  Role_Value,
  type User,
  type UserRequest,
  type UserScope,
} from "@/gen/apipb/auth_pb"
import { assignableRoles, ROLE_NAMES, useMe } from "@/lib/useMe"

const USERS_KEY = ["users"]
const USER_REQUESTS_KEY = ["user-requests"]

/** Admins and owners only: users' roles, library access and sign-ups. */
export function AdminSection() {
  const users = useQuery({
    queryKey: USERS_KEY,
    queryFn: listUsers,
    select: (res) => res.users.map((user) => ({ user, scope: res.scopes[user.username] })),
  })
  const requests = useQuery({ queryKey: USER_REQUESTS_KEY, queryFn: listUserRequests })
  return (
    <div className="flex flex-col gap-6">
      <Card title="Users" query={users} empty="No users.">
        {({ user, scope }) => <UserRow key={user.username} user={user} scope={scope} />}
      </Card>
      <Card title="New user requests" query={requests} empty="No requests.">
        {(request) => <RequestRow key={request.id} request={request} />}
      </Card>
    </div>
  )
}

function Card<T>({
  title,
  query,
  empty,
  children,
}: {
  title: string
  query: UseQueryResult<T[]>
  empty: string
  children: (item: T) => ReactNode
}) {
  return (
    <section aria-label={title}>
      <h3 className="mb-2 px-1 font-medium">{title}</h3>
      {query.isPending ? (
        <LoaderCircle className="size-5 animate-spin text-muted-foreground" />
      ) : query.isError ? (
        <p className="text-sm text-destructive">{String(query.error)}</p>
      ) : query.data.length === 0 ? (
        <p className="rounded-lg border px-4 py-3 text-sm text-muted-foreground">{empty}</p>
      ) : (
        <ul className="divide-y rounded-lg border px-4">{query.data.map(children)}</ul>
      )}
    </section>
  )
}

function UserRow({ user, scope }: { user: User; scope?: UserScope }) {
  const queryClient = useQueryClient()
  const me = useMe().data
  const roles = assignableRoles(me)
  // Yourself, owners, and admins unless you're an owner.
  const canChange = user.username !== me?.username && roles.includes(user.role)
  const change = useMutation({
    mutationFn: (role: Role_Value) => setUserRole(user.username, role),
    onSettled: () => queryClient.invalidateQueries({ queryKey: USERS_KEY }),
  })
  const [editing, setEditing] = useState(false)

  return (
    <li className="py-3">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium">{user.username}</p>
          <p className="truncate text-sm text-muted-foreground">{describeScope(scope)}</p>
          {change.error && <p className="text-sm text-destructive">{change.error.message}</p>}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {canChange && !editing && (
            <Button variant="outline" onClick={() => setEditing(true)}>
              Access
            </Button>
          )}
          {canChange ? (
            <RoleSelect
              roles={roles}
              value={user.role}
              disabled={change.isPending}
              label={`Role of ${user.username}`}
              onChange={(role) => change.mutate(role)}
            />
          ) : (
            <p className="text-sm text-muted-foreground">{ROLE_NAMES[user.role]}</p>
          )}
        </div>
      </div>
      {editing && (
        <ScopeForm username={user.username} scope={scope} onDone={() => setEditing(false)} />
      )}
    </li>
  )
}

/** "Whole library", "kid", "kid · 2 book rules" */
function describeScope(scope?: UserScope) {
  const root = scope?.libraryRoot || "Whole library"
  const rules = scope?.books.length ?? 0
  return rules ? `${root} · ${rules} book rule${rules === 1 ? "" : "s"}` : root
}

/** The part of the library a user sees: a folder, and globs for the books in it. */
function ScopeForm({
  username,
  scope,
  onDone,
}: {
  username: string
  scope?: UserScope
  onDone: () => void
}) {
  const queryClient = useQueryClient()
  const [root, setRoot] = useState(scope?.libraryRoot ?? "")
  const [patterns, setPatterns] = useState((scope?.books ?? []).join("\n"))
  const save = useMutation({
    mutationFn: () => {
      const books = patterns
        .split("\n")
        .map((line) => line.trim())
        .filter(Boolean)
      return setUserScope(username, { libraryRoot: root, books })
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: USERS_KEY })
      onDone()
    },
  })

  return (
    <form
      className="mt-3 flex flex-col gap-4 rounded-lg bg-muted/40 p-3"
      onSubmit={(e) => {
        e.preventDefault()
        save.mutate()
      }}
    >
      <div>
        <p className="text-sm font-medium">Folder</p>
        <p className="text-sm text-muted-foreground">
          {username} sees this folder as their whole library.
        </p>
        <FolderPicker folder={root} onChange={setRoot} />
      </div>
      <label className="flex flex-col gap-1">
        <span className="text-sm font-medium">Books</span>
        <Textarea
          value={patterns}
          onChange={(e) => setPatterns(e.target.value)}
          placeholder={"**/*.epub\n!scary/**"}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          className="bg-background font-mono"
          disabled={save.isPending}
        />
        <span className="text-sm text-muted-foreground">
          Globs relative to the folder, one per line. <code>**/*.epub</code> is every book;{" "}
          <code>!scary/**</code> hides a folder. The last match wins. Empty: every book in the
          folder.
        </span>
      </label>
      {save.isError && <p className="text-sm text-destructive">{save.error.message}</p>}
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" onClick={onDone} disabled={save.isPending}>
          Cancel
        </Button>
        <Button type="submit" disabled={save.isPending}>
          Save
        </Button>
      </div>
    </form>
  )
}

/** Walks the whole library's folders; the picked folder is the one it's in. */
function FolderPicker({ folder, onChange }: { folder: string; onChange: (f: string) => void }) {
  const dirs = useQuery({
    queryKey: ["scope-dirs", folder],
    queryFn: () => listScopeDirectories(folder),
  })
  const pathOf = (name: string) => (folder ? `${folder}/${name}` : name)

  return (
    <div className="mt-2 rounded-lg border bg-background">
      <div className="border-b px-3 pb-2">
        <Breadcrumb folder={folder} rootLabel="Whole library" onNavigate={onChange} />
      </div>
      {dirs.isPending ? (
        <LoaderCircle className="m-3 size-4 animate-spin text-muted-foreground" />
      ) : dirs.isError ? (
        <p className="px-3 py-2 text-sm text-destructive">{dirs.error.message}</p>
      ) : dirs.data.length === 0 ? (
        <p className="px-3 py-2 text-sm text-muted-foreground">No folders inside.</p>
      ) : (
        <ul className="max-h-48 overflow-y-auto py-1">
          {dirs.data.map((name) => (
            <li key={name}>
              <button
                type="button"
                className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-muted"
                onClick={() => onChange(pathOf(name))}
              >
                <Folder className="size-4 shrink-0 text-muted-foreground" />
                <span className="min-w-0 flex-1 truncate">{name}</span>
                <ChevronRight className="size-4 shrink-0 text-muted-foreground" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

function RequestRow({ request }: { request: UserRequest }) {
  const queryClient = useQueryClient()
  const roles = assignableRoles(useMe().data).filter((role) => role !== Role_Value.BANNED)
  const [role, setRole] = useState(Role_Value.READER)
  const refetch = async () => {
    await queryClient.invalidateQueries({ queryKey: USER_REQUESTS_KEY })
    await queryClient.invalidateQueries({ queryKey: USERS_KEY })
  }
  const approve = useMutation({
    mutationFn: () => approveUserRequest(request.id, role),
    onSettled: refetch,
  })
  const deny = useMutation({ mutationFn: () => denyUserRequest(request.id), onSettled: refetch })
  const busy = approve.isPending || deny.isPending
  const error = approve.error ?? deny.error

  return (
    <li className="flex flex-col gap-3 py-4 sm:flex-row sm:items-center sm:justify-between">
      <div className="min-w-0">
        <p className="truncate text-sm font-medium">{request.username}</p>
        <p className="text-sm text-muted-foreground">
          {request.taken ? "Username taken" : `Asked ${formatTime(request.created)}`}
        </p>
        {error && <p className="text-sm text-destructive">{error.message}</p>}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {!request.taken && (
          <>
            <RoleSelect
              roles={roles}
              value={role}
              disabled={busy}
              label={`Role for ${request.username}`}
              onChange={setRole}
            />
            <Button onClick={() => approve.mutate()} disabled={busy}>
              Approve
            </Button>
          </>
        )}
        <Button variant="outline" onClick={() => deny.mutate()} disabled={busy}>
          Deny
        </Button>
      </div>
    </li>
  )
}

function RoleSelect({
  roles,
  value,
  disabled,
  label,
  onChange,
}: {
  roles: Role_Value[]
  value: Role_Value
  disabled: boolean
  label: string
  onChange: (role: Role_Value) => void
}) {
  const choices = roles.map((role) => ({ value: role, label: ROLE_NAMES[role] }))
  return (
    <Select
      items={choices}
      value={value}
      disabled={disabled}
      onValueChange={(next) => {
        if (next !== null && next !== value) onChange(next)
      }}
    >
      <SelectTrigger className="w-28" aria-label={label}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {choices.map((choice) => (
          <SelectItem key={choice.value} value={choice.value}>
            {choice.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

function formatTime(rfc3339: string) {
  const date = new Date(rfc3339)
  if (Number.isNaN(date.getTime())) return "at an unknown time"
  return date.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })
}
