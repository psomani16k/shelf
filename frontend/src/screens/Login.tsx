import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"
import { ApiError, login, signUp } from "@/api"
import type { LibraryInfo } from "@/api/app"
import { CenteredScreen } from "@/components/CenteredScreen"
import { LibrarySwitcher } from "@/components/LibrarySwitcher"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { IS_TAURI } from "@/lib/platform"
import { resetLibraryCache, useLibraries } from "@/lib/useAppLibrary"

/**
 * Shown with no key or after a 401, with a mode for asking for an account. In the app, `library`
 * is the remote library to sign in to.
 */
export function Login({ library }: { library?: LibraryInfo }) {
  const queryClient = useQueryClient()
  const [creating, setCreating] = useState(false)
  const [username, setUsername] = useState("")
  const [password, setPassword] = useState("")
  const [confirm, setConfirm] = useState("")
  const [requested, setRequested] = useState(false)
  // Usernames are lowercase; phone keyboards capitalise the first letter.
  const name = username.trim().toLowerCase()

  const signIn = useMutation({
    // Nothing cached from before, which may have been another user, is shown. On the web it's
    // cleared before signing in: once signed in, `Home` mounts and its queries start, and
    // clearing then would cancel them and leave them pending for good.
    mutationFn: () => {
      if (!IS_TAURI) queryClient.clear()
      return login(name, password)
    },
    // The app's library screen isn't mounted behind the login screen, so this is safe after.
    onSuccess: () => (IS_TAURI ? resetLibraryCache(queryClient) : undefined),
  })
  const create = useMutation({
    mutationFn: () => signUp(name, password),
    onSuccess: () => {
      setRequested(true)
      setPassword("")
      setConfirm("")
      setCreating(false)
    },
  })

  const switchMode = () => {
    setCreating(!creating)
    setRequested(false)
    setConfirm("")
    signIn.reset()
    create.reset()
  }

  const mismatch = creating && !!confirm && password !== confirm
  const pending = signIn.isPending || create.isPending
  const canSubmit =
    !!name && !!password && !pending && (!creating || password === confirm)
  const error = creating ? create.error : signIn.error

  return (
    <CenteredScreen>
      <form
        className="flex w-full max-w-xs flex-col gap-3 text-left"
        onSubmit={(e) => {
          e.preventDefault()
          if (!canSubmit) return
          if (creating) create.mutate()
          else signIn.mutate()
        }}
      >
        <div className="mb-2 text-center">
          <h1 className="text-lg font-medium">{creating ? "Create an account" : "Sign in"}</h1>
          {library && (
            <p className="truncate text-sm text-muted-foreground">
              to {library.name} ({library.location})
            </p>
          )}
        </div>
        <Input
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          placeholder="Username"
          aria-label="Username"
          autoComplete="username"
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
          required
        />
        <Input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder="Password"
          aria-label="Password"
          autoComplete={creating ? "new-password" : "current-password"}
          required
        />
        {creating && (
          <Input
            type="password"
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
            placeholder="Password again"
            aria-label="Password again"
            autoComplete="new-password"
            aria-invalid={mismatch}
            required
          />
        )}
        {creating && (
          <p className="text-sm text-muted-foreground">
            An admin of this server has to approve the account before you can sign in.
          </p>
        )}
        {mismatch && <p className="text-sm text-destructive">The passwords don't match.</p>}
        {/* A 403 is about the account (banned, or waiting for approval), not a typo. */}
        {error &&
          (error instanceof ApiError && error.status === 403 ? (
            <p role="alert" className="rounded-lg border px-3 py-2 text-sm">
              {error.message}
            </p>
          ) : (
            <p className="text-sm text-destructive">{error.message}</p>
          ))}
        {requested && (
          <p className="text-sm">Request sent. You can sign in once an admin approves it.</p>
        )}
        <Button type="submit" disabled={!canSubmit}>
          {creating
            ? create.isPending
              ? "Sending…"
              : "Create account"
            : signIn.isPending
              ? "Signing in…"
              : "Sign in"}
        </Button>
        <Button type="button" variant="ghost" onClick={switchMode} disabled={pending}>
          {creating ? "Have an account? Sign in" : "No account? Create one"}
        </Button>
      </form>
      {library && <OtherLibraries />}
    </CenteredScreen>
  )
}

/** App only. */
function OtherLibraries() {
  const others = (useLibraries().data?.libraries.length ?? 0) > 1
  if (!others) return null
  return (
    <div className="flex w-full max-w-xs flex-col gap-1 text-left">
      <p className="text-sm text-muted-foreground">Or open another library:</p>
      <LibrarySwitcher className="rounded-lg border" />
    </div>
  )
}
