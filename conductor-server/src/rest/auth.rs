use crate::{
    db,
    rest::{ApiError, ApiResponse, ApiResult, SyncDatabase},
};
use rocket::futures::future::err;
use rocket::{
    request::{FromRequest, Outcome, Request},
    Route, State,
};
use rocket_contrib::json::Json as JsonBody;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct RawCredentials {
    password: String,
    username: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthTokenClaims {
    // Expiration timestamp
    exp: i64,
    // Issued at timestamp
    iat: i64,
    // Not before timestamp
    nbf: i64,
    // User ID
    uid: Uuid,
    // Auth session ID
    sid: Uuid,
}

#[derive(Serialize)]
struct AuthResponse {
    token: String,
}

// TODO: just proof-of-concept, return JWT in future
fn create_auth_token(user_id: &Uuid) -> Result<String, ApiError> {
    let claims = AuthTokenClaims {
        exp: 0,
        iat: 0,
        nbf: 0,
        uid: user_id.clone(),
        sid: uuid::Uuid::new_v4(),
    };

    let serialized = serde_json::to_string(&claims)?;
    return Ok(base64::encode(serialized));
}

pub fn validate_token(token: &str) -> Result<AuthTokenClaims, ApiError> {
    if token == "invalid" {
        return Err(ApiError::InvalidApiKey);
    }
    // TODO: do token validation
    Ok(AuthTokenClaims {
        iat: 0,
        exp: 0,
        nbf: 0,
        uid: Uuid::nil(),
        sid: Uuid::nil(),
    })
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AuthTokenClaims {
    type Error = ApiError;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match req.headers().get_one("x-api-key") {
            None => {
                let error = ApiError::ApiKeyIsMissing;
                Outcome::Failure((error.status(), error))
            }
            Some(key_str) => match validate_token(key_str) {
                Ok(key) => Outcome::Success(key),
                Err(error) => Outcome::Failure((error.status(), error)),
            },
        }
    }
}

#[derive(Serialize)]
struct AuthTokenResponse {
    token: String,
}

#[rocket::post("/register", data = "<credentials>")]
async fn register(
    credentials: JsonBody<RawCredentials>,
    database: State<'_, SyncDatabase>,
) -> ApiResult {
    let mut db = database.lock().await;

    if let Ok(_) = db.find_user_id_by_username(&credentials.username).await {
        log::debug!(
            "Api user tried to register already existing user {}",
            &credentials.username
        );
        return Err(ApiError::UserIsAlreadyRegistered);
    }

    let user = db::User::new_for_credentials(&credentials.username, &credentials.password);
    db.create_user(&user).await?;

    let token = create_auth_token(&user.id)?;
    ApiResponse::json(AuthTokenResponse { token })
}

#[rocket::post("/login", data = "<credentials>")]
async fn login(
    credentials: JsonBody<RawCredentials>,
    database: State<'_, SyncDatabase>,
) -> ApiResult {
    let mut db = database.lock().await;

    let id = db
        .find_user_id_by_username(&credentials.username)
        .await
        .map_err(|e| {
            log::debug!(
                "Request tried to query non-existing user {}: ({})",
                &credentials.username,
                e
            );
            ApiError::UserDoesNotExist
        })?;

    let user = db.fetch_user(&id).await?;

    let token = create_auth_token(&user.id)?;
    ApiResponse::json(AuthTokenResponse { token })
}

pub fn routes() -> Vec<Route> {
    rocket::routes![register, login]
}
