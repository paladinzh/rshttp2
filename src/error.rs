#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Code {
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

pub const ALL_ERRORS: [Code; 14] = [
    Code::NoError,
    Code::ProtocolError,
    Code::InternalError,
    Code::FlowControlError,
    Code::SettingsTimeout,
    Code::StreamClosed,
    Code::FrameSizeError,
    Code::RefusedStream,
    Code::Cancel,
    Code::CompressionError,
    Code::ConnectError,
    Code::EnhanceYourCalm,
    Code::InadequateSecurity,
    Code::Http1Required,
];

impl Code {
    pub fn from_h2_id(id: usize) -> Code {
        assert!(id < ALL_ERRORS.len(), "id={}", id);
        ALL_ERRORS[id].clone()
    }

    pub fn to_h2_id(&self) -> usize {
        match self {
            Code::NoError => 0,
            Code::ProtocolError => 1,
            Code::InternalError => 2,
            Code::FlowControlError => 3,
            Code::SettingsTimeout => 4,
            Code::StreamClosed => 5,
            Code::FrameSizeError => 6,
            Code::RefusedStream => 7,
            Code::Cancel => 8,
            Code::CompressionError => 9,
            Code::ConnectError => 10,
            Code::EnhanceYourCalm => 11,
            Code::InadequateSecurity => 12,
            Code::Http1Required => 13,
        }
    }
}

#[derive(Debug)]
pub enum Level {
    StreamLevel,
    ConnectionLevel,
}

#[derive(Debug)]
pub struct Error {
    level: Level,
    code: Code,
    message: String,
    cause: Option<tokio::io::Error>,
}

impl Error {
    pub fn new(
        level: Level,
        code: Code,
        message: String) -> Error {
        let desp = format!(
                "Code: {:?}, with details \"{}\"", code, message);
        Error{
            level,
            code,
            message: desp,
            cause: None,
        }
    }

    pub fn new_with_cause(
        level: Level,
        code: Code,
        message: String,
        cause: tokio::io::Error
    ) -> Error {
        let desp = format!(
            "Code: {:?}, with details \"{}\", caused by {}",
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn errorcode() {
        for oracle in &ALL_ERRORS {
            let x = oracle.to_h2_id();
            let trial = Code::from_h2_id(x);
            assert_eq!(trial, *oracle);
        }
    }
}

