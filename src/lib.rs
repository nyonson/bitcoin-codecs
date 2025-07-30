//! Push-based bitcoin protocol codecs using the `push_decode` library.
//!
//! # RPIT vs. Type Wrappers
//!
//! This library uses RPIT (Return Position Impl Trait) for decoder functions rather than
//! concrete wrapper types. This shields the caller from the gnarly types of the composed
//! codecs, but also makes it so they are not name-able. This shouldn't be too much of an
//! issues since the codecs are designed to be consumed quickly by the callers, not held
//! onto in a struct.
//!
//! ```rust,ignore
//! // Type wrapper approach - wraps the complex composed type:
//! use push_decode::{U32Le, ArrayDecoder, Chain};
//!
//! pub struct HeaderDecoder {
//!     inner: Chain<Chain<Chain<ArrayDecoder<4>, ArrayDecoder<12>>, U32Le>, ArrayDecoder<4>>,
//! }
//!
//! impl HeaderDecoder {
//!     pub fn new() -> Self {
//!         let inner = ArrayDecoder::<4>
//!             .chain(ArrayDecoder::<12>)
//!             .chain(U32Le)
//!             .chain(ArrayDecoder::<4>);
//!         Self { inner }
//!     }
//! }
//! ```
//!
//! # Caller I/O Ergonomics
//!
//! The codecs are sans-io, which places the burden on the caller to "push" bytes
//! in to the decoder and "pull" bytes through the encoder. However, the [`push_decode`]
//! library has great I/O wrappers for the codecs, but now the challenges is how to
//! make these discoverable for callers.
//!
//! 1. Document how a calling crate should depend on [`push_decode`] with its I/O of
//! choice feature flag enabled (e.g. `std`) and then import the I/O driver.
//! 2. Add extension traits to the library which delegate to [`push_decode`] drivers.

#![no_std]

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

use alloc::vec::Vec;
use bitcoin::consensus::{encode, Decodable};
use bitcoin::network::message::{CommandString, NetworkMessage};
use bitcoin::network::Magic;
use bitcoin::Network;
use push_decode::{Decoder, PushDecodeError};

/// A decoded Bitcoin message header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// Network magic bytes.
    pub magic: Magic,
    /// Command name.
    pub command: CommandString,
    /// Payload length.
    pub length: u32,
    /// Payload checksum.
    pub checksum: [u8; 4],
}

/// Creates a header decoder using push_decode's composition.
fn header_decoder() -> impl Decoder<Item = Header, Error = DecodeError> {
    use push_decode::{ArrayDecoder, U32Le};

    // Decode: magic (4 bytes) + command (12 bytes) + length (4 bytes) + checksum (4 bytes)
    ArrayDecoder::<4>
        .chain(ArrayDecoder::<12>)
        .chain(U32Le)
        .chain(ArrayDecoder::<4>)
        .map(|(((magic_bytes, command_bytes), length), checksum)| {
            let magic = Magic::from_bytes(&magic_bytes);
            let command = CommandString::try_from(&command_bytes[..])
                .map_err(|_| DecodeError::InvalidCommand)?;

            if length > 32 * 1024 * 1024 {
                return Err(DecodeError::PayloadTooLarge(length as usize));
            }

            Ok(Header {
                magic,
                command,
                length,
                checksum,
            })
        })
        .and_then(|result| result)
}

/// Creates a payload decoder that validates checksum and returns raw bytes.
fn payload_decoder(header: Header) -> impl Decoder<Item = Vec<u8>, Error = DecodeError> {
    use push_decode::VecDecoder;

    VecDecoder::new(header.length as usize)
        .map(move |payload| {
            let checksum = sha256d_checksum(&payload);
            if checksum != header.checksum {
                Err(DecodeError::InvalidChecksum)
            } else {
                Ok(payload)
            }
        })
        .and_then(|result| result)
}

/// Creates a V1 frame decoder for a specific network.
pub fn v1_frame_decoder(
    network: Network,
) -> impl Decoder<Item = NetworkMessage, Error = DecodeError> {
    header_decoder()
        .and_then(move |header| {
            // Validate network
            if header.magic != network.magic() {
                return Err(DecodeError::WrongMagic {
                    expected: network.magic(),
                    actual: header.magic,
                });
            }
            Ok(header)
        })
        .chain(|header| payload_decoder(header))
        .map(|(_header, payload_bytes)| {
            NetworkMessage::consensus_decode(&mut &payload_bytes[..])
                .map_err(|e| DecodeError::InvalidPayload(e))
        })
        .and_then(|result| result)
}

/// Errors that can occur during decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Wrong network magic bytes.
    WrongMagic { expected: Magic, actual: Magic },
    /// Invalid command string.
    InvalidCommand,
    /// Payload size exceeds maximum allowed (32MB).
    PayloadTooLarge(usize),
    /// Checksum verification failed.
    InvalidChecksum,
    /// Header incomplete.
    IncompleteHeader,
    /// Payload incomplete.
    IncompletePayload,
    /// Failed to decode payload contents into a valid NetworkMessage.
    InvalidPayload(encode::Error),
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::WrongMagic { expected, actual } => {
                write!(f, "wrong magic: expected {:?}, got {:?}", expected, actual)
            }
            DecodeError::InvalidCommand => write!(f, "invalid command string"),
            DecodeError::PayloadTooLarge(size) => write!(f, "payload too large: {} bytes", size),
            DecodeError::InvalidChecksum => write!(f, "checksum verification failed"),
            DecodeError::IncompleteHeader => write!(f, "incomplete header"),
            DecodeError::IncompletePayload => write!(f, "incomplete payload"),
            DecodeError::InvalidPayload(e) => write!(f, "invalid payload: {}", e),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for DecodeError {}

/// Calculate SHA256d checksum (first 4 bytes of SHA256(SHA256(data))).
fn sha256d_checksum(data: &[u8]) -> [u8; 4] {
    use bitcoin::hashes::{sha256d, Hash};

    let hash = sha256d::Hash::hash(data);
    let mut checksum = [0u8; 4];
    checksum.copy_from_slice(&hash[..4]);
    checksum
}
