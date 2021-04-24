use thiserror::Error;

pub type StringCow = std::borrow::Cow<'static, str>;

#[derive(Debug, Error)]
pub enum ConductorError {
    #[error("Failed to parse \"{source_string}\": {cause}")]
    StringParsingError {
        source_string: String,
        cause: StringCow,
    },
    #[error("Failed to perform IO operation: {source}")]
    IoError { source: std::io::Error },
    #[error("Personal key store was not found")]
    KeyStoreIsMissing,
    #[error("Personal key store is corrupted")]
    KeyStoreIsCorrupted,
    #[error("Crypto key is corrupted")]
    CorruptedCryptoKey,
    #[error("Failed to generate keystore")]
    KeyStoreGenerationFailure,
    #[error("Application error")]
    ApplicationError { cause: StringCow },
    #[error("Tunnel establish message is too big")]
    TooBigMessage,
    #[error("Tunnel establish message is corrupted")]
    MessageIsCorrupted,
    #[error("Failed to perform peer challenge")]
    PeerChallengeResolveFailed,
    #[error("Server returned invalid key")]
    InvalidServerKey,
    #[error("Invalid auth sequence")]
    AuthSequenceFailed,
    #[error("Tunnel alias does not exist")]
    TunnelAliasDoesNotExist,
    #[error("Tunnel does not exist")]
    TunnelDoesNotExist,
    #[error("Tunnel already exist")]
    TunnelAlreadyExist,
    #[error("Catalog db was corrupted")]
    CorruptedCatalog,
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

    pub fn from_application_error(cause: impl Into<StringCow>) -> Self {
        Self::ApplicationError {
            cause: cause.into(),
        }
    }
}
