use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use shelf_core::library::{LibraryError, user::validate_user};
use shelf_proto::{
    api::{
        ApproveUserRequestRequest, DenyUserRequestRequest, ListUserRequestsResponse, SignUpRequest,
        UserRequest, role::Value as Role,
    },
    server::{ServerData, ServerUser, ServerUserOnboardingRequests},
};
use shelf_utility::timeutil::Time;
use tracing::info;

use super::{
    Auth, CurrentUser, check_new_password, random_string,
    users::{cant_give, may_manage},
};
use crate::error::ApiErrorResponse;

/// Sign-up is open to anyone, so this caps how much a stranger can add to `data.json`.
const MAX_PENDING_REQUESTS: usize = 20;

impl Auth {
    /// Names are checked against `users` only; pending requests may share one.
    pub async fn sign_up(&self, req: &SignUpRequest, now: Time) -> Result<(), LibraryError> {
        if validate_user(&req.username).is_err() {
            return Err(LibraryError::Invalid(
                "a username is 1 to 32 of a–z, 0–9, - and _".to_owned(),
            ));
        }
        check_new_password(&req.password)?;
        let request = ServerUserOnboardingRequests {
            user_name: req.username.clone(),
            password: req.password.clone(),
            id: random_string(16)?,
            created: now.to_string(),
        };
        let mut inner = self.inner.lock().await;
        inner
            .file
            .try_update(now, |data| {
                if data.users.contains_key(&request.user_name) {
                    return Err(taken());
                }
                if data.user_requests.len() >= MAX_PENDING_REQUESTS {
                    return Err(LibraryError::TooManyRequests(
                        "too many accounts are waiting for approval; try again later".to_owned(),
                    ));
                }
                data.user_requests.push(request);
                Ok(())
            })
            .await??;
        // Not the name: it comes from an `/api/auth/*` body.
        info!("account requested");
        Ok(())
    }

    pub async fn list_requests(&self, now: Time) -> ListUserRequestsResponse {
        let mut inner = self.inner.lock().await;
        inner.file.refresh(now).await;
        let data = inner.file.data();
        let requests = data
            .user_requests
            .iter()
            .map(|r| UserRequest {
                id: r.id.clone(),
                username: r.user_name.clone(),
                created: r.created.clone(),
                taken: data.users.contains_key(&r.user_name),
            })
            .collect();
        ListUserRequestsResponse { requests }
    }

    /// Checks the name is still free in the file as it is now, so two users never share one.
    pub async fn approve_request(
        &self,
        approver: &CurrentUser,
        req: &ApproveUserRequestRequest,
        now: Time,
    ) -> Result<(), LibraryError> {
        let role = req.role();
        if role == Role::Banned || !may_manage(approver.role, role) {
            return Err(cant_give(role));
        }
        let mut inner = self.inner.lock().await;
        let username = inner
            .file
            .try_update(now, |data| -> Result<_, LibraryError> {
                let i = find(data, &req.id)?;
                let name = &data.user_requests[i].user_name;
                if validate_user(name).is_err() {
                    return Err(LibraryError::Invalid(
                        "the username isn't valid; deny the request".to_owned(),
                    ));
                }
                if data.users.contains_key(name) {
                    return Err(LibraryError::Conflict(
                        "this username is already taken; deny the request".to_owned(),
                    ));
                }
                let request = data.user_requests.remove(i);
                let user = ServerUser {
                    password: request.password,
                    role: role.into(),
                    ..Default::default()
                };
                data.users.insert(request.user_name.clone(), user);
                Ok(request.user_name)
            })
            .await??;
        info!(user = %approver.id, new_user = %username, role = role.as_str_name(), "account approved");
        Ok(())
    }

    pub async fn deny_request(
        &self,
        approver: &CurrentUser,
        req: &DenyUserRequestRequest,
        now: Time,
    ) -> Result<(), LibraryError> {
        let mut inner = self.inner.lock().await;
        let username = inner
            .file
            .try_update(now, |data| -> Result<_, LibraryError> {
                let i = find(data, &req.id)?;
                Ok(data.user_requests.remove(i).user_name)
            })
            .await??;
        info!(user = %approver.id, new_user = %username, "account request denied");
        Ok(())
    }
}

/// A request added by hand without an id can't be picked.
fn find(data: &ServerData, id: &str) -> Result<usize, LibraryError> {
    data.user_requests
        .iter()
        .position(|r| !r.id.is_empty() && r.id == id)
        .ok_or(LibraryError::NotFound)
}

fn taken() -> LibraryError {
    LibraryError::Conflict("this username is already taken".to_owned())
}

pub async fn sign_up(
    State(auth): State<Arc<Auth>>,
    Json(req): Json<SignUpRequest>,
) -> Result<StatusCode, ApiErrorResponse> {
    auth.sign_up(&req, Time::now()).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list(State(auth): State<Arc<Auth>>) -> Json<ListUserRequestsResponse> {
    Json(auth.list_requests(Time::now()).await)
}

pub async fn approve(
    State(auth): State<Arc<Auth>>,
    user: CurrentUser,
    Json(req): Json<ApproveUserRequestRequest>,
) -> Result<StatusCode, ApiErrorResponse> {
    auth.approve_request(&user, &req, Time::now()).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn deny(
    State(auth): State<Arc<Auth>>,
    user: CurrentUser,
    Json(req): Json<DenyUserRequestRequest>,
) -> Result<StatusCode, ApiErrorResponse> {
    auth.deny_request(&user, &req, Time::now()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::tests::{auth, is_status, now, stored};

    fn approver(role: Role) -> CurrentUser {
        CurrentUser {
            id: "boss".into(),
            role,
            scope: Default::default(),
            session: String::new(),
        }
    }

    fn sign_up(username: &str, password: &str) -> SignUpRequest {
        SignUpRequest {
            username: username.into(),
            password: password.into(),
        }
    }

    fn approval(id: &str, role: Role) -> ApproveUserRequestRequest {
        ApproveUserRequestRequest {
            id: id.into(),
            role: role.into(),
        }
    }

    async fn pending(auth: &Auth) -> Vec<UserRequest> {
        auth.list_requests(now()).await.requests
    }

    #[tokio::test]
    async fn a_taken_or_bad_name_or_password_is_refused_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        let before = stored(dir.path());

        let taken = auth.sign_up(&sign_up("parth", "pw"), now()).await;
        assert!(is_status(taken, StatusCode::CONFLICT));
        for name in ["Parth", "", "a b", "../x"] {
            let bad = auth.sign_up(&sign_up(name, "pw"), now()).await;
            assert!(is_status(bad, StatusCode::BAD_REQUEST), "{name}");
        }
        let no_password = auth.sign_up(&sign_up("newbie", ""), now()).await;
        assert!(is_status(no_password, StatusCode::BAD_REQUEST));

        assert_eq!(stored(dir.path()), before);
    }

    #[tokio::test]
    async fn pending_requests_are_capped() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        for i in 0..MAX_PENDING_REQUESTS {
            auth.sign_up(&sign_up(&format!("new{i}"), "pw"), now()).await.unwrap();
        }

        let one_more = auth.sign_up(&sign_up("onemore", "pw"), now()).await;
        assert!(is_status(one_more, StatusCode::TOO_MANY_REQUESTS));
    }

    #[tokio::test]
    async fn an_approved_request_becomes_a_user_who_can_sign_in() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        auth.sign_up(&sign_up("newbie", "secret"), now()).await.unwrap();

        let waiting = auth.login("newbie", "secret", now()).await;
        assert!(is_status(waiting, StatusCode::FORBIDDEN));
        let wrong = auth.login("newbie", "nope", now()).await;
        assert!(is_status(wrong, StatusCode::UNAUTHORIZED));

        let request = pending(&auth).await.remove(0);
        assert_eq!(request.username, "newbie");
        assert!(!request.taken);
        let approver = approver(Role::Admin);
        auth.approve_request(&approver, &approval(&request.id, Role::Librarian), now())
            .await
            .unwrap();

        assert!(pending(&auth).await.is_empty());
        let res = auth.login("newbie", "secret", now()).await.unwrap();
        assert_eq!(res.user.unwrap().role(), Role::Librarian);
    }

    #[tokio::test]
    async fn a_second_request_for_the_same_name_cant_be_approved() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        auth.sign_up(&sign_up("twin", "first"), now()).await.unwrap();
        auth.sign_up(&sign_up("twin", "second"), now()).await.unwrap();
        let approver = approver(Role::Admin);

        let first = pending(&auth).await.remove(0);
        auth.approve_request(&approver, &approval(&first.id, Role::Reader), now())
            .await
            .unwrap();

        let left = pending(&auth).await;
        assert_eq!(left.len(), 1);
        assert!(left[0].taken);
        let second = approval(&left[0].id, Role::Reader);
        let result = auth.approve_request(&approver, &second, now()).await;
        assert!(is_status(result, StatusCode::CONFLICT));
        assert_eq!(pending(&auth).await.len(), 1);
        assert!(auth.login("twin", "first", now()).await.is_ok());
        assert!(auth.login("twin", "second", now()).await.is_err());
    }

    #[tokio::test]
    async fn only_an_owner_can_give_the_admin_role() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        auth.sign_up(&sign_up("newbie", "secret"), now()).await.unwrap();
        let id = pending(&auth).await.remove(0).id;

        let as_admin = approval(&id, Role::Admin);
        let by_admin = auth.approve_request(&approver(Role::Admin), &as_admin, now()).await;
        assert!(is_status(by_admin, StatusCode::FORBIDDEN));
        for role in [Role::Owner, Role::Banned] {
            let req = approval(&id, role);
            let result = auth.approve_request(&approver(Role::Owner), &req, now()).await;
            assert!(is_status(result, StatusCode::FORBIDDEN), "{role:?}");
        }

        auth.approve_request(&approver(Role::Owner), &approval(&id, Role::Admin), now())
            .await
            .unwrap();
        let res = auth.login("newbie", "secret", now()).await.unwrap();
        assert_eq!(res.user.unwrap().role(), Role::Admin);
    }

    #[tokio::test]
    async fn deny_removes_the_request() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        auth.sign_up(&sign_up("newbie", "secret"), now()).await.unwrap();
        let id = pending(&auth).await.remove(0).id;
        let approver = approver(Role::Admin);
        let deny = DenyUserRequestRequest { id: id.clone() };

        auth.deny_request(&approver, &deny, now()).await.unwrap();

        assert!(pending(&auth).await.is_empty());
        assert!(!stored(dir.path()).contains("newbie"));
        let again = auth.deny_request(&approver, &deny, now()).await;
        assert!(is_status(again, StatusCode::NOT_FOUND));
        let approve = approval(&id, Role::Reader);
        let result = auth.approve_request(&approver, &approve, now()).await;
        assert!(is_status(result, StatusCode::NOT_FOUND));
        assert!(is_status(
            auth.login("newbie", "secret", now()).await,
            StatusCode::UNAUTHORIZED
        ));
    }
}
