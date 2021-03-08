use crate::{
    channel_io::ChannelIO,
    proto::{ConductorCodec, ConductorMessage, ConductorProtoError, DataChannelId},
};

use futures::{channel::mpsc, ready, FutureExt, Sink, SinkExt, Stream, StreamExt};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    u16,
};
use thiserror::Error;
use tokio::net::TcpSocket;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    select,
    sync::{mpsc::error::TrySendError, Mutex},
};
use tokio_util::codec::{FramedRead, FramedWrite};

#[derive(Debug, Error)]
enum SwitchError {
    #[error("Channel already exist")]
    ChannelAlreadyExist,
    #[error("Channel is unregistered")]
    ChannelIsUnregistered,
    #[error("Channel network side already acquired")]
    RxChannelAlreadyAcquired,
    #[error("Channel was already closed")]
    ChannelAlreadyClosed,
    #[error("Failed to lookup host")]
    HostLookupFailure,
    #[error("Failed to bind socket")]
    BindFailed,
    #[error("Failed to connect to the socket")]
    ConnectFailed,
    #[error("Failed to accept socket")]
    AcceptFailed,
    #[error(transparent)]
    ProtoError(#[from] ConductorProtoError),
}

type SwitchResult<T> = Result<T, SwitchError>;
type SwitchSender = mpsc::Sender<ConductorMessage>;
type SwitchReceiver = mpsc::Receiver<ConductorMessage>;

// TODO: change Channel<Bytes> for ring buffer ?

struct ChannelInfo {
    /// network -> this machine
    rx_channel: (SwitchSender, Option<SwitchReceiver>),
}

type ChannelInfoEntry = Option<Box<ChannelInfo>>;

fn make_io_channel() -> (SwitchSender, SwitchReceiver) {
    mpsc::channel(256)
}

impl Default for ChannelInfo {
    fn default() -> Self {
        let (rx_sender, rx_receiver) = make_io_channel();
        Self {
            rx_channel: (rx_sender, Some(rx_receiver)),
        }
    }
}

impl Clone for ChannelInfo {
    fn clone(&self) -> Self {
        Self::default()
    }
}

enum ChannelRegistry {
    Fast(Vec<ChannelInfoEntry>),
}

enum ChannelRegistryMode {
    Fast,
}

impl ChannelRegistry {
    pub fn new(mode: ChannelRegistryMode) -> Self {
        match mode {
            ChannelRegistryMode::Fast => {
                Self::Fast(vec![ChannelInfoEntry::None; u16::MAX as usize])
            }
        }
    }

    pub fn create_channel(&mut self, num: DataChannelId) -> SwitchResult<()> {
        match self {
            ChannelRegistry::Fast(channels) => {
                // num is guaranteed to be in bounds
                let channel = unsafe { channels.get_unchecked_mut(num as usize) };
                if channel.is_some() {
                    return Err(SwitchError::ChannelAlreadyExist);
                }

                channel.replace(Box::new(ChannelInfo::default()));
            }
        }

        Ok(())
    }

    fn get_channel(&mut self, num: DataChannelId) -> &mut ChannelInfoEntry {
        match self {
            ChannelRegistry::Fast(channels) => unsafe { channels.get_unchecked_mut(num as usize) },
        }
    }

    pub fn close_channel(&mut self, num: DataChannelId) -> SwitchResult<()> {
        let channel = self.get_channel(num);
        if channel.is_none() {
            return Err(SwitchError::ChannelAlreadyClosed);
        }
        *channel = None;
        Ok(())
    }

    pub fn take_channel_rx_receiver(&mut self, num: DataChannelId) -> SwitchResult<SwitchReceiver> {
        match self.get_channel(num) {
            ChannelInfoEntry::None => Err(SwitchError::ChannelIsUnregistered),
            ChannelInfoEntry::Some(channel_info) => channel_info
                .rx_channel
                .1
                .take()
                .ok_or(SwitchError::RxChannelAlreadyAcquired),
        }
    }
}

struct ChannelSwitchContext {
    channels: ChannelRegistry,
    /// this machine -> network
    tx_channel: (SwitchSender, Option<SwitchReceiver>),
}

struct ChannelSwitch {
    ctx: Arc<Mutex<ChannelSwitchContext>>,
}

impl ChannelSwitch {
    async fn process_socket(ctx: Arc<Mutex<ChannelSwitchContext>>, stream: TcpStream) {
        let (rx_tcp, tx_tcp) = tokio::io::split(stream);

        let tx = FramedWrite::new(tx_tcp, ConductorCodec::encoder());
        let rx = FramedRead::new(rx_tcp, ConductorCodec::decoder());

        let receive_task = async {
            let mut rx = rx;

            loop {
                if let Some(message) = rx.next().await.transpose()? {
                    let channel = message.channel;
                    let mut rx_sender = {
                        // TODO: should not acquire mutex on each packet!
                        let mut locked = ctx.lock().await;
                        locked
                            .channels
                            .get_channel(channel)
                            .as_ref()
                            .expect("no channel created")
                            .rx_channel
                            .0
                            .clone()
                    };
                    rx_sender.send(message).await.expect("rx failed");
                } else {
                    break;
                }
            }

            SwitchResult::Ok(())
        };

        let send_task = async {
            let mut tx = tx;

            let mut tx_receiver = {
                let mut locked = ctx.lock().await;
                locked
                    .tx_channel
                    .1
                    .take()
                    .expect("tx_channel already captured")
            };

            loop {
                if let Some(message) = tx_receiver.next().await {
                    tx.send(message).await.expect("tx failed");
                } else {
                    break;
                }
            }

            SwitchResult::Ok(())
        };
        let result = select!(
            send_result = send_task => send_result,
            receive_result = receive_task => receive_result,
        );

        if let Err(e) = result {
            log::error!("Conection failed: {}", e);
        }
    }

    pub async fn connect(self, host: impl ToSocketAddrs) -> SwitchResult<()> {
        let socket = tokio::net::lookup_host(host)
            .await
            .map_err(|_| SwitchError::HostLookupFailure)?
            .nth(0)
            .ok_or(SwitchError::HostLookupFailure)?;

        let stream = TcpSocket::new_v4()
            .map_err(|_| SwitchError::BindFailed)?
            .connect(socket)
            .await
            .map_err(|_| SwitchError::ConnectFailed)?;

        Self::process_socket(self.ctx.clone(), stream);

        Ok(())
    }

    pub async fn listen(self, host: impl ToSocketAddrs) -> SwitchResult<()> {
        let socket = tokio::net::lookup_host(host)
            .await
            .map_err(|_| SwitchError::HostLookupFailure)?
            .nth(0)
            .ok_or(SwitchError::HostLookupFailure)?;

        let stream = TcpListener::bind(socket)
            .await
            .map_err(|_| SwitchError::BindFailed)?;

        loop {
            let (stream, _) = stream
                .accept()
                .await
                .map_err(|_| SwitchError::AcceptFailed)?;
            tokio::spawn(Self::process_socket(self.ctx.clone(), stream));
        }

        Ok(())
    }

    pub async fn create_channel(&mut self, num: DataChannelId) -> SwitchResult<ChannelIO> {
        let mut this = self.ctx.lock().await;

        this.channels.create_channel(num)?;
        let rx_receiver = this.channels.take_channel_rx_receiver(num)?;
        let tx_sender = this.tx_channel.0.clone();

        Ok(ChannelIO::new(num, tx_sender, rx_receiver))
    }
}

impl Default for ChannelSwitch {
    fn default() -> Self {
        let tx_channel = make_io_channel();
        let ctx = ChannelSwitchContext {
            channels: ChannelRegistry::new(ChannelRegistryMode::Fast),
            tx_channel: (tx_channel.0, Some(tx_channel.1)),
        };

        Self {
            ctx: Arc::new(Mutex::new(ctx)),
        }
    }
}
