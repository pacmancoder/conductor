#![allow(dead_code)]

pub mod command;
pub mod db;
pub mod encoding;
pub mod env;
pub mod net;

mod auth;
mod connection;
mod crypto;
mod proto;
mod worker;

mod error;

pub use error::ConductorError as Error;
pub type Result<T> = std::result::Result<T, Error>;
