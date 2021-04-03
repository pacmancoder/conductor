//! Conductor protocol
//!
//! ### Flags (bits table)
//! |   **bit**   |  7  |  6  |  5  |  4  |  3  |  2  |  1  |  0  |
//! | ----------- | --- | --- | --- | --- | --- | --- | --- | --- |
//! | **purpose** |  0  |  0  |  0  |  0  |  0  |  0  |  0  |  0  |
//!
//! ### Packet data layout (bytes table)
//!
//! | **position** | buffer[0]  | buffer[1]...buffer[2] | buffer[3]...buffer[4]  |
//! | ------------ | ---------- | --------------------- | ---------------------- |
//! | **example**  | 0x56       |  0xDEADBEAF           | 0xCAFE                 |
//! | **purpose**  | marker     |  channel_id           | payload_size           |
//! | **type**     | u8         |  u16, LE              | u16, LE                |

use byteorder::ReadBytesExt;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use thiserror::Error;
use tokio_util::codec::{Decoder, Encoder};
use uuid::Uuid;

mod control;
pub mod tunnel;

const HEADER_SIZE: usize = 5;
const HEADER_MARKER_VALUE: u8 = 0x56;

/// Unique channel id between peer and conductor-server instance
pub type DataChannelId = u16;

/// Unique Client <-> Host tunnel identifier
pub type TunnelId = Uuid;

/// Auth JWT token
pub type AuthToken = String;

pub struct ConductorMessage {
    pub channel: DataChannelId,
    pub data: Bytes,
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
        let payload_size = item.data.len();

        if payload_size > u16::max_value() as usize {
            log::error!("Control payload size can't be bigger than u16 can fit");
            return Err(ConductorProtoError::TooBigPayload);
        }

        dst.reserve(HEADER_SIZE + payload_size);
        dst.put_u8(HEADER_MARKER_VALUE);
        dst.put_u16_le(item.channel);
        dst.put_u16_le(payload_size as u16);
        dst.extend_from_slice(&item.data);

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

        let channel = reader.read_u16::<byteorder::LittleEndian>().unwrap();

        let payload_size = reader.read_u16::<byteorder::LittleEndian>().unwrap() as usize;
        let current_frame_size = HEADER_SIZE + payload_size;

        if src.len() < current_frame_size {
            // reserve place for the current frame and the next header for better efficiency
            src.reserve(current_frame_size + HEADER_SIZE);
            return Ok(None);
        }

        src.advance(HEADER_SIZE);
        let data = src.split_to(payload_size).freeze();

        Ok(Some(ConductorMessage { channel, data }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_message_encode() {
        const EXPECTED: &[u8] = &[0x56, 0x34, 0x12, 0x03, 0x00, 0x01, 0x02, 0x03];

        let mut actual = BytesMut::with_capacity(16);
        ConductorCodec::encoder()
            .encode(
                ConductorMessage {
                    channel: 0x1234,
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
            ConductorMessage {
                channel: 0x2233,
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
    fn decoder_requests_more_data_to_read_header() {
        const INPUT_DATA: &[u8] = &[0x56, 0x00];

        let mut buffer = BytesMut::from(INPUT_DATA);
        let result = ConductorCodec::decoder().decode(&mut buffer);
        assert!(matches!(result, Ok(None)));
        assert_eq!(buffer.len(), INPUT_DATA.len());
    }

    #[test]
    fn decoder_requests_more_data_to_read_payload() {
        const INPUT_DATA: &[u8] = &[0x56, 0x33, 0x22, 0x03, 0x00];

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
        const INPUT_DATA: &[u8] = &[0x56, 0x33, 0x22, 0x03, 0x00, 0x01, 0x02, 0x03];

        let mut buffer = BytesMut::from(INPUT_DATA);
        let result = ConductorCodec::decoder().decode(&mut buffer);
        assert!(matches!(result, Ok(Some(ConductorMessage {..}))));
        assert_eq!(buffer.len(), 0);

        if let Ok(Some(ConductorMessage { channel, data })) = result {
            assert_eq!(channel, 0x2233);
            assert_eq!(data.as_ref(), &[0x01, 0x02, 0x03]);
        }
    }

    #[test]
    fn decoder_parses_two_messages() {
        const INPUT_DATA: &[u8] = &[
            0x56, 0x00, 0x00, 0x03, 0x00, 0x01, 0x02, 0x03, // control
            0x56, 0x33, 0x22, 0x03, 0x00, 0x04, 0x05, 0x06, // data
        ];

        let mut buffer = BytesMut::from(INPUT_DATA);
        let mut decoder = ConductorCodec::decoder();

        let result = decoder.decode(&mut buffer);
        assert!(matches!(result, Ok(Some(ConductorMessage {..}))));
        assert_eq!(buffer.len(), INPUT_DATA.len() / 2);

        let result = decoder.decode(&mut buffer);
        assert!(matches!(result, Ok(Some(ConductorMessage {..}))));
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn decoder_accepts_zero_length_message() {
        const INPUT_DATA: &[u8] = &[0x56, 0x33, 0x22, 0x00, 0x00];

        let mut buffer = BytesMut::from(INPUT_DATA);
        let result = ConductorCodec::decoder().decode(&mut buffer);
        assert!(matches!(result, Ok(Some(ConductorMessage {..}))));
        assert_eq!(buffer.len(), 0);

        if let Ok(Some(ConductorMessage { channel, data })) = result {
            assert_eq!(channel, 0x2233);
            assert!(data.is_empty());
        }
    }
}
