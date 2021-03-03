//! Conductor protocol
//!
//! ### Flags (bits table)
//! |   **bit**   |  7  |  6  |  5  |  4  |  3  |  2  |  1  |  0  |
//! | ----------- | --- | --- | --- | --- | --- | --- | --- | --- |
//! | **purpose** | ccm |  0  |  0  |  0  |  0  |  0  |  0  |  0  |
//!
//! - **ccm** - CONTROL_CHANNEL_MESSAGE. Set to one when next message is CCM message
//!   - when ccm is set, channel_id should be 0x00000000
//!
//! ### Packet data layout (bytes table)
//!
//! | **position** | buffer[0]  | buffer[1]  | buffer[2]...buffer[5] | buffer[6]...buffer[7]  |
//! | ------------ | ---------- | ---------- | --------------------- | ---------------------- |
//! | **example**  | 0x56       | 0b10000000 |  0xDEADBEAF           | 0xCAFE                 |
//! | **purpose**  | marker     | flags      |  channel_id           | payload_size           |
//! | **type**     | u8         | u8         |  u32, LE              | u16, LE                |

mod control;

use bitflags::bitflags;
use byteorder::ReadBytesExt;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::codec::{Decoder, Encoder};
use uuid::Uuid;

const HEADER_SIZE: usize = 8;
const HEADER_MARKER_VALUE: u8 = 0x56;

bitflags! {
    pub struct ChannelPacketFlags: u8 {
        const CONTROL_CHANNEL_MESSAGE = 0b10000000;
    }
}

/// Unique channel id between peer and conductor-server instance
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct DataChannelId(u32);

/// Unique Client <-> Host tunnel identifier
pub type TunnelId = Uuid;

/// Auth JWT token
pub type AuthToken = String;

pub enum ConductorMessage {
    Control(Bytes),
    Data { channel: DataChannelId, data: Bytes },
}

#[derive(Error, Debug)]
pub enum ConductorProtoError {
    #[error("Payload is too big")]
    TooBigPayload,
    #[error("Invalid message header")]
    InvalidHeader,
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

// =================================================================================================
// SERVER CODEC
// =================================================================================================

pub struct ConductorCodec;

impl ConductorCodec {
    pub fn encoder() -> ConductorEncoder {
        ConductorEncoder::default()
    }

    pub fn decoder() -> ConductorDecoder {
        ConductorDecoder::default()
    }
}

#[derive(Default)]
pub struct ConductorEncoder;

impl Encoder<ConductorMessage> for ConductorEncoder {
    type Error = ConductorProtoError;

    fn encode(&mut self, item: ConductorMessage, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let (header, channel, payload) = match item {
            ConductorMessage::Control(data) => {
                let header = ChannelPacketFlags::CONTROL_CHANNEL_MESSAGE.bits;
                (header, 0x00000000u32, data)
            }
            ConductorMessage::Data { channel, data } => (0x00u8, channel.0, data),
        };

        let payload_size = payload.len();

        if payload_size > u16::max_value() as usize {
            log::error!("Control payload size can't be bigger than u16 can fit");
            return Err(ConductorProtoError::TooBigPayload);
        }

        dst.reserve(HEADER_SIZE + payload_size);
        dst.put_u8(HEADER_MARKER_VALUE);
        dst.put_u8(header);
        dst.put_u32_le(channel);
        dst.put_u16_le(payload_size as u16);
        dst.extend_from_slice(&payload);

        Ok(())
    }
}

#[derive(Default)]
pub struct ConductorDecoder;

impl Decoder for ConductorDecoder {
    type Item = ConductorMessage;
    type Error = ConductorProtoError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < HEADER_SIZE {
            return Ok(None);
        }

        let mut reader = src.as_ref();
        if reader.read_u8().unwrap() != HEADER_MARKER_VALUE {
            return Err(ConductorProtoError::InvalidHeader);
        }
        let flags = ChannelPacketFlags::from_bits(reader.read_u8().unwrap())
            .ok_or(ConductorProtoError::InvalidHeader)?;

        let channel = reader.read_u32::<byteorder::LittleEndian>().unwrap();

        if flags.contains(ChannelPacketFlags::CONTROL_CHANNEL_MESSAGE) {
            if channel != 0x00000000 {
                return Err(ConductorProtoError::InvalidHeader);
            }
        }

        let payload_size = reader.read_u16::<byteorder::LittleEndian>().unwrap() as usize;
        let current_frame_size = HEADER_SIZE + payload_size;

        if src.len() < current_frame_size {
            // reserve place for the current frame and the next header for better efficiency
            src.reserve(current_frame_size + HEADER_SIZE);
            return Ok(None);
        }

        src.advance(HEADER_SIZE);
        let payload = src.split_to(payload_size).freeze();

        if flags.contains(ChannelPacketFlags::CONTROL_CHANNEL_MESSAGE) {
            Ok(Some(ConductorMessage::Control(payload)))
        } else {
            Ok(Some(ConductorMessage::Data {
                channel: DataChannelId(channel),
                data: payload,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_message_encode() {
        const EXPECTED: &[u8] = &[
            0x56, 0b10000000, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x02, 0x03,
        ];

        let mut actual = BytesMut::with_capacity(16);
        ConductorCodec::encoder()
            .encode(
                ConductorMessage::Control(Bytes::from_static(&[0x01, 0x02, 0x03])),
                &mut actual,
            )
            .unwrap();
        assert_eq!(&actual[..], EXPECTED)
    }

    #[test]
    fn data_message_encode() {
        const EXPECTED: &[u8] = &[
            0x56, 0b00000000, 0x33, 0x22, 0x11, 0x99, 0x03, 0x00, 0x01, 0x02, 0x03,
        ];

        let mut actual = BytesMut::with_capacity(16);
        ConductorCodec::encoder()
            .encode(
                ConductorMessage::Data {
                    channel: DataChannelId(0x99112233),
                    data: Bytes::from_static(&[0x01, 0x02, 0x03]),
                },
                &mut actual,
            )
            .unwrap();
        assert_eq!(&actual[..], EXPECTED)
    }

    #[test]
    fn too_long_message_encode_fails() {
        let mut actual = BytesMut::new();
        let result = ConductorCodec::encoder().encode(
            ConductorMessage::Data {
                channel: DataChannelId(0x99112233),
                data: Bytes::from_static(&[0u8; 66554]),
            },
            &mut actual,
        );

        assert!(matches!(result, Err(_)));
    }

    #[test]
    fn decode_fails_when_marker_is_invalid() {
        const INPUT_DATA: &[u8] = &[
            0x42, 0b00000000, 0x33, 0x22, 0x11, 0x99, 0x03, 0x00, 0x01, 0x02, 0x03,
        ];

        let mut buffer = BytesMut::from(INPUT_DATA);
        let result = ConductorCodec::decoder().decode(&mut buffer);
        assert!(matches!(result, Err(_)));
        assert_eq!(buffer.len(), INPUT_DATA.len());
    }

    #[test]
    fn decode_fails_when_control_message_has_invalid_channel() {
        const INPUT_DATA: &[u8] = &[
            0x56, 0b10000000, 0x33, 0x22, 0x11, 0x99, 0x03, 0x00, 0x01, 0x02, 0x03,
        ];

        let mut buffer = BytesMut::from(INPUT_DATA);
        let result = ConductorCodec::decoder().decode(&mut buffer);
        assert!(matches!(result, Err(_)));
        assert_eq!(buffer.len(), INPUT_DATA.len());
    }

    #[test]
    fn decode_fails_when_control_message_has_redundant_flags() {
        const INPUT_DATA: &[u8] = &[
            0x56, 0b01111111, 0x33, 0x22, 0x11, 0x99, 0x03, 0x00, 0x01, 0x02, 0x03,
        ];

        let mut buffer = BytesMut::from(INPUT_DATA);
        let result = ConductorCodec::decoder().decode(&mut buffer);
        assert!(matches!(result, Err(_)));
        assert_eq!(buffer.len(), INPUT_DATA.len());
    }

    #[test]
    fn decoder_requests_more_data_to_read_header() {
        const INPUT_DATA: &[u8] = &[0x56, 0b10000000, 0x00];

        let mut buffer = BytesMut::from(INPUT_DATA);
        let result = ConductorCodec::decoder().decode(&mut buffer);
        assert!(matches!(result, Ok(None)));
        assert_eq!(buffer.len(), INPUT_DATA.len());
    }

    #[test]
    fn decoder_requests_more_data_to_read_payload() {
        const INPUT_DATA: &[u8] = &[0x56, 0b00000000, 0x33, 0x22, 0x11, 0x99, 0x03, 0x00];

        let mut buffer = BytesMut::from(INPUT_DATA);
        let result = ConductorCodec::decoder().decode(&mut buffer);
        assert!(matches!(result, Ok(None)));
        assert_eq!(buffer.len(), INPUT_DATA.len());

        buffer.put_u8(0x01);
        let prev_length = buffer.len();

        let result = ConductorCodec::decoder().decode(&mut buffer);
        assert!(matches!(result, Ok(None)));
        assert_eq!(buffer.len(), prev_length);
    }

    #[test]
    fn decoder_returns_data_message() {
        const INPUT_DATA: &[u8] = &[
            0x56, 0b00000000, 0x33, 0x22, 0x11, 0x99, 0x03, 0x00, 0x01, 0x02, 0x03,
        ];

        let mut buffer = BytesMut::from(INPUT_DATA);
        let result = ConductorCodec::decoder().decode(&mut buffer);
        assert!(matches!(result, Ok(Some(ConductorMessage::Data {..}))));
        assert_eq!(buffer.len(), 0);

        if let Ok(Some(ConductorMessage::Data { channel, data })) = result {
            assert_eq!(channel.0, 0x99112233);
            assert_eq!(data.as_ref(), &[0x01, 0x02, 0x03]);
        }
    }

    #[test]
    fn decoder_returns_control_message() {
        const INPUT_DATA: &[u8] = &[
            0x56, 0b10000000, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x02, 0x03,
        ];

        let mut buffer = BytesMut::from(INPUT_DATA);
        let result = ConductorCodec::decoder().decode(&mut buffer);
        assert!(matches!(result, Ok(Some(ConductorMessage::Control(..)))));
        assert_eq!(buffer.len(), 0);

        if let Ok(Some(ConductorMessage::Control(data))) = result {
            assert_eq!(data.as_ref(), &[0x01, 0x02, 0x03]);
        }
    }

    #[test]
    fn decoder_parses_two_messages() {
        const INPUT_DATA: &[u8] = &[
            0x56, 0b10000000, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x02, 0x03, // control
            0x56, 0b00000000, 0x33, 0x22, 0x11, 0x99, 0x03, 0x00, 0x04, 0x05, 0x06, // data
        ];

        let mut buffer = BytesMut::from(INPUT_DATA);
        let mut decoder = ConductorCodec::decoder();

        let result = decoder.decode(&mut buffer);
        assert!(matches!(result, Ok(Some(ConductorMessage::Control(..)))));
        assert_eq!(buffer.len(), INPUT_DATA.len() / 2);

        let result = decoder.decode(&mut buffer);
        assert!(matches!(result, Ok(Some(ConductorMessage::Data {..}))));
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn decoder_accepts_zero_length_message() {
        const INPUT_DATA: &[u8] = &[0x56, 0b00000000, 0x33, 0x22, 0x11, 0x99, 0x00, 0x00];

        let mut buffer = BytesMut::from(INPUT_DATA);
        let result = ConductorCodec::decoder().decode(&mut buffer);
        assert!(matches!(result, Ok(Some(ConductorMessage::Data {..}))));
        assert_eq!(buffer.len(), 0);

        if let Ok(Some(ConductorMessage::Data { channel, data })) = result {
            assert_eq!(channel.0, 0x99112233);
            assert!(data.is_empty());
        }
    }
}
