use crate::{Error, Result};
use rsa::PublicKeyParts;
use sha2::{Digest, Sha256};

pub type Fingerprint = String;

pub struct PublicKey {
    key: rsa::RSAPublicKey,
    key_picky: picky::key::PublicKey,
    key_raw: Vec<u8>,
}

impl PublicKey {
    pub fn parse_pkcs8(data: &[u8]) -> Result<Self> {
        let key = rsa::RSAPublicKey::from_pkcs8(data).map_err(|_| Error::CorruptedCryptoKey)?;
        let key_picky = picky::key::PublicKey::from_pem(
            &picky::pem::parse_pem(data).map_err(|_| Error::CorruptedCryptoKey)?,
        )
        .map_err(|_| Error::CorruptedCryptoKey)?;
        let key_raw = data.into();

        Ok(Self {
            key,
            key_picky,
            key_raw,
        })
    }

    pub fn fingerprint(&self) -> Fingerprint {
        let modulus = self.key.n().to_bytes_be();
        let exponent = self.key.e().to_bytes_be();

        let mut hasher = Sha256::default();
        hasher.update(modulus);
        hasher.update(exponent);
        let hash = hasher.finalize();

        base64::encode(hash)
    }
}

impl AsRef<rsa::RSAPublicKey> for PublicKey {
    fn as_ref(&self) -> &rsa::RSAPublicKey {
        &self.key
    }
}

impl AsRef<picky::key::PublicKey> for PublicKey {
    fn as_ref(&self) -> &picky::key::PublicKey {
        &self.key_picky
    }
}

impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        &self.key_raw
    }
}

pub struct PrivateKey {
    key: rsa::RSAPrivateKey,
    key_picky: picky::key::PrivateKey,
    key_raw: Vec<u8>,
}

impl PrivateKey {
    pub fn parse_pkcs8(data: &[u8]) -> Result<Self> {
        let key = rsa::RSAPrivateKey::from_pkcs8(data).map_err(|_| Error::CorruptedCryptoKey)?;
        let key_picky = picky::key::PrivateKey::from_pem(
            &picky::pem::parse_pem(data).map_err(|_| Error::CorruptedCryptoKey)?,
        )
        .map_err(|_| Error::CorruptedCryptoKey)?;
        let key_raw = data.into();

        Ok(Self {
            key,
            key_picky,
            key_raw,
        })
    }
}

impl AsRef<rsa::RSAPrivateKey> for PrivateKey {
    fn as_ref(&self) -> &rsa::RSAPrivateKey {
        &self.key
    }
}

impl AsRef<picky::key::PrivateKey> for PrivateKey {
    fn as_ref(&self) -> &picky::key::PrivateKey {
        &self.key_picky
    }
}

impl AsRef<[u8]> for PrivateKey {
    fn as_ref(&self) -> &[u8] {
        &self.key_raw
    }
}

pub struct KeyPair {
    pub public: PublicKey,
    pub private: PrivateKey,
}

impl KeyPair {
    pub fn from_pkcs8(public_data: &[u8], private_data: &[u8]) -> Result<Self> {
        let public = PublicKey::parse_pkcs8(public_data)?;
        let private = PrivateKey::parse_pkcs8(private_data)?;

        Ok(Self { public, private })
    }
}
