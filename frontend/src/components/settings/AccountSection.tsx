import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { LoaderCircle } from "lucide-react"
import { useState } from "react"
import { changePassword, listSessions, logout, signOutSession } from "@/api"
import { NameWithBadge } from "@/components/settings/NameWithBadge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type { SessionInfo } from "@/gen/apipb/auth_pb"
import { DATE_TIME } from "@/lib/format"
import { IS_TAURI } from "@/lib/platform"
import { LIBRARIES_KEY, SESSIONS_KEY } from "@/lib/queryKeys"
import { useCurrentLibrary } from "@/lib/useAppLibrary"
import { ROLE_NAMES, useMe } from "@/lib/useMe"

/**
 * Who's signed in, changing the password, signing out, and the devices signed in. A local library
 * has no accounts, only the name chosen on this device.
 */
export function AccountSection() {
  const me = useMe()
  if (me.isPending) return <LoaderCircle className="size-5 animate-spin text-muted-foreground" />
  if (me.isError) return <p className="text-sm text-destructive">{String(me.error)}</p>
  if (me.data === null) return <LocalReader />
  return (
    <div className="flex flex-col gap-4">
      <AccountRow
        name={me.data.username}
        description={ROLE_NAMES[me.data.role]}
        action="Log out"
      />
      <ChangePassword />
      <Sessions />
    </div>
  )
}

/** App only: a local library's reader, with no password or role. */
function LocalReader() {
  return (
    <AccountRow
      name={useCurrentLibrary()?.user}
      description="Reading this folder on this device. No password: anyone using it can switch."
      action="Switch user"
    />
  )
}

/** Who's signed in, and a button that signs them out. */
function AccountRow({
  name,
  description,
  action,
}: {
  name: string | undefined
  description: string
  action: string
}) {
  const logOut = useLogout()
  return (
    <div className="flex items-center justify-between gap-4 rounded-lg border px-4 py-3">
      <div className="min-w-0">
        <p className="truncate text-sm font-medium">{name}</p>
        <p className="text-sm text-muted-foreground">{description}</p>
      </div>
      <Button
        variant="outline"
        className="shrink-0"
        onClick={() => logOut.mutate()}
        disabled={logOut.isPending}
      >
        {action}
      </Button>
    </div>
  )
}

function useLogout() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: logout,
    // Shows the login screen, or for a local library the one that asks who's reading.
    onSettled: () => queryClient.invalidateQueries({ queryKey: LIBRARIES_KEY }),
  })
}

function ChangePassword() {
  const queryClient = useQueryClient()
  const [current, setCurrent] = useState("")
  const [next, setNext] = useState("")
  const [confirm, setConfirm] = useState("")
  const change = useMutation({
    mutationFn: () => changePassword(current, next),
    onSuccess: () => {
      // The other sessions have ended.
      void queryClient.invalidateQueries({ queryKey: SESSIONS_KEY })
      setCurrent("")
      setNext("")
      setConfirm("")
    },
  })
  const mismatch = !!confirm && next !== confirm
  const canSubmit = !!current && !!next && next === confirm && !change.isPending

  return (
    <form
      className="flex flex-col gap-3 rounded-lg border p-4"
      onSubmit={(e) => {
        e.preventDefault()
        if (canSubmit) change.mutate()
      }}
    >
      <div>
        <p className="text-sm font-medium">Change password</p>
        <p className="text-sm text-muted-foreground">
          {IS_TAURI ? "Other devices" : "Other devices and browsers"} signed in as you will have
          to sign in again.
        </p>
      </div>
      {/* Lets password managers save the new password for the right account. */}
      <input type="text" autoComplete="username" hidden readOnly />
      <Input
        type="password"
        value={current}
        onChange={(e) => setCurrent(e.target.value)}
        placeholder="Current password"
        aria-label="Current password"
        autoComplete="current-password"
      />
      <Input
        type="password"
        value={next}
        onChange={(e) => setNext(e.target.value)}
        placeholder="New password"
        aria-label="New password"
        autoComplete="new-password"
      />
      <Input
        type="password"
        value={confirm}
        onChange={(e) => setConfirm(e.target.value)}
        placeholder="New password again"
        aria-label="New password again"
        autoComplete="new-password"
        aria-invalid={mismatch}
      />
      {mismatch && <p className="text-sm text-destructive">The new passwords don't match.</p>}
      {change.isError && <p className="text-sm text-destructive">{change.error.message}</p>}
      {change.isSuccess && <p className="text-sm">Password changed.</p>}
      <div className="flex justify-end">
        <Button type="submit" disabled={!canSubmit}>
          {change.isPending ? "Changing…" : "Change password"}
        </Button>
      </div>
    </form>
  )
}

/** Every device signed in as this user, with *Sign out* for the others. */
function Sessions() {
  const queryClient = useQueryClient()
  const sessions = useQuery({ queryKey: SESSIONS_KEY, queryFn: listSessions })
  const signOut = useMutation({
    mutationFn: signOutSession,
    onSuccess: (left) => queryClient.setQueryData(SESSIONS_KEY, left),
  })

  return (
    <div className="flex flex-col gap-3 rounded-lg border p-4">
      <div>
        <p className="text-sm font-medium">Signed-in devices</p>
        <p className="text-sm text-muted-foreground">
          Devices and browsers signed in as you. One you sign out has to sign in again.
        </p>
      </div>
      {sessions.isPending && <LoaderCircle className="size-5 animate-spin text-muted-foreground" />}
      {sessions.isError && <p className="text-sm text-destructive">{String(sessions.error)}</p>}
      {sessions.data && (
        <ul className="flex flex-col divide-y">
          {sessions.data.map((session) => (
            <SessionRow
              key={session.id}
              session={session}
              busy={signOut.isPending && signOut.variables === session.id}
              onSignOut={() => signOut.mutate(session.id)}
            />
          ))}
        </ul>
      )}
      {signOut.isError && <p className="text-sm text-destructive">{signOut.error.message}</p>}
    </div>
  )
}

function SessionRow({
  session,
  busy,
  onSignOut,
}: {
  session: SessionInfo
  busy: boolean
  onSignOut: () => void
}) {
  const date = (rfc3339: string) => DATE_TIME.format(new Date(rfc3339))
  return (
    <li className="flex items-center justify-between gap-4 py-3 first:pt-0 last:pb-0">
      <div className="min-w-0">
        <NameWithBadge
          name={session.device || "Unknown device"}
          badge={session.current && "This device"}
        />
        <p className="text-sm text-muted-foreground">
          Signed in {date(session.created)} · last used {date(session.active)}
        </p>
      </div>
      {!session.current && (
        <Button variant="outline" className="shrink-0" onClick={onSignOut} disabled={busy}>
          {busy && <LoaderCircle className="animate-spin" />}
          Sign out
        </Button>
      )}
    </li>
  )
}
