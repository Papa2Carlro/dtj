"""Custom exceptions for dtj-sdk."""


class DTJError(Exception):
    """Base exception for dtj-sdk errors."""
    pass


class DTJProtocolError(DTJError):
    """Raised when the agent returns an error frame or protocol violation occurs."""
    def __init__(self, message: str, opcode: int | None = None):
        super().__init__(message)
        self.opcode = opcode


class DTJConnectionError(DTJError):
    """Raised when connection to agent fails."""
    pass


class DTJAgentNotFoundError(DTJError):
    """Raised when dtj-agent binary cannot be found."""
    pass


class DTJValueError(DTJError):
    """Raised when an unsupported value type is provided."""
    pass


class DTJSessionError(DTJError):
    """Raised when session operations fail (e.g., not opened, already closed)."""
    pass