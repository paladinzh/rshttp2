#[derive(Debug, Clone)]
pub enum ErrorCode {
    NoError,
    ProtocolError,
    InternalError,
    FlowControlError,
    SettingsTimeout,
    StreamClosed,
    FrameSizeError,
    RefusedStream,
    Cancel,
    CompressionError,
    ConnectError,
    EnhanceYourCalm,
    InadequateSecurity,
    Http1Required,
}

pub const ALL_ERRORS: [ErrorCode; 14] = [
    ErrorCode::NoError,
    ErrorCode::ProtocolError,
    ErrorCode::InternalError,
    ErrorCode::FlowControlError,
    ErrorCode::SettingsTimeout,
    ErrorCode::StreamClosed,
    ErrorCode::FrameSizeError,
    ErrorCode::RefusedStream,
    ErrorCode::Cancel,
    ErrorCode::CompressionError,
    ErrorCode::ConnectError,
    ErrorCode::EnhanceYourCalm,
    ErrorCode::InadequateSecurity,
    ErrorCode::Http1Required,
];

impl ErrorCode {
    pub fn from_h2_id(id: usize) -> ErrorCode {
        assert!(id < ALL_ERRORS.len(), "id={}", id);
        ALL_ERRORS[id].clone()
    }
}


