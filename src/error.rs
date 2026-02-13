use std::sync::Arc;

use n0_error::stack_error;
use thiserror::Error;

use crate::{ConnectError, SettingsError};

/// An error returned when connecting to a WebTransport endpoint.
#[stack_error(derive, from_sources)]
#[derive(Clone)]
pub enum ClientError {
    #[error("unexpected end of stream")]
    UnexpectedEnd,

    #[error("failed to connect")]
    Connect(#[error(source)] Arc<iroh::endpoint::ConnectError>),

    #[error("connection failed")]
    Connection(#[error(source, std_err)] iroh::endpoint::ConnectionError),

    #[error("failed to write")]
    WriteError(#[error(source, std_err)] iroh::endpoint::WriteError),

    #[error("failed to read")]
    ReadError(#[error(source, std_err)] iroh::endpoint::ReadError),

    #[error("failed to exchange h3 settings")]
    SettingsError(#[error(from, source, std_err)] SettingsError),

    #[error("failed to exchange h3 connect")]
    HttpError(#[error(from, source, std_err)] ConnectError),

    #[error("invalid URL")]
    InvalidUrl,

    #[error("endpoint failed to bind")]
    Bind(#[error(source)] Arc<iroh::endpoint::BindError>),
}

/// An errors returned by [`crate::Session`], split based on if they are underlying QUIC errors or WebTransport errors.
#[derive(Clone, Error, Debug)]
pub enum SessionError {
    #[error("connection error: {0}")]
    ConnectionError(#[from] iroh::endpoint::ConnectionError),

    #[error("webtransport error: {0}")]
    WebTransportError(#[from] WebTransportError),

    #[error("send datagram error: {0}")]
    SendDatagramError(#[from] iroh::endpoint::SendDatagramError),
}

/// An error that can occur when reading/writing the WebTransport stream header.
#[derive(Clone, Error, Debug)]
pub enum WebTransportError {
    #[error("closed: code={0} reason={1}")]
    Closed(u32, String),

    #[error("unknown session")]
    UnknownSession,

    #[error("read error: {0}")]
    ReadError(#[from] iroh::endpoint::ReadExactError),

    #[error("write error: {0}")]
    WriteError(#[from] iroh::endpoint::WriteError),
}

/// An error when writing to [`crate::SendStream`]. Similar to [`iroh::endpoint::WriteError`].
#[derive(Clone, Error, Debug)]
pub enum WriteError {
    #[error("STOP_SENDING: {0}")]
    Stopped(u32),

    #[error("invalid STOP_SENDING: {0}")]
    InvalidStopped(iroh::endpoint::VarInt),

    #[error("session error: {0}")]
    SessionError(#[from] SessionError),

    #[error("stream closed")]
    ClosedStream,
}

impl From<iroh::endpoint::WriteError> for WriteError {
    fn from(e: iroh::endpoint::WriteError) -> Self {
        match e {
            iroh::endpoint::WriteError::Stopped(code) => {
                match web_transport_proto::error_from_http3(code.into_inner()) {
                    Some(code) => WriteError::Stopped(code),
                    None => WriteError::InvalidStopped(code),
                }
            }
            iroh::endpoint::WriteError::ClosedStream => WriteError::ClosedStream,
            iroh::endpoint::WriteError::ConnectionLost(e) => WriteError::SessionError(e.into()),
            iroh::endpoint::WriteError::ZeroRttRejected => unreachable!("0-RTT not supported"),
        }
    }
}

/// An error when reading from [`crate::RecvStream`]. Similar to [`iroh::endpoint::ReadError`].
#[derive(Clone, Error, Debug)]
pub enum ReadError {
    #[error("session error: {0}")]
    SessionError(#[from] SessionError),

    #[error("RESET_STREAM: {0}")]
    Reset(u32),

    #[error("invalid RESET_STREAM: {0}")]
    InvalidReset(iroh::endpoint::VarInt),

    #[error("stream already closed")]
    ClosedStream,
}

impl From<iroh::endpoint::ReadError> for ReadError {
    fn from(value: iroh::endpoint::ReadError) -> Self {
        match value {
            iroh::endpoint::ReadError::Reset(code) => {
                match web_transport_proto::error_from_http3(code.into_inner()) {
                    Some(code) => ReadError::Reset(code),
                    None => ReadError::InvalidReset(code),
                }
            }
            iroh::endpoint::ReadError::ConnectionLost(e) => Self::SessionError(e.into()),
            iroh::endpoint::ReadError::ClosedStream => Self::ClosedStream,
            iroh::endpoint::ReadError::ZeroRttRejected => unreachable!("0-RTT not supported"),
        }
    }
}

/// An error returned by [`crate::RecvStream::read_exact`]. Similar to [`iroh::endpoint::ReadExactError`].
#[derive(Clone, Error, Debug)]
pub enum ReadExactError {
    #[error("finished early")]
    FinishedEarly(usize),

    #[error("read error: {0}")]
    ReadError(#[from] ReadError),
}

impl From<iroh::endpoint::ReadExactError> for ReadExactError {
    fn from(e: iroh::endpoint::ReadExactError) -> Self {
        match e {
            iroh::endpoint::ReadExactError::FinishedEarly(size) => {
                ReadExactError::FinishedEarly(size)
            }
            iroh::endpoint::ReadExactError::ReadError(e) => ReadExactError::ReadError(e.into()),
        }
    }
}

/// An error returned by [`crate::RecvStream::read_to_end`]. Similar to [`iroh::endpoint::ReadToEndError`].
#[derive(Clone, Error, Debug)]
pub enum ReadToEndError {
    #[error("too long")]
    TooLong,

    #[error("read error: {0}")]
    ReadError(#[from] ReadError),
}

impl From<iroh::endpoint::ReadToEndError> for ReadToEndError {
    fn from(e: iroh::endpoint::ReadToEndError) -> Self {
        match e {
            iroh::endpoint::ReadToEndError::TooLong => ReadToEndError::TooLong,
            iroh::endpoint::ReadToEndError::Read(e) => ReadToEndError::ReadError(e.into()),
        }
    }
}

/// An error indicating the stream was already closed.
#[derive(Clone, Error, Debug)]
#[error("stream closed")]
pub struct ClosedStream;

impl From<iroh::endpoint::ClosedStream> for ClosedStream {
    fn from(_: iroh::endpoint::ClosedStream) -> Self {
        ClosedStream
    }
}

/// An error returned when receiving a new WebTransport session.
#[stack_error(derive, from_sources)]
#[derive(Clone)]
pub enum ServerError {
    #[error("unexpected end of stream")]
    UnexpectedEnd,

    #[error("connection failed")]
    Connection(#[error(source, std_err)] iroh::endpoint::ConnectionError),

    #[error("connection failed during handshake")]
    Connecting(#[error(source)] Arc<iroh::endpoint::ConnectingError>),

    #[error("failed to write")]
    WriteError(#[error(source, std_err)] iroh::endpoint::WriteError),

    #[error("failed to read")]
    ReadError(#[error(source, std_err)] iroh::endpoint::ReadError),

    #[error("io error")]
    IoError(#[error(source)] Arc<std::io::Error>),

    #[error("failed to bind endpoint")]
    Bind(#[error(source)] Arc<iroh::endpoint::BindError>),

    #[error("failed to exchange h3 connect")]
    HttpError(#[error(source, from, std_err)] ConnectError),

    #[error("failed to exchange h3 settings")]
    SettingsError(#[error(source, from, std_err)] SettingsError),
}

impl web_transport_trait::Error for SessionError {
    fn session_error(&self) -> Option<(u32, String)> {
        if let SessionError::WebTransportError(WebTransportError::Closed(code, reason)) = self {
            return Some((*code, reason.to_string()));
        }

        None
    }
}

impl web_transport_trait::Error for WriteError {
    fn session_error(&self) -> Option<(u32, String)> {
        if let WriteError::SessionError(e) = self {
            return e.session_error();
        }

        None
    }

    fn stream_error(&self) -> Option<u32> {
        match self {
            WriteError::Stopped(code) => Some(*code),
            _ => None,
        }
    }
}

impl web_transport_trait::Error for ReadError {
    fn session_error(&self) -> Option<(u32, String)> {
        if let ReadError::SessionError(e) = self {
            return e.session_error();
        }

        None
    }

    fn stream_error(&self) -> Option<u32> {
        match self {
            ReadError::Reset(code) => Some(*code),
            _ => None,
        }
    }
}
