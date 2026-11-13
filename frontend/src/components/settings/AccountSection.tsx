import { useMutation, useQueryClient } from "@tanstack/react-query"
import { LoaderCircle } from "lucide-react"
import { useState } from "react"
import { changePassword, logout } from "@/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type { User } from "@/gen/apipb/auth_pb"
import { IS_TAURI } from "@/lib/platform"
import { LIBRARIES_KEY } from "@/lib/useAppLibrary"
import { ROLE_NAMES, useMe } from "@/lib/useMe"

/** Who's signed in, changing the password and signing out. A local library has no accounts. */
export function AccountSection() {
  const me = useMe()
  if (me.isPending) return <LoaderCircle className="size-5 animate-spin text-muted-foreground" />
  if (me.isError) return <p className="text-sm text-destructive">{String(me.error)}</p>
  if (me.data === null) {
    return (
      <p className="text-sm text-muted-foreground">
        This library is a folder on this device, so it has no accounts.
      </p>
    )
  }
  return (
    <div className="flex flex-col gap-4">
      <SignedIn user={me.data} />
      <ChangePassword />
    </div>
  )
}

function SignedIn({ user }: { user: User }) {
  const queryClient = useQueryClient()
  const signOut = useMutation({
    mutationFn: logout,
    // The login screen replaces everything; the app's library list now shows it signed out.
    onSettled: () => queryClient.invalidateQueries({ queryKey: LIBRARIES_KEY }),
  })
  return (
    <div className="flex items-center justify-between gap-4 rounded-lg border px-4 py-3">
      <div className="min-w-0">
        <p className="truncate text-sm font-medium">{user.username}</p>
        <p className="text-sm text-muted-foreground">{ROLE_NAMES[user.role]}</p>
      </div>
      <Button
        variant="outline"
        className="shrink-0"
        onClick={() => signOut.mutate()}
        disabled={signOut.isPending}
      >
        Log out
      </Button>
    </div>
  )
}

function ChangePassword() {
  const [current, setCurrent] = useState("")
  const [next, setNext] = useState("")
  const [confirm, setConfirm] = useState("")
  const change = useMutation({
    mutationFn: () => changePassword(current, next),
    onSuccess: () => {
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
