import { useQuery } from "@tanstack/react-query"
import { me } from "@/api"
import { type User, Role_Value } from "@/gen/apipb/auth_pb"

export const ME_KEY = ["me"]

export const ROLE_NAMES: Record<Role_Value, string> = {
  [Role_Value.BANNED]: "Banned",
  [Role_Value.READER]: "Reader",
  [Role_Value.EDITOR]: "Editor",
  [Role_Value.ADMIN]: "Admin",
  [Role_Value.OWNER]: "Owner",
}

/** The signed-in user; `null` for an app's local library, which has no accounts. */
export function useMe() {
  return useQuery({ queryKey: ME_KEY, queryFn: me, staleTime: Infinity })
}

/**
 * What the user's role allows. Without accounts (`null`) everything is, except managing users;
 * while loading, nothing.
 */
export function permissions(user: User | null | undefined) {
  const role = user === null ? Role_Value.OWNER : (user?.role ?? Role_Value.BANNED)
  const accountRole = user?.role ?? Role_Value.BANNED
  return {
    canEdit: role >= Role_Value.EDITOR,
    canAdmin: role >= Role_Value.ADMIN,
    canManageUsers: accountRole >= Role_Value.ADMIN,
  }
}

export function usePermissions() {
  return permissions(useMe().data)
}

/** The roles `user` may give or take away; the server's `may_manage`. */
export function assignableRoles(user: User | null | undefined): Role_Value[] {
  const role = user?.role ?? Role_Value.BANNED
  if (role < Role_Value.ADMIN) return []
  const roles = [Role_Value.READER, Role_Value.EDITOR, Role_Value.BANNED]
  return role === Role_Value.OWNER ? [...roles, Role_Value.ADMIN] : roles
}
