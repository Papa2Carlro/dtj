"""Unit tests for agent discovery."""

import os
import tempfile
import unittest
from unittest.mock import patch, MagicMock
from pathlib import Path

from dtj_sdk.discovery import AgentDiscovery
from dtj_sdk.exceptions import DTJAgentNotFoundError, DTJConnectionError


class TestAgentDiscovery(unittest.TestCase):
    """Test agent discovery logic."""
    
    def test_find_agent_explicit_path(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            agent_path = Path(tmpdir) / "dtj-agent"
            agent_path.write_text("#!/bin/sh\necho hello")
            agent_path.chmod(0o755)
            
            discovery = AgentDiscovery(agent_path=str(agent_path))
            found = discovery.find_agent()
            self.assertEqual(found, str(agent_path))
    
    def test_find_agent_explicit_path_not_executable(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            agent_path = Path(tmpdir) / "dtj-agent"
            agent_path.write_text("not executable")
            agent_path.chmod(0o644)
            
            discovery = AgentDiscovery(agent_path=str(agent_path))
            with self.assertRaises(DTJAgentNotFoundError):
                discovery.find_agent()
    
    @patch.dict(os.environ, {"DTJ_AGENT_PATH": "/fake/path"})
    @patch("shutil.which", return_value=None)
    @patch("pathlib.Path.is_file", return_value=True)
    @patch("os.access", return_value=True)
    def test_find_agent_env_var(self, mock_access, mock_is_file, mock_which):
        discovery = AgentDiscovery()
        found = discovery.find_agent()
        self.assertEqual(found, "/fake/path")
    
    @patch.dict(os.environ, {}, clear=True)
    @patch("shutil.which", return_value="/usr/bin/dtj-agent")
    @patch("pathlib.Path.is_file", return_value=True)
    @patch("os.access", return_value=True)
    def test_find_agent_path_lookup(self, mock_access, mock_is_file, mock_which):
        discovery = AgentDiscovery()
        found = discovery.find_agent()
        self.assertEqual(found, "/usr/bin/dtj-agent")
    
    @patch.dict(os.environ, {}, clear=True)
    @patch("shutil.which", return_value=None)
    @patch("os.access", return_value=True)
    def test_find_agent_homebrew_arm(self, mock_access, mock_which):
        with patch("pathlib.Path.is_file", return_value=True):
            discovery = AgentDiscovery()
            found = discovery.find_agent()
            # first fallback is /opt/homebrew/bin/dtj-agent
            self.assertEqual(found, "/opt/homebrew/bin/dtj-agent")
    
    @patch.dict(os.environ, {}, clear=True)
    @patch("shutil.which", return_value=None)
    @patch("os.access", return_value=True)
    def test_find_agent_homebrew_intel(self, mock_access, mock_which):
        with patch("pathlib.Path.is_file", side_effect=[False, True]):
            discovery = AgentDiscovery()
            found = discovery.find_agent()
            # first candidate fails, second succeeds
            self.assertEqual(found, "/usr/local/bin/dtj-agent")
    
    @patch.dict(os.environ, {}, clear=True)
    @patch("shutil.which", return_value=None)
    @patch("os.access", return_value=True)
    def test_find_agent_cargo_fallback(self, mock_access, mock_which):
        with patch("pathlib.Path.is_file", side_effect=[False, False, True]):
            with patch("pathlib.Path.home", return_value=Path("/home/user")):
                discovery = AgentDiscovery()
                found = discovery.find_agent()
                self.assertEqual(found, "/home/user/.cargo/bin/dtj-agent")

    @patch.dict(os.environ, {}, clear=True)
    @patch("shutil.which", return_value=None)
    @patch("pathlib.Path.is_file", return_value=False)
    @patch("os.access", return_value=False)
    def test_find_agent_not_found(self, mock_access, mock_is_file, mock_which):
        discovery = AgentDiscovery()
        found = discovery.find_agent()
        self.assertIsNone(found)
    
    @patch("dtj_sdk.discovery.AgentDiscovery.find_agent", return_value=None)
    def test_start_agent_not_found_emits_warning(self, mock_find):
        discovery = AgentDiscovery(data_dir="/tmp/traces")
        with self.assertRaises(DTJAgentNotFoundError):
            discovery.start_agent()
    
    @patch("dtj_sdk.discovery.AgentDiscovery.find_agent", return_value="/fake/agent")
    @patch("subprocess.Popen")
    def test_start_agent_creates_socket(self, mock_popen, mock_find):
        mock_process = MagicMock()
        mock_popen.return_value = mock_process
        
        discovery = AgentDiscovery(data_dir="/tmp/traces")
        socket_path = discovery.start_agent()
        
        self.assertTrue(socket_path.endswith("agent.sock"))
        mock_popen.assert_called_once()
        args = mock_popen.call_args[0][0]
        self.assertEqual(args[0], "/fake/agent")
        self.assertIn("--socket", args)
        self.assertIn("--data-dir", args)
    
    @patch("dtj_sdk.discovery.AgentDiscovery.find_agent", return_value="/fake/agent")
    def test_start_agent_with_existing_socket(self, mock_find):
        discovery = AgentDiscovery(socket_path="/existing/socket", data_dir="/tmp/traces")
        socket_path = discovery.start_agent()
        self.assertEqual(socket_path, "/existing/socket")
    
    def test_stop_agent_cleans_up(self):
        discovery = AgentDiscovery()
        discovery._process = MagicMock()
        discovery._temp_socket_dir = MagicMock()
        
        # Capture mock methods before stop_agent (which sets _process to None in finally)
        terminate_method = discovery._process.terminate
        wait_method = discovery._process.wait
        cleanup_method = discovery._temp_socket_dir.cleanup
        
        discovery.stop_agent()
        
        # Verify the mocks were called
        terminate_method.assert_called_once()
        wait_method.assert_called_once()
        cleanup_method.assert_called_once()

    def test_context_manager(self):
        with patch.object(AgentDiscovery, "find_agent", return_value="/fake/agent"):
            with patch("subprocess.Popen") as mock_popen:
                mock_process = MagicMock()
                mock_popen.return_value = mock_process
                
                with AgentDiscovery(data_dir="/tmp/traces") as discovery:
                    socket_path = discovery.start_agent()
                
                mock_process.terminate.assert_called_once()

    def test_start_agent_socket_path_no_find_agent_no_spawn(self):
        """Test that socket_path skips find_agent and subprocess spawn."""
        with patch("dtj_sdk.discovery.AgentDiscovery.find_agent") as mock_find:
            with patch("subprocess.Popen") as mock_popen:
                discovery = AgentDiscovery(socket_path="/existing/socket")
                result = discovery.start_agent()
                
                # Should return the provided socket_path
                self.assertEqual(result, "/existing/socket")
                # find_agent should NOT be called
                mock_find.assert_not_called()
                # subprocess.Popen should NOT be called (no spawn)
                mock_popen.assert_not_called()

    def test_start_agent_with_config_uses_config_flag(self):
        """Test that use_config=True launches agent with --config flag."""
        import tempfile
        from pathlib import Path
        
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / "config.toml"
            config_path.write_text('[storage]\ndata_dir = "traces"\n')
            
            with patch("dtj_sdk.discovery.AgentDiscovery.find_agent", return_value="/fake/agent"):
                with patch("subprocess.Popen") as mock_popen:
                    mock_process = MagicMock()
                    mock_popen.return_value = mock_process
                    
                    discovery = AgentDiscovery(
                        data_dir="/tmp/traces",
                        config_path=str(config_path),
                        use_config=True,
                    )
                    socket_path = discovery.start_agent()
                    
                    # Verify Popen was called with --config not --data-dir
                    mock_popen.assert_called_once()
                    args = mock_popen.call_args[0][0]
                    self.assertEqual(args[0], "/fake/agent")
                    self.assertIn("--socket", args)
                    self.assertIn("--config", args)
                    self.assertEqual(args[args.index("--config") + 1], str(config_path))
                    self.assertNotIn("--data-dir", args)
                    
                    # Should return the created socket path (temp file)
                    self.assertTrue(socket_path.endswith("agent.sock"))

    def test_start_agent_without_config_uses_data_dir(self):
        """Test that use_config=False launches agent with --data-dir."""
        import tempfile
        from unittest.mock import patch, MagicMock
        from pathlib import Path
        
        with tempfile.TemporaryDirectory() as tmpdir:
            data_dir = Path(tmpdir) / "traces"
            
            with patch("dtj_sdk.discovery.AgentDiscovery.find_agent", return_value="/fake/agent"):
                with patch("subprocess.Popen") as mock_popen:
                    mock_process = MagicMock()
                    mock_popen.return_value = mock_process
                    
                    discovery = AgentDiscovery(
                        data_dir=str(data_dir),
                        use_config=False,
                    )
                    socket_path = discovery.start_agent()
                    
                    # Verify Popen was called with --data-dir not --config
                    args = mock_popen.call_args[0][0]
                    self.assertIn("--data-dir", args)
                    self.assertEqual(args[args.index("--data-dir") + 1], str(data_dir))
                    self.assertNotIn("--config", args)


if __name__ == "__main__":
    unittest.main()