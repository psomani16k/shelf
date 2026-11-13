use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use shelf_core::library::{
    LibraryError,
    names::folder_path,
    scope::{EVERYTHING, Scope},
};
use shelf_proto::api::{
    ListDirectoriesRequest, ListDirectoriesResponse, ListUsersResponse, SetUserRoleRequest,
    SetUserScopeRequest, User, UserScope, role::Value as Role,
};
use shelf_utility::timeutil::Time;
use tracing::info;

use super::{Auth, CurrentUser};
use crate::{api::Lib, error::ApiErrorResponse};

/// Whether `actor` may give `role` to someone, or take it from them.
pub(super) fn may_manage(actor: Role, role: Role) -> bool {
    match role {
        Role::Banned | Role::Reader | Role::Contributor | Role::Librarian => actor >= Role::Admin,
        Role::Admin => actor == Role::Owner,
        Role::Owner => false,
    }
}

pub(super) fn cant_give(role: Role) -> LibraryError {
    LibraryError::Forbidden(format!("you can't give the {} role", role.as_str_name()))
}

fn cant_change_own(what: &str) -> LibraryError {
    LibraryError::Forbidden(format!("you can't change your own {what}"))
}

fn cant_change(what: &str, role: Role) -> LibraryError {
    LibraryError::Forbidden(format!("you can't change an {}'s {what}", role.as_str_name()))
}

/// A scope from the client as it's stored: the root normalized, blank patterns dropped.
fn normalize(scope: UserScope) -> Result<UserScope, LibraryError> {
    let root = folder_path(&scope.library_root)?.to_string_lossy().into_owned();
    let books: Vec<String> = scope
        .books
        .iter()
        .map(|b| b.trim())
        .filter(|b| !b.is_empty())
        .map(str::to_owned)
        .collect();
    Scope::new(&root, &books).map_err(|e| LibraryError::Invalid(format!("{e:#}")))?;
    Ok(UserScope {
        library_root: root,
        books,
    })
}

impl Auth {
    /// Sorted by name: `users` is a `BTreeMap`.
    pub async fn list_users(&self, now: Time) -> ListUsersResponse {
        let mut inner = self.inner.lock().await;
        inner.file.refresh(now).await;
        let data = inner.file.data();
        let users = data
            .users
            .iter()
            .map(|(name, user)| User {
                username: name.clone(),
                role: user.role,
            })
            .collect();
        let scopes = data
            .users
            .iter()
            .map(|(name, user)| {
                let scope = UserScope {
                    library_root: user.library_root.clone(),
                    books: user.books.clone(),
                };
                (name.clone(), scope)
            })
            .collect();
        ListUsersResponse { users, scopes }
    }

    /// Banning also ends every session of the user.
    pub async fn set_role(
        &self,
        actor: &CurrentUser,
        req: &SetUserRoleRequest,
        now: Time,
    ) -> Result<(), LibraryError> {
        if req.username == actor.id {
            return Err(cant_change_own("role"));
        }
        let role = req.role();
        if !may_manage(actor.role, role) {
            return Err(cant_give(role));
        }
        let mut inner = self.inner.lock().await;
        inner
            .file
            .try_update(now, |data| -> Result<_, LibraryError> {
                let user = data.users.get_mut(&req.username).ok_or(LibraryError::NotFound)?;
                let current = user.role();
                if !may_manage(actor.role, current) {
                    return Err(cant_change("role", current));
                }
                user.set_role(role);
                if role == Role::Banned {
                    user.sessions.clear();
                }
                Ok(())
            })
            .await??;
        info!(user = %actor.id, changed_user = %req.username, role = role.as_str_name(), "role changed");
        Ok(())
    }

    /// `scope` must already be normalized. Same rule as roles: who may give the user's current
    /// role may change their scope.
    async fn set_scope(
        &self,
        actor: &CurrentUser,
        username: &str,
        scope: UserScope,
        now: Time,
    ) -> Result<(), LibraryError> {
        if username == actor.id {
            return Err(cant_change_own("library access"));
        }
        let mut inner = self.inner.lock().await;
        inner
            .file
            .try_update(now, |data| -> Result<_, LibraryError> {
                let user = data.users.get_mut(username).ok_or(LibraryError::NotFound)?;
                if !may_manage(actor.role, user.role()) {
                    return Err(cant_change("library access", user.role()));
                }
                user.library_root = scope.library_root;
                user.books = scope.books;
                Ok(())
            })
            .await??;
        info!(user = %actor.id, changed_user = %username, "library scope changed");
        Ok(())
    }
}

pub async fn list(State(auth): State<Arc<Auth>>) -> Json<ListUsersResponse> {
    Json(auth.list_users(Time::now()).await)
}

pub async fn set_role(
    State(auth): State<Arc<Auth>>,
    user: CurrentUser,
    Json(req): Json<SetUserRoleRequest>,
) -> Result<StatusCode, ApiErrorResponse> {
    auth.set_role(&user, &req, Time::now()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The root must be a folder in the library. An admin's own scope doesn't limit this.
pub async fn set_scope(
    State(auth): State<Arc<Auth>>,
    State(library): State<Lib>,
    user: CurrentUser,
    Json(req): Json<SetUserScopeRequest>,
) -> Result<StatusCode, ApiErrorResponse> {
    let scope = normalize(req.scope.unwrap_or_default())?;
    if !library.view(&EVERYTHING).has_folder(&scope.library_root).await? {
        let message = format!("there's no folder {:?}", scope.library_root);
        return Err(LibraryError::Invalid(message).into());
    }
    auth.set_scope(&user, &req.username, scope, Time::now()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The whole library's folders, for picking a user's root.
pub async fn list_directories(
    State(library): State<Lib>,
    Json(req): Json<ListDirectoriesRequest>,
) -> Json<ListDirectoriesResponse> {
    Json(library.view(&EVERYTHING).list_sub_directories(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::tests::{auth, is_status, now, stored};

    fn actor(id: &str, role: Role) -> CurrentUser {
        CurrentUser {
            id: id.into(),
            role,
            scope: Default::default(),
            session: String::new(),
        }
    }

    fn admin() -> CurrentUser {
        actor("boss", Role::Admin)
    }

    fn owner() -> CurrentUser {
        actor("parth", Role::Owner)
    }

    fn set(username: &str, role: Role) -> SetUserRoleRequest {
        SetUserRoleRequest {
            username: username.into(),
            role: role.into(),
        }
    }

    fn scope(root: &str, books: &[&str]) -> UserScope {
        UserScope {
            library_root: root.into(),
            books: books.iter().map(|b| b.to_string()).collect(),
        }
    }

    async fn scope_of(auth: &Auth, name: &str) -> UserScope {
        auth.list_users(now()).await.scopes.remove(name).unwrap()
    }

    async fn role_of(auth: &Auth, name: &str) -> Role {
        let users = auth.list_users(now()).await.users;
        users.iter().find(|u| u.username == name).unwrap().role()
    }

    #[tokio::test]
    async fn an_admin_can_promote_ban_and_unban_a_reader() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        let token = auth.login("kid", "pancakes", now()).await.unwrap().token;

        auth.set_role(&admin(), &set("kid", Role::Librarian), now()).await.unwrap();
        assert_eq!(role_of(&auth, "kid").await, Role::Librarian);
        assert!(auth.authenticate(&token, now()).await.is_ok());

        auth.set_role(&admin(), &set("kid", Role::Banned), now()).await.unwrap();
        assert!(is_status(
            auth.authenticate(&token, now()).await,
            StatusCode::UNAUTHORIZED
        ));
        let banned = auth.login("kid", "pancakes", now()).await;
        assert!(is_status(banned, StatusCode::FORBIDDEN));

        auth.set_role(&admin(), &set("kid", Role::Reader), now()).await.unwrap();
        // The old session stays gone.
        assert!(auth.authenticate(&token, now()).await.is_err());
        assert!(auth.login("kid", "pancakes", now()).await.is_ok());
    }

    #[tokio::test]
    async fn an_admin_cant_touch_admins_or_owners() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        auth.set_role(&owner(), &set("kid", Role::Admin), now()).await.unwrap();
        let before = stored(dir.path());

        let attempts = [
            set("kid", Role::Reader),
            set("parth", Role::Banned),
            set("nopass", Role::Admin),
            set("nopass", Role::Owner),
        ];
        for req in attempts {
            let result = auth.set_role(&admin(), &req, now()).await;
            assert!(is_status(result, StatusCode::FORBIDDEN), "{}", req.username);
        }

        assert_eq!(stored(dir.path()), before);
    }

    #[tokio::test]
    async fn an_owner_can_make_and_ban_an_admin_but_not_an_owner() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;

        auth.set_role(&owner(), &set("kid", Role::Admin), now()).await.unwrap();
        assert_eq!(role_of(&auth, "kid").await, Role::Admin);
        auth.set_role(&owner(), &set("kid", Role::Banned), now()).await.unwrap();
        assert_eq!(role_of(&auth, "kid").await, Role::Banned);

        let make_owner = auth.set_role(&owner(), &set("kid", Role::Owner), now()).await;
        assert!(is_status(make_owner, StatusCode::FORBIDDEN));
    }

    #[tokio::test]
    async fn no_one_changes_their_own_role_and_unknown_users_are_404() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;

        let own = auth.set_role(&owner(), &set("parth", Role::Admin), now()).await;
        assert!(is_status(own, StatusCode::FORBIDDEN));
        let missing = auth.set_role(&owner(), &set("ghost", Role::Reader), now()).await;
        assert!(is_status(missing, StatusCode::NOT_FOUND));
    }

    #[tokio::test]
    async fn users_are_listed_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;

        let users = auth.list_users(now()).await.users;

        let names: Vec<_> = users.iter().map(|u| u.username.as_str()).collect();
        assert_eq!(names, ["banned", "kid", "nopass", "parth"]);
        assert_eq!(users[0].role(), Role::Banned);
    }

    #[tokio::test]
    async fn an_admin_scopes_a_reader_and_the_root_is_stored_normalized() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;

        let stored = normalize(scope("/kid/books/", &[" **/*.epub ", "", "!scary/**"])).unwrap();
        auth.set_scope(&admin(), "kid", stored, now()).await.unwrap();

        assert_eq!(scope_of(&auth, "kid").await, scope("kid/books", &["**/*.epub", "!scary/**"]));
        assert_eq!(scope_of(&auth, "parth").await, scope("", &[]));
    }

    #[test]
    fn bad_roots_and_patterns_are_400() {
        for bad in [scope("../x", &[]), scope(".shelf", &[]), scope("", &["a["]), scope("", &["!"])] {
            let root = bad.library_root.clone();
            assert!(is_status(normalize(bad), StatusCode::BAD_REQUEST), "{root}");
        }
    }

    #[tokio::test]
    async fn scopes_follow_the_role_rule_and_never_your_own() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        auth.set_role(&owner(), &set("kid", Role::Admin), now()).await.unwrap();
        let before = stored(dir.path());

        for (actor, name) in [(admin(), "kid"), (admin(), "parth"), (owner(), "parth")] {
            let result = auth.set_scope(&actor, name, scope("x", &[]), now()).await;
            assert!(is_status(result, StatusCode::FORBIDDEN), "{} {name}", actor.id);
        }
        assert_eq!(stored(dir.path()), before);

        auth.set_scope(&owner(), "kid", scope("x", &[]), now()).await.unwrap();
        assert_eq!(scope_of(&auth, "kid").await.library_root, "x");
        let missing = auth.set_scope(&owner(), "ghost", scope("", &[]), now()).await;
        assert!(is_status(missing, StatusCode::NOT_FOUND));
    }
}
