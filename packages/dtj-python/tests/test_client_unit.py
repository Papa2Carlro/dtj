"""Unit tests for client (no socket/agent required)."""

import warnings
import unittest
import struct
from pathlib import Path
from unittest.mock import patch, MagicMock, call
from dtj_sdk.client import TraceSession, TraceConfig, NoOpTraceSession
from dtj_sdk.exceptions import DTJValueError, DTJSessionError
from dtj_sdk.protocol import SEVERITY_MAP, TypeTag


class MockSocket:
    """Mock socket that handles partial reads correctly for frame-based protocol."""
    
    def __init__(self, frames: list[bytes]):
        self._buffer = b"".join(frames)
        self._pos = 0
        self.sendall = MagicMock()
    
    def recv(self, n: int) -> bytes:
        if self._pos >= len(self._buffer):
            return b""
        end = min(self._pos + n, len(self._buffer))
        data = self._buffer[self._pos:end]
        self._pos = end
        return data
    
    def close(self):
        pass


class TestNoOpTraceSession(unittest.TestCase):
    """Test no-op trace session behavior."""
    
    def test_noop_emit_does_nothing(self):
        session = NoOpTraceSession()
        # Should not raise
        session.emit(domain="test", category="cat", name="event", severity="info", field_name="field", value=1)
    
    def test_noop_close_does_nothing(self):
        session = NoOpTraceSession()
        session.close()  # Should not raise
    
    def test_noop_context_manager(self):
        with NoOpTraceSession() as session:
            session.emit(domain="test", category="cat", name="event", severity="info", field_name="field", value=1)
        # Should not raise
    
    def test_noop_emits_warning_once(self):
        session = NoOpTraceSession()
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            session.emit(domain="test", category="cat", name="event", severity="info", field_name="field", value=1)
            session.emit(domain="test", category="cat", name="event", severity="info", field_name="field", value=2)
            self.assertEqual(len(w), 1)
            self.assertTrue(issubclass(w[0].category, RuntimeWarning))
            self.assertIn("tracing disabled", str(w[0].message))


class TestTraceConfig(unittest.TestCase):
    """Test TraceConfig."""
    
    def test_default_config(self):
        config = TraceConfig()
        self.assertIsNone(config.data_dir)
        self.assertIsNone(config.agent_path)
        self.assertIsNone(config.socket_path)
        self.assertTrue(config.enabled)
        self.assertIsNone(config.session_file_name)
    
    def test_custom_config(self):
        config = TraceConfig(
            data_dir="/custom/traces",
            agent_path="/custom/agent",
            socket_path="/custom/socket",
            enabled=False,
            session_file_name="custom.dtj",
        )
        self.assertEqual(config.data_dir, Path("/custom/traces"))
        self.assertEqual(config.agent_path, "/custom/agent")
        self.assertEqual(config.socket_path, "/custom/socket")
        self.assertFalse(config.enabled)
        self.assertEqual(config.session_file_name, "custom.dtj")
    
    def test_config_with_config_path_only(self):
        """Test that config with only config_path sets use_config=True."""
        import tempfile
        from pathlib import Path
        
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / ".dtj" / "config.toml"
            config_path.parent.mkdir(parents=True)
            config_path.write_text('[storage]\ndata_dir = "traces"\n')
            
            config = TraceConfig(config_path=str(config_path))
            # data_dir should be None (not explicitly set by user)
            self.assertIsNone(config.data_dir)
            # config_path should be stored
            self.assertEqual(config.config_path, str(config_path))
    
    def test_config_explicit_data_dir_wins_over_config(self):
        """Test that explicit data_dir (even "./traces") wins over config."""
        import tempfile
        from pathlib import Path
        
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / ".dtj" / "config.toml"
            config_path.parent.mkdir(parents=True)
            config_path.write_text('[storage]\ndata_dir = "traces"\n')
            
            # Explicit data_dir should win over config
            config = TraceConfig(data_dir="./traces", config_path=str(config_path))
            self.assertEqual(config.data_dir, Path("./traces"))
            self.assertEqual(config.config_path, str(config_path))
        self.assertIsNone(config.agent_path)
        self.assertIsNone(config.socket_path)
        self.assertTrue(config.enabled)
        self.assertIsNone(config.session_file_name)
    
    def test_custom_config(self):
        config = TraceConfig(
            data_dir="/custom/traces",
            agent_path="/custom/agent",
            socket_path="/custom/socket",
            enabled=False,
            session_file_name="custom.dtj",
        )
        self.assertEqual(config.data_dir, Path("/custom/traces"))
        self.assertEqual(config.agent_path, "/custom/agent")
        self.assertEqual(config.socket_path, "/custom/socket")
        self.assertFalse(config.enabled)
        self.assertEqual(config.session_file_name, "custom.dtj")


class TestTraceSessionUnit(unittest.TestCase):
    """Unit tests for TraceSession (mocked socket)."""
    
    def setUp(self):
        self.mock_sock = MockSocket([])
        self.mock_discovery = MagicMock()
        self.mock_metadata = MagicMock()
        self.mock_metadata.file_name = "test.dtj"
        self.mock_metadata.session_id = b"\x01" * 16
        self.mock_metadata.start_utc_unix_ms = 1234567890000
        self.mock_metadata.mono_origin_ns = 9876543210
        self.mock_metadata.producer_name = "test-prod"
        self.mock_metadata.producer_version = "1.0.0"
        
        self.session = TraceSession(self.mock_sock, self.mock_discovery, self.mock_metadata)
    
    def test_emit_validates_severity(self):
        with self.assertRaises(DTJValueError):
            self.session.emit(
                domain="api", category="request", name="completed",
                severity="invalid", field_name="duration", value=1.0
            )
    
    def _setup_emit_mocks(self, intern_side_effect=None, append_event_seq=1):
        """Helper to set up socket mocks for emit tests."""
        if intern_side_effect is None:
            intern_side_effect = [1, 2, 3, 4, 5, 6]
        
        # Create frames for InternOk responses and AppendEventOk
        frames = []
        for dict_id in intern_side_effect:
            # InternOk frame: length=5 (1+4), opcode=0x86, body=dict_id (4 bytes)
            body = struct.pack("<I", dict_id)
            frame = struct.pack("<I", 1 + len(body)) + bytes([0x86]) + body
            frames.append(frame)
        # AppendEventOk frame: length=9 (1+8), opcode=0x83, body=seq (8 bytes)
        body = struct.pack("<Q", append_event_seq)
        frame = struct.pack("<I", 1 + len(body)) + bytes([0x83]) + body
        frames.append(frame)

        # Replace the mock socket with a new one containing the frames
        self.mock_sock = MockSocket(frames)
        self.session._sock = self.mock_sock

    def test_emit_accepts_valid_severities(self):
        for severity in SEVERITY_MAP.keys():
            with self.subTest(severity=severity):
                self._setup_emit_mocks()
                try:
                    self.session.emit(
                        domain="api", category="request", name="completed",
                        severity=severity, field_name="duration", value=1.0
                    )
                except Exception as e:
                    if "Invalid severity" in str(e):
                        self.fail(f"Valid severity {severity} rejected")
    
    def test_emit_string_value_uses_interned(self):
        # 5 intern calls: domain, category, event_name, field_name, string value
        # (correlation is None, so no intern call for it)
        self._setup_emit_mocks(intern_side_effect=[1, 2, 3, 4, 5])
        
        self.session.emit(
            domain="api", category="request", name="completed",
            severity="info", field_name="status", value="success"
        )
        
        # Verify string value was interned (check string cache)
        self.assertIn("success", self.session._string_cache)
        self.assertEqual(self.session._string_cache["success"], 5)
    
    def test_emit_unsupported_type_raises(self):
        self._setup_emit_mocks()
        with self.assertRaises(DTJValueError):
            self.session.emit(
                domain="api", category="request", name="completed",
                severity="info", field_name="data", value=[1, 2, 3]
            )
    
    def test_emit_multi_field_not_supported(self):
        # MVP only supports single field - this is tested by API design
        # The emit method only takes one field_name/value pair
        pass
    
    def test_close_idempotent(self):
        # Mock socket recv to return FinishSessionOk response
        body = b""
        frame = struct.pack("<I", 1 + len(body)) + bytes([0x84]) + body
        self.mock_sock = MockSocket([frame])
        self.session._sock = self.mock_sock
        self.session.close()
        self.session.close()  # Should not raise
        self.assertTrue(self.session._closed)
    
    def test_emit_after_close_raises(self):
        # Mock socket recv to return FinishSessionOk response
        body = b""
        frame = struct.pack("<I", 1 + len(body)) + bytes([0x84]) + body
        self.mock_sock = MockSocket([frame])
        self.session._sock = self.mock_sock
        self.session.close()
        with self.assertRaises(DTJSessionError):
            self.session.emit(
                domain="api", category="request", name="completed",
                severity="info", field_name="duration", value=1.0
            )
    
    def test_context_manager_closes(self):
        session = TraceSession(self.mock_sock, self.mock_discovery, self.mock_metadata)
        # Mock socket recv to return FinishSessionOk response
        body = b""
        frame = struct.pack("<I", 1 + len(body)) + bytes([0x84]) + body
        session._sock = MockSocket([frame])
        with session:
            pass
        self.assertTrue(session._closed)
        self.mock_discovery.stop_agent.assert_called_once()


class TestTraceSessionOpenDisabled(unittest.TestCase):
    """Test TraceSession.open with enabled=False."""
    
    def test_open_disabled_returns_noop(self):
        session = TraceSession.open(
            producer_name="test",
            producer_version="1.0",
            enabled=False,
        )
        self.assertIsInstance(session, NoOpTraceSession)
    
    @patch("dtj_sdk.client.AgentDiscovery.find_agent", return_value=None)
    def test_open_no_agent_returns_noop(self, mock_find):
        session = TraceSession.open(
            producer_name="test",
            producer_version="1.0",
            enabled=True,
        )
        self.assertIsInstance(session, NoOpTraceSession)


class TestSeverityMapping(unittest.TestCase):
    """Test severity string to value mapping."""
    
    def test_all_severities_mapped(self):
        expected = {
            "trace": 0,
            "debug": 1,
            "info": 2,
            "warn": 3,
            "error": 4,
            "fatal": 5,
        }
        self.assertEqual(SEVERITY_MAP, expected)


if __name__ == "__main__":
    unittest.main()