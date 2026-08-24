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
    def test_find_agent_path_lookup(self, mock_which):
        discovery = AgentDiscovery()
        found = discovery.find_agent()
        self.assertEqual(found, "/usr/bin/dtj-agent")
    
    @patch.dict(os.environ, {}, clear=True)
    @patch("shutil.which", return_value=None)
    def test_find_agent_not_found(self, mock_which):
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


if __name__ == "__main__":
    unittest.main()