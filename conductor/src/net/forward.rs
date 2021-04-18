use crate::{Error, Result};
use futures::FutureExt;
use std::num::NonZeroUsize;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};

async fn forward_reader_to_writer<R, W>(mut reader: R, mut writer: W) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = vec![0u8; 4096];

    let bytes_read = reader.read(&mut buf).await.map_err(Error::from_io_error)?;

    while let Some(count) = NonZeroUsize::new(bytes_read) {
        writer
            .write_all(&buf[0..count.get()])
            .await
            .map_err(Error::from_io_error)?;
    }

    Ok(())
}

pub async fn forward_socket(client: TcpStream, server: TcpStream) -> Result<()> {
    let (client_reader, client_writer) = tokio::io::split(client);
    let (server_reader, server_writer) = tokio::io::split(server);

    let client_to_server = forward_reader_to_writer(client_reader, server_writer).fuse();
    let server_to_client = forward_reader_to_writer(server_reader, client_writer).fuse();

    log::info!("Forwarding started...");
    tokio::select! {
        _ = client_to_server => {},
        _ = server_to_client => {},
    }
    Ok(())
}
