// TODO: stream timeouts (via tokio_io_timeout crate?)
use crate::{Error, Result};
use picky::jose::{
    jwe::JweAlg,
    jwt::{JwtEnc, JwtValidator},
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_MESSAGE_SIZE: usize = 1024 * 32; // 16K

async fn send_message<S>(stream: &mut S, data: String) -> Result<()>
where
    S: AsyncWrite + Unpin,
{
    if data.len() > MAX_MESSAGE_SIZE {
        return Err(Error::TooBigMessage);
    }

    let bytes = data.as_bytes();

    stream
        .write_u16(bytes.len() as u16)
        .await
        .map_err(Error::from_io_error)?;
    stream
        .write_all(bytes)
        .await
        .map_err(Error::from_io_error)?;

    Ok(())
}

async fn receive_message<S>(stream: &mut S) -> Result<String>
where
    S: AsyncRead + Unpin,
{
    let len = stream.read_u16().await.map_err(Error::from_io_error)?;

    if len as usize > MAX_MESSAGE_SIZE {
        return Err(Error::TooBigMessage);
    }

    let mut data = Vec::new();
    stream
        .read_exact(&mut data)
        .await
        .map_err(Error::from_io_error)?;

    Ok(String::from_utf8(data).map_err(|_| Error::MessageIsCorrupted)?)
}

pub async fn send_serialized<S>(stream: &mut S, message: impl serde::Serialize) -> Result<()>
where
    S: AsyncWrite + Unpin,
{
    let message = serde_json::to_string(&message)
        .map_err(|_| Error::from_application_error("Failed to serialize message"))?;

    send_message(stream, message).await
}

pub async fn receive_serialized<T, S>(stream: &mut S) -> Result<T>
where
    S: AsyncRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    let serialized = receive_message(stream).await?;

    let value: T = serde_json::from_str(&serialized)
        .map_err(|_| Error::from_application_error("Failed to deserialize message"))?;

    Ok(value)
}

pub async fn send_encrypted_jwt<S>(
    stream: &mut S,
    claims: impl serde::Serialize,
    public_key: &picky::key::PublicKey,
) -> Result<()>
where
    S: AsyncWrite + Unpin,
{
    let client_message = JwtEnc::new(
        JweAlg::RsaPkcs1v15,
        picky::jose::jwe::JweEnc::Aes128Gcm,
        claims,
    )
    .encode(public_key)
    .map_err(|_| Error::from_application_error("Failed to encrypt jwt"))?;

    send_message(stream, client_message).await
}

pub async fn receive_encrypted_jwt<T, S>(
    stream: &mut S,
    private_key: &picky::key::PrivateKey,
) -> Result<T>
where
    S: AsyncRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    let jwt_string = receive_message(stream).await?;

    // TODO: do actual jwt validation
    let jwt: JwtEnc<T> = JwtEnc::decode(&jwt_string, private_key, &JwtValidator::no_check())
        .map_err(|_| Error::from_application_error("Failed to decrypt jwt"))?;

    Ok(jwt.claims)
}
