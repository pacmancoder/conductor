use crate::{
    encoding::Base64EncodedData,
    env::{locate_path, PathAsset},
    Error, Result,
};
use rsa;
use serde::{Deserialize, Serialize};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
};

#[derive(Serialize, Deserialize)]
struct KeyStore {
    public: Base64EncodedData,
    private: Base64EncodedData,
}

impl KeyStore {
    pub async fn load() -> Result<Self> {
        let keystore_path = locate_path(PathAsset::KeyStore);

        if !keystore_path.exists() {
            return Err(Error::KeyStoreIsMissing);
        }

        let mut keystore_file = File::open(keystore_path)
            .await
            .map_err(Error::from_io_error)?;

        let mut keystore_data = Vec::new();
        keystore_file
            .read_to_end(&mut keystore_data)
            .await
            .map_err(Error::from_io_error)?;

        let keystore = toml::from_slice(&keystore_data).map_err(|_| Error::KeyStoreIsCorrupted)?;

        Ok(keystore)
    }

    pub fn generate() -> Result<Self> {
        use rsa::{PrivateKeyEncoding, PublicKeyEncoding};

        let mut rng = rand::thread_rng();
        let private_key = rsa::RSAPrivateKey::new(&mut rng, 2048)
            .map_err(|_| Error::KeyStoreGenerationFailure)?;
        let public_key = rsa::RSAPublicKey::from(&private_key);

        let private_key_data = private_key
            .to_pkcs8()
            .map_err(|_| Error::KeyStoreGenerationFailure)?;
        let public_key_data = public_key
            .to_pkcs8()
            .map_err(|_| Error::KeyStoreGenerationFailure)?;

        let keystore = KeyStore {
            private: private_key_data.into(),
            public: public_key_data.into(),
        };

        Ok(keystore)
    }

    pub async fn save(&self) -> Result<()> {
        let keystore_path = locate_path(PathAsset::KeyStore);

        let mut keystore_file = File::create(keystore_path)
            .await
            .map_err(Error::from_io_error)?;

        let keystore_data = toml::to_vec(self)
            .map_err(|_| Error::from_application_error("Keysore serialization failed"))?;

        keystore_file
            .write_all(&keystore_data)
            .await
            .map_err(Error::from_io_error)?;

        Ok(())
    }
}
