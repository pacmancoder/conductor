use thiserror::Error;

pub type StringCow = std::borrow::Cow<'static, str>;

#[derive(Debug, Error)]
pub enum ConductorError {
    #[error("Failed to parse \"{source_string}\": {cause}")]
    StringParsingError {
        source_string: String,
        cause: StringCow,
    },
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
}
