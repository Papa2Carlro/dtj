#[derive(Debug, PartialEq)]
pub enum Error {
    FrameTooLarge,
    BadLength,
    BadName,
    BadVersion,
    BadIntern,
    BadBytes,
    IoError,
    Protocol,
    SessionClosed,
    AgentNotFound,
    NotExecutable,
    SocketNotFound,
    ConnectionFailed,
    Disabled,
    NotConnected,
}

impl Error {
    /// Returns true if this is a "not found" type error
    pub fn is_not_found(&self) -> bool {
        matches!(self, Error::AgentNotFound | Error::SocketNotFound)
    }
}

impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Error::IoError
    }
}
