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

#[derive(Debug)]
pub enum ErrorLevel {
    StreamLevel,
    ConnectionLevel,
}

#[derive(Debug)]
pub struct Error {
    level: ErrorLevel,
    code: ErrorCode,
    message: String,
    cause: Option<tokio::io::Error>,
}

impl Error {
    pub fn new(
        level: ErrorLevel,
        code: ErrorCode,
        message: String) -> Error {
        let desp = format!(
                "ErrorCode: {:?}, with details \"{}\"", code, message);
        Error{
            level,
            code,
            message: desp,
            cause: None,
        }
    }

    pub fn new_with_cause(
        level: ErrorLevel,
        code: ErrorCode,
        message: String,
        cause: tokio::io::Error
    ) -> Error {
        let desp = format!(
            "ErrorCode: {:?}, with details \"{}\", caused by {}",
            code,
            message,
            cause);
        Error{
            level,
            code,
            message: desp,
            cause: Some(cause),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        self.message.as_str()
    }

    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.cause {
            None => None,
            Some(ref err) => Some(err),
        }
    }
}

