//! Push-based bitcoin protocol codecs using the `push_decode` library.
//!
//! ## Caller I/O Ergonomics
//!
//! The codecs are sans-io, which places the burden on the caller to "push" bytes
//! in to the decoder and "pull" bytes through the encoder. However, the [`push_decode`]
//! library has great I/O wrappers for the codecs, but now the challenges is how to
//! make these discoverable for callers.
//!
//! 1. Document how a calling crate should depend on [`push_decode`] with its I/O of
//!    choice feature flag enabled (e.g. `std`) and then import the I/O driver.
//! 2. Add extension traits to the library which delegate to [`push_decode`] drivers.

#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

use bitcoin::{
    consensus::encode,
    p2p::{
        message::{CommandString, NetworkMessage, RawNetworkMessage},
        Magic,
    },
    Network,
};
use either::Either;
use push_decode::{
    decoders::{
        combinators::{Chain, Then},
        ByteArrayDecoder, ByteVecDecoder, IntDecoder,
    },
    int::LittleEndian,
    Decoder,
};

/// A decoded Bitcoin message header.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Header {
    /// Network magic bytes.
    pub magic: Magic,
    /// Command name.
    pub command: CommandString,
    /// Payload length.
    pub length: u32,
    /// Payload checksum.
    pub checksum: [u8; 4],
}

// Type alias for the decoder chain that parses raw header bytes
type RawHeaderDecoder = Chain<
    Chain<Chain<ByteArrayDecoder<4>, ByteArrayDecoder<12>>, IntDecoder<u32, LittleEndian>>,
    ByteArrayDecoder<4>,
>;

/// Decoder for bitcoin v1 transport message headers.
struct HeaderDecoder {
    inner: RawHeaderDecoder,
    expected_magic: Magic,
}

impl HeaderDecoder {
    fn new(expected_magic: Magic) -> Self {
        Self {
            inner: ByteArrayDecoder::<4>::new()
                .chain(ByteArrayDecoder::<12>::new())
                .chain(IntDecoder::<u32, LittleEndian>::new())
                .chain(ByteArrayDecoder::<4>::new()),
            expected_magic,
        }
    }
}

impl Decoder for HeaderDecoder {
    type Value = Header;
    type Error = DecodeError;

    fn decode_chunk(&mut self, bytes: &mut &[u8]) -> Result<(), Self::Error> {
        self.inner.decode_chunk(bytes)?;
        Ok(())
    }

    fn end(self) -> Result<Self::Value, Self::Error> {
        // Extract the raw values from the inner decoder and validate.
        let (((magic_bytes, command_bytes), length), checksum) = self.inner.end()?;
        let magic = Magic::from_bytes(magic_bytes);
        let command = encode::deserialize::<CommandString>(&command_bytes[..])
            .map_err(|_| DecodeError::InvalidCommand)?;

        if magic != self.expected_magic {
            return Err(DecodeError::WrongMagic {
                expected: self.expected_magic,
                actual: magic,
            });
        }

        if length > 32 * 1024 * 1024 {
            return Err(DecodeError::PayloadTooLarge(length as usize));
        }

        Ok(Header {
            magic,
            command,
            length,
            checksum,
        })
    }
}

/// Decoder for Bitcoin message payloads
struct PayloadDecoder {
    inner: ByteVecDecoder,
    expected_checksum: [u8; 4],
}

impl PayloadDecoder {
    pub fn new(header: Header) -> Self {
        Self {
            inner: ByteVecDecoder::new(header.length as usize),
            expected_checksum: header.checksum,
        }
    }
}

impl Decoder for PayloadDecoder {
    type Value = NetworkMessage;
    type Error = DecodeError;

    fn decode_chunk(&mut self, bytes: &mut &[u8]) -> Result<(), Self::Error> {
        self.inner.decode_chunk(bytes)?;
        Ok(())
    }

    fn end(self) -> Result<Self::Value, Self::Error> {
        let payload_bytes = self.inner.end()?;

        // Validate checksum
        let checksum = sha256d_checksum(&payload_bytes);
        if checksum != self.expected_checksum {
            return Err(DecodeError::InvalidChecksum);
        }

        // Decode the network message
        let message = encode::deserialize::<RawNetworkMessage>(&payload_bytes[..])
            .map_err(DecodeError::InvalidPayload)?;
        Ok(message.into_payload())
    }
}

// Type alias for the decoder chain.
type V1DecoderInner = Then<HeaderDecoder, PayloadDecoder, fn(Header) -> PayloadDecoder>;

/// Decoder for Bitcoin V1 protocol messages
pub struct V1MessageDecoder {
    inner: V1DecoderInner,
}

impl V1MessageDecoder {
    /// Creates a new V1 message decoder for the specified network
    pub fn new(network: Network) -> Self {
        Self {
            inner: HeaderDecoder::new(network.magic()).then(PayloadDecoder::new),
        }
    }
}

impl Decoder for V1MessageDecoder {
    type Value = NetworkMessage;
    type Error = DecodeError;

    fn decode_chunk(&mut self, bytes: &mut &[u8]) -> Result<(), Self::Error> {
        self.inner.decode_chunk(bytes)?;
        Ok(())
    }

    fn end(self) -> Result<Self::Value, Self::Error> {
        let value = self.inner.end()?;
        Ok(value)
    }
}

/// Errors that can occur during decoding.
#[derive(Debug)]
pub enum DecodeError {
    /// Wrong network magic bytes.
    WrongMagic { expected: Magic, actual: Magic },
    /// Invalid command string.
    InvalidCommand,
    /// Payload size exceeds maximum allowed (32MB).
    PayloadTooLarge(usize),
    /// Checksum verification failed.
    InvalidChecksum,
    /// Message incomplete.
    IncompleteMessage,
    /// Failed to decode payload contents into a valid NetworkMessage.
    InvalidPayload(encode::Error),
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::WrongMagic { expected, actual } => {
                write!(f, "wrong magic: expected {expected:?}, got {actual:?}")
            }
            DecodeError::InvalidCommand => write!(f, "invalid command string"),
            DecodeError::PayloadTooLarge(size) => write!(f, "payload too large: {size} bytes"),
            DecodeError::InvalidChecksum => write!(f, "checksum verification failed"),
            DecodeError::IncompleteMessage => write!(f, "incomplete message"),
            DecodeError::InvalidPayload(e) => write!(f, "invalid payload: {e}"),
        }
    }
}

impl From<push_decode::error::UnexpectedEnd> for DecodeError {
    fn from(_: push_decode::error::UnexpectedEnd) -> Self {
        DecodeError::IncompleteMessage
    }
}

impl<L, R> From<Either<L, R>> for DecodeError
where
    DecodeError: From<L>,
    DecodeError: From<R>,
{
    fn from(err: Either<L, R>) -> Self {
        match err {
            Either::Left(l) => DecodeError::from(l),
            Either::Right(r) => DecodeError::from(r),
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
