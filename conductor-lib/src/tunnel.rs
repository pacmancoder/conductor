use crate::proto::tunnel::ResultCode;
use futures::FutureExt;
use num_traits::FromPrimitive;
use std::num::NonZeroUsize;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    pin, select,
};

async fn forward_reader_to_writer<R, W>(mut reader: R, mut writer: W) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = vec![0; 4096];

    while let Some(count) = NonZeroUsize::new(reader.read(&mut buf).await?) {
        writer.write_all(&buf[0..count.get()]).await?;
    }

    Ok(())
}

/// Called by peer instance when tunnel is required due to incoming connect to local
/// listening port or when client peer requests connection to host's local port
async fn peer_open_tunnel(
    local: TcpStream,
    remote: TcpStream,
    header: String,
) -> std::io::Result<()> {
    let (mut remote_rx, mut remote_tx) = remote.into_split();
    let (local_rx, local_tx) = local.into_split();

    assert!(
        header.len() <= u16::max_value() as usize,
        "Peer connect header is too long"
    );
    remote_tx.write_u16(header.len() as u16).await?;
    remote_tx.write_all(header.as_bytes()).await?;

    let result_code = ResultCode::from_u16(remote_rx.read_u16().await?);

    if result_code != Some(ResultCode::Ok) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Server returned error",
        ));
    }

    let local_to_remote = forward_reader_to_writer(local_rx, remote_tx).fuse();
    let remote_to_local = forward_reader_to_writer(remote_rx, local_tx).fuse();
    pin!(local_to_remote, remote_to_local);

    select! {
        result = local_to_remote => {
            match result {
                Err(_) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "local to remote connection was closed unexpectedly",
                    ));
                },
                _ => {},
            };
        },
        result = remote_to_local => {
            match result {
                Err(_) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "remote to local connection was closed unexpectedly",
                    ));
                },
                _ => {},
            };
        },
    };

    Ok(())
}
