"""dtj-sdk — Python SDK for DTJ Agent.

A thin middleware client that communicates with the local dtj-agent binary
via Unix domain socket. The SDK never writes .dtj bytes directly.
"""

from .client import TraceSession, TraceConfig, NoOpTraceSession
from .exceptions import (
    DTJError,
    DTJProtocolError,
    DTJConnectionError,
    DTJAgentNotFoundError,
    DTJValueError,
    DTJSessionError,
)

__version__ = "0.1.0"

__all__ = [
    "TraceSession",
    "TraceConfig",
    "NoOpTraceSession",
    "DTJError",
    "DTJProtocolError",
    "DTJConnectionError",
    "DTJAgentNotFoundError",
    "DTJValueError",
    "DTJSessionError",
]