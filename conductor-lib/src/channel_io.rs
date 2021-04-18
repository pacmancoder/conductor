use crate::proto::{ConductorMessage, DataChannelId};
use bytes::{Bytes, BytesMut};
use futures::{channel::mpsc, ready, Stream};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

enum ChannelRxState {
    WaitingForMessage,
    ReadingMessage(ConductorMessage),
}

enum ChannelTxState {
    WaitingForMessage,
    GrowingMessage(BytesMut),
    WritingMessage(ConductorMessage),
}

pub struct ChannelIO {
    channel_num: DataChannelId,

    tx: mpsc::Sender<ConductorMessage>,
    rx: mpsc::Receiver<ConductorMessage>,

    tx_state: Option<ChannelTxState>,
    rx_state: ChannelRxState,
}

impl ChannelIO {
    pub fn new(
        channel_num: DataChannelId,
        tx: mpsc::Sender<ConductorMessage>,
        rx: mpsc::Receiver<ConductorMessage>,
    ) -> Self {
        Self {
            channel_num,
            tx,
            rx,
            tx_state: Some(ChannelTxState::WaitingForMessage),
            rx_state: ChannelRxState::WaitingForMessage,
        }
    }
}

impl AsyncRead for ChannelIO {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        loop {
            match &mut self.rx_state {
                ChannelRxState::WaitingForMessage => {
                    let message = ready!(Pin::new(&mut self.rx).poll_next(cx));
                    if message.is_none() {
                        return Poll::Ready(Ok(()));
                    }

                    self.rx_state = ChannelRxState::ReadingMessage(message.unwrap());
                }
                ChannelRxState::ReadingMessage(message) => {
                    let bytes_to_read = message.data.len().min(buf.remaining());

                    let slice_to_read = message.data.split_to(bytes_to_read);
                    buf.put_slice(&slice_to_read);

                    if message.data.is_empty() {
                        self.rx_state = ChannelRxState::WaitingForMessage;
                    }

                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

const MAX_MESSAGE_SIZE: usize = 1024;

impl AsyncWrite for ChannelIO {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let channel = self.channel_num;
        let result = self.tx.try_send(ConductorMessage {
            channel,
            data: Bytes::copy_from_slice(buf),
        });

        if let Err(_) = result {
            println!("fail!");
        }

        Poll::Ready(Ok(buf.len()))
        /*
        loop {
            match self.tx_state.take().unwrap() {
                ChannelTxState::WaitingForMessage => {
                    if buf.len() >= MAX_MESSAGE_SIZE {
                        let channel = self.channel_num;

                        self.tx_state
                            .replace(ChannelTxState::WritingMessage(ConductorMessage {
                                channel,
                                data: Bytes::copy_from_slice(&buf[..MAX_MESSAGE_SIZE]),
                            }));
                        return Poll::Ready(Ok(MAX_MESSAGE_SIZE));
                    }

                    let mut bytes = BytesMut::with_capacity(buf.len());
                    bytes.extend_from_slice(buf);
                    self.tx_state.replace(ChannelTxState::GrowingMessage(bytes));
                    return Poll::Ready(Ok(buf.len()));
                }
                ChannelTxState::GrowingMessage(mut message) => {
                    if message.len() + buf.len() >= MAX_MESSAGE_SIZE {
                        let channel = self.channel_num;
                        let bytes_to_write = MAX_MESSAGE_SIZE - message.len();

                        message.extend_from_slice(&buf[..bytes_to_write]);
                        let data = message.freeze();

                        self.tx_state
                            .replace(ChannelTxState::WritingMessage(ConductorMessage {
                                channel,
                                data,
                            }));
                        return Poll::Ready(Ok(bytes_to_write));
                    }

                    message.extend_from_slice(buf);
                    self.tx_state
                        .replace(ChannelTxState::GrowingMessage(message));
                    return Poll::Ready(Ok(buf.len()));
                }
                ChannelTxState::WritingMessage(message) => {
                    self.tx_state
                        .replace(ChannelTxState::WritingMessage(message));
                    // flush will adjust state accordingly itself.
                    // On Poll::Ready(Ok(_)) loop will continue
                    ready!(self.as_mut().poll_flush(cx))?;
                }
            }
        }
         */
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        loop {
            match self.tx_state.take().unwrap() {
                ChannelTxState::WaitingForMessage => {
                    // internal buffer is empty
                    self.tx_state.replace(ChannelTxState::WaitingForMessage);
                    return Poll::Ready(Ok(()));
                }
                ChannelTxState::GrowingMessage(data) => {
                    let channel = self.channel_num;
                    // goto next state (send)
                    self.tx_state
                        .replace(ChannelTxState::WritingMessage(ConductorMessage {
                            channel,
                            data: data.freeze(),
                        }));
                }
                ChannelTxState::WritingMessage(message) => {
                    let result = self.tx.poll_ready(cx).map_err(|_| {
                        std::io::Error::new(
                            std::io::ErrorKind::ConnectionAborted,
                            "Stream was closed".to_string(),
                        )
                    });

                    return match result {
                        Poll::Ready(Ok(_)) => {
                            // actually send message over channel in non-blocking fashion

                            self.tx.start_send(message).map_err(|_| {
                                std::io::Error::new(
                                    std::io::ErrorKind::ConnectionAborted,
                                    "Stream was closed".to_string(),
                                )
                            })?;

                            self.tx_state.replace(ChannelTxState::WaitingForMessage);

                            Poll::Ready(Ok(()))
                        }
                        Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::ConnectionAborted,
                            format!("Failed to flush data: {}", e),
                        ))),
                        Poll::Pending => {
                            self.tx_state
                                .replace(ChannelTxState::WritingMessage(message));
                            Poll::Pending
                        }
                    };
                }
            };
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        // Flush message before closing channel
        self.poll_flush(cx)
    }
}
