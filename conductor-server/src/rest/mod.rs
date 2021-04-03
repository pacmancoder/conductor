mod auth;
mod health;
mod tunnel;

pub mod server;

use crate::db::Database;
use serde::Serialize;
use std::borrow::Cow;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

use rocket::http::ContentType;
use rocket::{http::Status, response::Responder, Request, Response};
use std::io::Cursor;

pub type SyncDatabase = Arc<Mutex<Database>>;

pub type ApiResult = Result<ApiResponse, ApiError>;

#[derive(Debug)]
pub enum ApiError {
    UserIsAlreadyRegistered,
    UserDoesNotExist,
    ApiKeyIsMissing,
    InvalidApiKey,
    InternalError(String),
}

impl ApiError {
    pub fn description(&self) -> String {
        match &self {
            Self::UserIsAlreadyRegistered => "User is already registered".into(),
            Self::UserDoesNotExist => "User does not exist".into(),
            Self::ApiKeyIsMissing => "API key is missing from headers".into(),
            Self::InvalidApiKey => "Invalid API key".into(),
            Self::InternalError(e) => format!("Internal server error: {}", e),
        }
    }

    pub fn status(&self) -> Status {
        match &self {
            Self::UserIsAlreadyRegistered => Status::Conflict,
            Self::UserDoesNotExist => Status::NotFound,
            Self::ApiKeyIsMissing | Self::InvalidApiKey => Status::Unauthorized,
            Self::InternalError(_) => Status::InternalServerError,
        }
    }
}

impl<T> From<T> for ApiError
where
    T: std::error::Error,
{
    fn from(err: T) -> Self {
        Self::InternalError(err.to_string())
    }
}

impl<'r> Responder<'r, 'static> for ApiError {
    fn respond_to(self, _: &'r Request<'_>) -> Result<Response<'static>, Status> {
        Response::build()
            .status(self.status())
            .header(ContentType::Plain)
            .sized_body(None, Cursor::new(self.description()))
            .ok()
    }
}

pub enum ApiResponse {
    Json(String),
}

impl ApiResponse {
    pub fn json(obj: impl Serialize) -> Result<Self, ApiError> {
        Ok(Self::Json(serde_json::to_string(&obj)?))
    }
}

impl<'r> Responder<'r, 'static> for ApiResponse {
    fn respond_to(self, _: &'r Request<'_>) -> Result<Response<'static>, Status> {
        match self {
            ApiResponse::Json(json) => Response::build()
                .status(Status::Ok)
                .header(ContentType::JSON)
                .sized_body(None, Cursor::new(json))
                .ok(),
        }
    }
}
