use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, anyhow};
use axum::{
    Json,
    extract::{FromRequestParts, Request, State},
    http::{HeaderMap, StatusCode, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use shelf_core::library::{LibraryError, scope::Scope};
use shelf_proto::{
    api::{ChangePasswordRequest, LoginRequest, LoginResponse, User, role::Value as Role},
    server::Session,
};
use shelf_utility::timeutil::{Duration, Instant, Time};
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::{data::DataFile, error::ApiErrorResponse};

pub mod requests;
pub mod users;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const SESSION_LIFETIME: Duration = Duration::from_secs(90 * 24 * 60 * 60);
/// Failed logins in a row before the user is locked out for `LOCK_TIME`.
const MAX_FAILURES: u32 = 5;
const LOCK_TIME: Duration = Duration::from_secs(60);
const MAX_PASSWORD_CHARS: usize = 256;
/// Compared against when the username is unknown, so the reply takes as long as for a real user.
const DUMMY_PASSWORD: &str = "not a real password, only here for the timing";

/// Users, their sessions and the login lockouts. One lock: every check may reload or write
/// `data.json`.
pub struct Auth {
    inner: Mutex<Inner>,
}

struct Inner {
    file: DataFile,
    failures: HashMap<String, Failures>,
}

#[derive(Default)]
struct Failures {
    count: u32,
    locked_until: Option<Instant>,
}

/// The signed-in user, put in the request's extensions by `authenticate`. No `Debug`, so the
/// session's hash can't end up in a log line.
#[derive(Clone)]
pub struct CurrentUser {
    pub id: String,
    pub role: Role,
    /// The part of the library they see.
    pub scope: Arc<Scope>,
    /// The hash of this request's token, the session's key in `data.json`.
    session: String,
}

/// Who made the request, for the access log; put in the response's extensions.
#[derive(Clone)]
pub struct LoggedUser(pub String);

impl Auth {
    pub fn new(file: DataFile) -> Self {
        Self {
            inner: Mutex::new(Inner {
                file,
                failures: HashMap::new(),
            }),
        }
    }

    pub async fn user_count(&self) -> usize {
        self.inner.lock().await.file.data().users.len()
    }

    pub async fn login(
        &self,
        username: &str,
        password: &str,
        now: Time,
    ) -> Result<LoginResponse, LibraryError> {
        let mut inner = self.inner.lock().await;
        inner.file.refresh(now).await;
        let found = inner.file.data().users.get(username);
        let Some((role, ok)) = found.map(|u| (u.role(), password_matches(&u.password, password)))
        else {
            let _ = password_matches(DUMMY_PASSWORD, password);
            let pending = inner.file.data().user_requests.iter();
            if pending
                .filter(|r| r.user_name == username)
                .any(|r| password_matches(&r.password, password))
            {
                info!("login refused: waiting for approval");
                return Err(LibraryError::Forbidden(
                    "this account is waiting for an admin's approval".to_owned(),
                ));
            }
            // Not the name: it may be a password typed into the wrong field.
            info!("login failed: unknown user");
            return Err(wrong_password());
        };
        inner.check_lock(username)?;
        if !ok {
            info!(user = username, "login failed: wrong password");
            inner.record_failure(username);
            return Err(wrong_password());
        }
        inner.failures.remove(username);
        if role == Role::Banned {
            info!(user = username, "login refused: banned");
            return Err(LibraryError::Forbidden(
                "this account is banned; contact an admin of this server".to_owned(),
            ));
        }

        let token = random_string(32)?;
        let session = Session {
            created: now.to_string(),
            expires: (now + SESSION_LIFETIME).to_string(),
        };
        let added = inner
            .file
            .update(now, |data| match data.users.get_mut(username) {
                Some(user) => {
                    user.sessions.insert(hash(&token), session);
                    true
                }
                None => false,
            })
            .await?;
        if !added {
            return Err(wrong_password());
        }
        info!(user = username, "logged in");
        Ok(LoginResponse {
            token,
            user: Some(User {
                username: username.to_owned(),
                role: role.into(),
            }),
        })
    }

    /// The session's user, with the role `data.json` gives them now. Pushes the expiry back to a
    /// full `SESSION_LIFETIME`, at most once a day.
    pub async fn authenticate(&self, token: &str, now: Time) -> Result<CurrentUser, LibraryError> {
        let session = hash(token);
        let mut inner = self.inner.lock().await;
        inner.file.refresh(now).await;
        let found = inner.file.data().users.iter().find_map(|(id, user)| {
            let expires = user.sessions.get(&session)?.expires.parse::<Time>().ok()?;
            Some((id.clone(), user.role(), expires))
        });
        let live = |(_, role, expires): &(String, Role, Time)| *role != Role::Banned && *expires > now;
        let Some((id, role, expires)) = found.filter(live) else {
            return Err(LibraryError::Unauthorized(
                "this session has ended; sign in again".to_owned(),
            ));
        };
        let scope = inner
            .file
            .scope(&id)
            .context("a signed-in user has no scope")?;

        if expires < now + (SESSION_LIFETIME - DAY) {
            let extended = inner
                .file
                .update(now, |data| {
                    let stored = data.users.get_mut(&id)?.sessions.get_mut(&session)?;
                    stored.expires = (now + SESSION_LIFETIME).to_string();
                    Some(())
                })
                .await;
            if let Err(e) = extended {
                warn!(error = %format!("{e:#}"), "couldn't extend a session");
            }
        }
        Ok(CurrentUser {
            id,
            role,
            scope,
            session,
        })
    }

    pub async fn logout(&self, user: &CurrentUser, now: Time) -> Result<(), LibraryError> {
        let mut inner = self.inner.lock().await;
        inner
            .file
            .update(now, |data| {
                if let Some(stored) = data.users.get_mut(&user.id) {
                    stored.sessions.remove(&user.session);
                }
            })
            .await?;
        info!(user = %user.id, "logged out");
        Ok(())
    }

    /// Keeps the caller's session and ends every other one of theirs.
    pub async fn change_password(
        &self,
        user: &CurrentUser,
        req: &ChangePasswordRequest,
        now: Time,
    ) -> Result<(), LibraryError> {
        check_new_password(&req.new_password)?;
        let mut inner = self.inner.lock().await;
        inner.file.refresh(now).await;
        inner.check_lock(&user.id)?;
        let ok = inner
            .file
            .data()
            .users
            .get(&user.id)
            .is_some_and(|u| password_matches(&u.password, &req.current_password));
        if !ok {
            info!(user = %user.id, "password change failed: wrong current password");
            inner.record_failure(&user.id);
            // Not a 401: that would sign the client out.
            return Err(LibraryError::Invalid(
                "the current password is wrong".to_owned(),
            ));
        }
        inner.failures.remove(&user.id);

        inner
            .file
            .update(now, |data| {
                if let Some(stored) = data.users.get_mut(&user.id) {
                    stored.password = req.new_password.clone();
                    stored.sessions.retain(|key, _| *key == user.session);
                }
            })
            .await?;
        info!(user = %user.id, "password changed");
        Ok(())
    }
}

impl Inner {
    fn check_lock(&self, user: &str) -> Result<(), LibraryError> {
        let locked = self
            .failures
            .get(user)
            .and_then(|f| f.locked_until)
            .is_some_and(|until| Instant::now() < until);
        if locked {
            return Err(LibraryError::TooManyRequests(
                "too many wrong passwords; try again in a minute".to_owned(),
            ));
        }
        Ok(())
    }

    fn record_failure(&mut self, user: &str) {
        let failures = self.failures.entry(user.to_owned()).or_default();
        failures.count += 1;
        if failures.count >= MAX_FAILURES {
            failures.count = 0;
            failures.locked_until = Some(Instant::now() + LOCK_TIME);
            warn!(user, "too many wrong passwords, login locked for a minute");
        }
    }
}

/// A user with no password set can't log in.
fn password_matches(stored: &str, given: &str) -> bool {
    let same: bool = stored.as_bytes().ct_eq(given.as_bytes()).into();
    same && !stored.is_empty()
}

fn wrong_password() -> LibraryError {
    LibraryError::Unauthorized("wrong username or password".to_owned())
}

fn check_new_password(password: &str) -> Result<(), LibraryError> {
    if !(1..=MAX_PASSWORD_CHARS).contains(&password.chars().count()) {
        return Err(LibraryError::Invalid(format!(
            "the password must be 1 to {MAX_PASSWORD_CHARS} characters"
        )));
    }
    Ok(())
}

/// `len` random bytes from the OS, in base64url. Of a login token, only the hash is stored.
fn random_string(len: usize) -> anyhow::Result<String> {
    let mut bytes = vec![0u8; len];
    getrandom::fill(&mut bytes).map_err(|e| anyhow!("couldn't get random bytes: {e}"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
}

/// Middleware for every route that needs a session: a missing or ended one is a 401 before the
/// handler, or any body extractor, runs.
pub async fn authenticate(State(auth): State<Arc<Auth>>, mut req: Request, next: Next) -> Response {
    let Some(token) = bearer(req.headers()).map(str::to_owned) else {
        let e = LibraryError::Unauthorized("sign in first".to_owned());
        return ApiErrorResponse(e).into_response();
    };
    let user = match auth.authenticate(&token, Time::now()).await {
        Ok(user) => user,
        Err(e) => return ApiErrorResponse(e).into_response(),
    };
    let id = user.id.clone();
    req.extensions_mut().insert(user);
    let mut response = next.run(req).await;
    response.extensions_mut().insert(LoggedUser(id));
    response
}

/// Middleware after `authenticate`: a user below `min` gets a 403.
pub async fn require(min: Role, req: Request, next: Next) -> Response {
    match req.extensions().get::<CurrentUser>().map(|user| user.role) {
        Some(role) if role >= min => next.run(req).await,
        Some(_) => {
            let message = format!("this needs the {} role or higher", min.as_str_name());
            ApiErrorResponse(LibraryError::Forbidden(message)).into_response()
        }
        None => {
            let e = LibraryError::Unauthorized("sign in first".to_owned());
            ApiErrorResponse(e).into_response()
        }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for CurrentUser {
    type Rejection = ApiErrorResponse;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<CurrentUser>().cloned().ok_or_else(|| {
            ApiErrorResponse(LibraryError::Unauthorized("sign in first".to_owned()))
        })
    }
}

pub async fn login(
    State(auth): State<Arc<Auth>>,
    Json(req): Json<LoginRequest>,
) -> Result<Response, ApiErrorResponse> {
    let res = auth.login(&req.username, &req.password, Time::now()).await?;
    let mut response = Json(res).into_response();
    response.extensions_mut().insert(LoggedUser(req.username));
    Ok(response)
}

pub async fn logout(
    State(auth): State<Arc<Auth>>,
    user: CurrentUser,
) -> Result<StatusCode, ApiErrorResponse> {
    auth.logout(&user, Time::now()).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn me(user: CurrentUser) -> Json<User> {
    Json(User {
        username: user.id,
        role: user.role.into(),
    })
}

pub async fn change_password(
    State(auth): State<Arc<Auth>>,
    user: CurrentUser,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<StatusCode, ApiErrorResponse> {
    auth.change_password(&user, &req, Time::now()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    pub(super) fn now() -> Time {
        "2026-11-13T10:00:00Z".parse().unwrap()
    }

    pub(super) async fn auth(dir: &Path) -> Auth {
        let json = r#"{ "users": {
            "parth": { "password": "hunter2", "role": "OWNER" },
            "kid": { "password": "pancakes", "role": "READER" },
            "banned": { "password": "waffles" },
            "nopass": { "role": "READER" }
        } }"#;
        std::fs::write(dir.join("data.json"), json).unwrap();
        Auth::new(DataFile::load(dir, now()).await.unwrap())
    }

    pub(super) fn stored(dir: &Path) -> String {
        std::fs::read_to_string(dir.join("data.json")).unwrap()
    }

    /// With a later mtime, so the next call reloads it.
    pub(super) fn edit_by_hand(dir: &Path, from: &str, to: &str) {
        let file = dir.join("data.json");
        let before = std::fs::metadata(&file).unwrap().modified().unwrap();
        std::fs::write(&file, stored(dir).replace(from, to)).unwrap();
        let handle = std::fs::File::options().write(true).open(&file).unwrap();
        handle.set_modified(before + Duration::from_secs(1)).unwrap();
    }

    pub(super) fn is_status(result: Result<impl Sized, LibraryError>, want: StatusCode) -> bool {
        let Err(e) = result else { return false };
        ApiErrorResponse(e).into_response().status() == want
    }

    #[tokio::test]
    async fn login_gives_a_token_that_authenticates_and_only_its_hash_is_stored() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;

        let res = auth.login("parth", "hunter2", now()).await.unwrap();
        let user = auth.authenticate(&res.token, now()).await.unwrap();

        assert_eq!(user.id, "parth");
        assert_eq!(user.role, Role::Owner);
        assert_eq!(res.user.unwrap().role(), Role::Owner);
        let file = stored(dir.path());
        assert!(!file.contains(&res.token));
        assert!(file.contains(&hash(&res.token)));
    }

    #[tokio::test]
    async fn wrong_password_unknown_user_and_no_password_are_401() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;

        for (user, password) in [("parth", "nope"), ("nobody", "hunter2"), ("nopass", "")] {
            let result = auth.login(user, password, now()).await;
            assert!(is_status(result, StatusCode::UNAUTHORIZED), "{user}");
        }
        assert!(is_status(
            auth.authenticate("made-up", now()).await,
            StatusCode::UNAUTHORIZED
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn five_wrong_passwords_lock_the_user_for_a_minute() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        for _ in 0..MAX_FAILURES {
            let _ = auth.login("parth", "nope", now()).await;
        }

        let locked = auth.login("parth", "hunter2", now()).await;
        assert!(is_status(locked, StatusCode::TOO_MANY_REQUESTS));
        assert!(auth.login("kid", "pancakes", now()).await.is_ok());

        tokio::time::advance(LOCK_TIME).await;
        assert!(auth.login("parth", "hunter2", now()).await.is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn a_successful_login_clears_the_failures() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        for _ in 0..MAX_FAILURES - 1 {
            let _ = auth.login("parth", "nope", now()).await;
        }
        auth.login("parth", "hunter2", now()).await.unwrap();

        let _ = auth.login("parth", "nope", now()).await;
        assert!(auth.login("parth", "hunter2", now()).await.is_ok());
    }

    #[tokio::test]
    async fn a_session_expires_unless_used_and_extends_at_most_once_a_day() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        let token = auth.login("kid", "pancakes", now()).await.unwrap().token;
        let first = stored(dir.path());

        auth.authenticate(&token, now() + Duration::from_secs(60 * 60)).await.unwrap();
        assert_eq!(stored(dir.path()), first, "used again within a day: no write");

        let later = now() + DAY + Duration::from_secs(1);
        auth.authenticate(&token, later).await.unwrap();
        assert!(stored(dir.path()).contains(&(later + SESSION_LIFETIME).to_string()));

        let ended = later + SESSION_LIFETIME;
        assert!(is_status(
            auth.authenticate(&token, ended).await,
            StatusCode::UNAUTHORIZED
        ));
    }

    #[tokio::test]
    async fn logout_ends_only_that_session() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        let phone = auth.login("kid", "pancakes", now()).await.unwrap().token;
        let laptop = auth.login("kid", "pancakes", now()).await.unwrap().token;

        let user = auth.authenticate(&phone, now()).await.unwrap();
        auth.logout(&user, now()).await.unwrap();

        assert!(auth.authenticate(&phone, now()).await.is_err());
        assert!(auth.authenticate(&laptop, now()).await.is_ok());
    }

    #[tokio::test]
    async fn changing_the_password_keeps_this_session_and_ends_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        let phone = auth.login("kid", "pancakes", now()).await.unwrap().token;
        let laptop = auth.login("kid", "pancakes", now()).await.unwrap().token;
        let user = auth.authenticate(&phone, now()).await.unwrap();

        let wrong = ChangePasswordRequest {
            current_password: "waffles".into(),
            new_password: "syrup".into(),
        };
        assert!(is_status(
            auth.change_password(&user, &wrong, now()).await,
            StatusCode::BAD_REQUEST
        ));
        let empty = ChangePasswordRequest {
            current_password: "pancakes".into(),
            new_password: String::new(),
        };
        assert!(auth.change_password(&user, &empty, now()).await.is_err());

        let req = ChangePasswordRequest {
            current_password: "pancakes".into(),
            new_password: "syrup".into(),
        };
        auth.change_password(&user, &req, now()).await.unwrap();

        assert!(auth.authenticate(&phone, now()).await.is_ok());
        assert!(auth.authenticate(&laptop, now()).await.is_err());
        assert!(auth.login("kid", "pancakes", now()).await.is_err());
        assert!(auth.login("kid", "syrup", now()).await.is_ok());
    }

    #[tokio::test]
    async fn a_role_edited_by_hand_applies_to_the_next_request() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        let token = auth.login("parth", "hunter2", now()).await.unwrap().token;

        edit_by_hand(dir.path(), "OWNER", "LIBRARIAN");

        let user = auth.authenticate(&token, now()).await.unwrap();
        assert_eq!(user.role, Role::Librarian);
    }

    #[tokio::test]
    async fn a_banned_user_cant_sign_in() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;

        let wrong = auth.login("banned", "nope", now()).await;
        assert!(is_status(wrong, StatusCode::UNAUTHORIZED));
        let right = auth.login("banned", "waffles", now()).await;
        assert!(is_status(right, StatusCode::FORBIDDEN));
        assert!(!stored(dir.path()).contains("sessions"));
    }

    #[tokio::test]
    async fn banning_by_hand_ends_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path()).await;
        let token = auth.login("kid", "pancakes", now()).await.unwrap().token;

        edit_by_hand(dir.path(), r#""role": "READER""#, r#""role": "BANNED""#);

        assert!(is_status(
            auth.authenticate(&token, now()).await,
            StatusCode::UNAUTHORIZED
        ));
    }

    #[test]
    fn bearer_reads_the_token() {
        let mut headers = HeaderMap::new();
        assert_eq!(bearer(&headers), None);
        headers.insert(header::AUTHORIZATION, "Bearer abc".parse().unwrap());
        assert_eq!(bearer(&headers), Some("abc"));
        headers.insert(header::AUTHORIZATION, "Basic abc".parse().unwrap());
        assert_eq!(bearer(&headers), None);
    }
}
