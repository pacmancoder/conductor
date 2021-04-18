use thiserror::Error;
use futures::future::err;

pub type StringCow = std::borrow::Cow<'static, str>;

#[derive(Debug, Error)]
pub enum ConductorError {
    #[error("Failed to parse \"{source_string}\": {cause}")]
    StringParsingError {
        source_string: String,
        cause: StringCow,
    },
    #[error("Failed to perform IO operation: {source}")]
    IoError {
        source: std::io::Error,
    }
}

impl ConductorError {
    pub fn from_string_parsing_error(
        source_string: impl Into<String>,
        cause: impl Into<StringCow>,
    ) -> Self {
        Self::StringParsingError {
            source_string: source_string.into(),
            cause: cause.into(),
        }
    }

    pub fn from_io_error(source: std::io::Error) -> Self {
        Self::IoError { source }
    }
}
