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
}

impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Error::IoError
    }
}
