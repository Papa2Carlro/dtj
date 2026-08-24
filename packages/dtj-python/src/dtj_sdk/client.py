"""Main client for dtj-agent communication."""

import socket
import time
import warnings
from pathlib import Path
from typing import Optional, Any, Dict
from contextlib import contextmanager

from .protocol import (
    PROTOCOL_VERSION,
    Cmd, Resp,
    DictKind, SEVERITY_MAP, TypeTag,
    encode_frame, decode_frame,
    encode_hello, decode_hello_ok,
    encode_open_session, encode_intern, decode_intern_ok,
    encode_append_event, decode_append_event_ok,
    encode_finish_session, encode_ping,
    decode_error,
    OpenSessionMetadata,
    encode_value,
    DTJProtocolError,
)
from .discovery import AgentDiscovery
from .exceptions import (
    DTJProtocolError as SDKProtocolError,
    DTJConnectionError,
    DTJAgentNotFoundError,
    DTJValueError,
    DTJSessionError,
)


class NoOpTraceSession:
    """No-op trace session for disabled mode."""
    
    def __init__(self, *args, **kwargs):
        self._warning_emitted = False
    
    def _emit_warning(self):
        if not self._warning_emitted:
            warnings.warn(
                "dtj-sdk: tracing disabled (no dtj-agent found). "
                "Install dtj-agent or set DTJ_AGENT_PATH to enable.",
                RuntimeWarning,
                stacklevel=2,
            )
            self._warning_emitted = True
    
    def emit(self, *args, **kwargs) -> None:
        self._emit_warning()
    
    def close(self) -> None:
        pass
    
    def __enter__(self):
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()
        return False


class TraceSession:
    """Active trace session connected to dtj-agent."""
    
    def __init__(
        self,
        sock: socket.socket,
        discovery: AgentDiscovery,
        metadata: OpenSessionMetadata,
    ):
        self._sock = sock
        self._discovery = discovery
        self._metadata = metadata
        self._closed = False
        
        # Dictionary caches
        self._domain_cache: Dict[str, int] = {}
        self._category_cache: Dict[str, int] = {}
        self._event_name_cache: Dict[str, int] = {}
        self._string_cache: Dict[str, int] = {}
    
    @classmethod
    def open(
        cls,
        producer_name: str,
        producer_version: str,
        data_dir: str | Path = "./traces",
        agent_path: Optional[str] = None,
        socket_path: Optional[str] = None,
        session_file_name: Optional[str] = None,
        enabled: bool = True,
    ) -> "TraceSession | NoOpTraceSession":
        """Open a new trace session."""
        
        if not enabled:
            return NoOpTraceSession()
        
        discovery = AgentDiscovery(
            agent_path=agent_path,
            socket_path=socket_path,
            data_dir=str(data_dir),
        )
        
        # Check if agent exists before trying to connect
        agent_binary = discovery.find_agent()
        if not agent_binary:
            return NoOpTraceSession()
        
        # Start or connect to agent
        try:
            actual_socket_path = discovery.start_agent()
        except DTJAgentNotFoundError:
            return NoOpTraceSession()
        except Exception as e:
            raise DTJConnectionError(f"Failed to start agent: {e}")
        
        # Connect to socket with retry
        sock = cls._connect_with_retry(actual_socket_path)
        
        try:
            # Hello handshake
            sock.sendall(encode_hello())
            response = cls._read_frame(sock)
            opcode, body = decode_frame(response)
            if opcode == Resp.ERROR:
                raise DTJProtocolError(f"Hello failed: {decode_error(body)}")
            if opcode != Resp.HELLO_OK:
                raise DTJProtocolError(f"Expected HelloOk, got {opcode:#x}")
            version = decode_hello_ok(body)
            if version != PROTOCOL_VERSION:
                raise DTJProtocolError(f"Protocol version mismatch: {version} != {PROTOCOL_VERSION}")
            
            # Generate metadata
            if session_file_name is None:
                session_file_name = f"session-{int(time.time() * 1000)}.dtj"
            
            metadata = OpenSessionMetadata.create(
                file_name=session_file_name,
                producer_name=producer_name,
                producer_version=producer_version,
            )
            
            # OpenSession
            open_frame = encode_open_session(
                file_name=metadata.file_name,
                session_id=metadata.session_id,
                start_utc_unix_ms=metadata.start_utc_unix_ms,
                mono_origin_ns=metadata.mono_origin_ns,
                producer_name=metadata.producer_name,
                producer_version=metadata.producer_version,
            )
            sock.sendall(open_frame)
            response = cls._read_frame(sock)
            opcode, body = decode_frame(response)
            if opcode == Resp.ERROR:
                raise DTJProtocolError(f"OpenSession failed: {decode_error(body)}")
            if opcode != Resp.OPEN_SESSION_OK:
                raise DTJProtocolError(f"Expected OpenSessionOk, got {opcode:#x}")
            
            return cls(sock, discovery, metadata)
            
        except Exception:
            discovery.stop_agent()
            sock.close()
            raise
    
    @staticmethod
    def _connect_with_retry(socket_path: str, timeout: float = 5.0) -> socket.socket:
        """Connect to Unix socket with retry."""
        deadline = time.monotonic() + timeout
        last_error = None
        
        while time.monotonic() < deadline:
            try:
                sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                sock.settimeout(5.0)
                sock.connect(socket_path)
                return sock
            except (ConnectionRefusedError, FileNotFoundError) as e:
                last_error = e
                time.sleep(0.01)
            except Exception as e:
                last_error = e
                break
        
        raise DTJConnectionError(f"Failed to connect to agent at {socket_path}: {last_error}")
    
    @staticmethod
    def _read_frame(sock: socket.socket) -> bytes:
        """Read a complete frame from socket."""
        # Read length (4 bytes)
        len_buf = b""
        while len(len_buf) < 4:
            chunk = sock.recv(4 - len(len_buf))
            if not chunk:
                raise DTJConnectionError("Socket closed unexpectedly")
            len_buf += chunk
        
        length = int.from_bytes(len_buf, "little")
        if length > 1_048_576:
            raise DTJProtocolError(f"Frame too large: {length}")
        
        # Read payload (length bytes including opcode)
        payload = b""
        while len(payload) < length:
            chunk = sock.recv(length - len(payload))
            if not chunk:
                raise DTJConnectionError("Socket closed unexpectedly")
            payload += chunk
        
        return len_buf + payload
    
    def _intern(self, kind: int, name: str) -> int:
        """Intern a string, using cache."""
        cache_map = {
            DictKind.DOMAIN: self._domain_cache,
            DictKind.CATEGORY: self._category_cache,
            DictKind.EVENT_NAME: self._event_name_cache,
            DictKind.STRING: self._string_cache,
        }
        cache = cache_map.get(kind)
        if cache is None:
            raise DTJValueError(f"Unknown dict kind: {kind}")
        
        if name in cache:
            return cache[name]
        
        frame = encode_intern(kind, name)
        self._sock.sendall(frame)
        response = self._read_frame(self._sock)
        opcode, body = decode_frame(response)
        
        if opcode == Resp.ERROR:
            raise DTJProtocolError(f"Intern failed: {decode_error(body)}")
        if opcode != Resp.INTERN_OK:
            raise DTJProtocolError(f"Expected InternOk, got {opcode:#x}")
        
        dict_id = decode_intern_ok(body)
        cache[name] = dict_id
        return dict_id
    
    def _get_or_intern_domain(self, name: str) -> int:
        return self._intern(DictKind.DOMAIN, name)
    
    def _get_or_intern_category(self, name: str) -> int:
        return self._intern(DictKind.CATEGORY, name)
    
    def _get_or_intern_event_name(self, name: str) -> int:
        return self._intern(DictKind.EVENT_NAME, name)
    
    def _get_or_intern_string(self, name: str) -> int:
        return self._intern(DictKind.STRING, name)
    
    def emit(
        self,
        domain: str,
        category: str,
        name: str,
        severity: str,
        field_name: str,
        value: Any,
        correlation: Optional[str] = None,
    ) -> int:
        """Emit a single event with one field."""
        if self._closed:
            raise DTJSessionError("Session already closed")
        
        # Validate severity
        severity_lower = severity.lower()
        if severity_lower not in SEVERITY_MAP:
            raise DTJValueError(f"Invalid severity: {severity}. Must be one of {list(SEVERITY_MAP.keys())}")
        severity_val = SEVERITY_MAP[severity_lower]
        
        # Get or intern dictionary entries
        domain_id = self._get_or_intern_domain(domain)
        category_id = self._get_or_intern_category(category)
        event_name_id = self._get_or_intern_event_name(name)
        correlation_id = self._get_or_intern_string(correlation) if correlation else 0
        field_name_id = self._get_or_intern_string(field_name)
        
        # Encode value
        if isinstance(value, str):
            # String values are interned
            value_id = self._get_or_intern_string(value)
            type_tag = TypeTag.INTERNED
            value_body = value_id.to_bytes(4, "little")
        else:
            type_tag, value_body = encode_value(value)
        
        # Current monotonic timestamp
        monotonic_ns = time.monotonic_ns()
        
        # Send AppendEvent
        frame = encode_append_event(
            monotonic_ns=monotonic_ns,
            domain_id=domain_id,
            category_id=category_id,
            event_name_id=event_name_id,
            correlation_id=correlation_id,
            severity=severity_val,
            field_name_id=field_name_id,
            type_tag=type_tag,
            value_body=value_body,
        )
        self._sock.sendall(frame)
        response = self._read_frame(self._sock)
        opcode, body = decode_frame(response)
        
        if opcode == Resp.ERROR:
            raise DTJProtocolError(f"AppendEvent failed: {decode_error(body)}")
        if opcode != Resp.APPEND_EVENT_OK:
            raise DTJProtocolError(f"Expected AppendEventOk, got {opcode:#x}")
        
        return decode_append_event_ok(body)
    
    def close(self) -> None:
        """Close the session and cleanup."""
        if self._closed:
            return
        
        self._closed = True
        
        try:
            # Send FinishSession
            self._sock.sendall(encode_finish_session())
            response = self._read_frame(self._sock)
            opcode, _ = decode_frame(response)
            if opcode == Resp.ERROR:
                # Log but don't raise on finish error
                pass
        except Exception:
            pass
        finally:
            try:
                self._sock.close()
            except Exception:
                pass
            self._discovery.stop_agent()
    
    def __enter__(self):
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()
        return False


class TraceConfig:
    """Configuration for TraceSession."""
    
    def __init__(
        self,
        data_dir: str | Path = "./traces",
        agent_path: Optional[str] = None,
        socket_path: Optional[str] = None,
        enabled: bool = True,
        session_file_name: Optional[str] = None,
    ):
        self.data_dir = Path(data_dir)
        self.agent_path = agent_path
        self.socket_path = socket_path
        self.enabled = enabled
        self.session_file_name = session_file_name
    
    def open_session(
        self,
        producer_name: str,
        producer_version: str,
    ) -> TraceSession | NoOpTraceSession:
        """Open a trace session with this config."""
        return TraceSession.open(
            producer_name=producer_name,
            producer_version=producer_version,
            data_dir=self.data_dir,
            agent_path=self.agent_path,
            socket_path=self.socket_path,
            session_file_name=self.session_file_name,
            enabled=self.enabled,
        )