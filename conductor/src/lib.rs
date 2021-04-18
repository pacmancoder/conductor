pub mod command;
pub mod encoding;
pub mod net;

mod connection;
mod proto;
mod worker;

mod error;

pub use error::ConductorError as Error;
pub type Result<T> = std::result::Result<T, Error>;