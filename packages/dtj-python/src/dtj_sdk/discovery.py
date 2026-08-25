"""Agent discovery and process management."""

import os
import shutil
import subprocess
import tempfile
import warnings
from pathlib import Path
from typing import Optional

from .exceptions import DTJAgentNotFoundError, DTJConnectionError


class AgentDiscovery:
    """Handles discovery and lifecycle of dtj-agent binary."""
    
    def __init__(
        self,
        agent_path: Optional[str] = None,
        socket_path: Optional[str] = None,
        data_dir: Optional[str] = None,
        config_path: Optional[str] = None,
        use_config: bool = False,
    ):
        self.agent_path = agent_path
        self.socket_path = socket_path
        self.data_dir = data_dir
        self.config_path = config_path
        self.use_config = use_config
        self._process: Optional[subprocess.Popen] = None
        self._temp_socket_dir: Optional[tempfile.TemporaryDirectory] = None
        self._warning_emitted = False
    
    def find_agent(self) -> Optional[str]:
        """Find dtj-agent binary using discovery order."""
        # 1. Explicit agent_path
        if self.agent_path:
            path = Path(self.agent_path)
            if path.is_file() and os.access(path, os.X_OK):
                return str(path)
            raise DTJAgentNotFoundError(f"Specified agent_path not executable: {self.agent_path}")
        
        # 2. DTJ_AGENT_PATH environment variable
        env_path = os.environ.get("DTJ_AGENT_PATH")
        if env_path:
            path = Path(env_path)
            if path.is_file() and os.access(path, os.X_OK):
                return str(path)
        
        # 3. PATH lookup
        which_path = shutil.which("dtj-agent")
        if which_path:
            path = Path(which_path)
            if path.is_file() and os.access(path, os.X_OK):
                return str(path)
        
        # 4. macOS Homebrew fallback
        for candidate in ("/opt/homebrew/bin/dtj-agent", "/usr/local/bin/dtj-agent"):
            path = Path(candidate)
            if path.is_file() and os.access(path, os.X_OK):
                return str(path)
        
        # 5. Cargo dev install fallback
        cargo_path = Path.home() / ".cargo" / "bin" / "dtj-agent"
        if cargo_path.is_file() and os.access(cargo_path, os.X_OK):
            return str(cargo_path)
        
        return None
    
    def start_agent(self) -> str:
        """Start dtj-agent and return socket path."""
        # If socket_path provided, don't start agent (connect to existing)
        if self.socket_path:
            return self.socket_path
        
        agent_binary = self.find_agent()
        if not agent_binary:
            self._emit_warning()
            raise DTJAgentNotFoundError(
                "dtj-agent not found. Install dtj-agent or set DTJ_AGENT_PATH. "
                "Tracing disabled."
            )
        
        # Ensure data_dir exists - default to ./traces if not set
        data_dir = Path(self.data_dir) if self.data_dir else Path.cwd() / "traces"
        
        if self.use_config and self.config_path:
            # Use --config flag instead of --data-dir
            config_file = Path(self.config_path)
            if not config_file.is_file():
                # Try to read it anyway - start_agent will handle the error
                pass
            
            # Create socket dir for agent (even when using config)
            self._temp_socket_dir = tempfile.TemporaryDirectory(prefix="dtj-agent-")
            socket_dir = Path(self._temp_socket_dir.name)
            socket_path = socket_dir / "agent.sock"
            
            # Start agent with --config flag instead of --data-dir
            try:
                self._process = subprocess.Popen(
                    [agent_binary, "--socket", str(socket_path), "--config", str(config_file)],
                    stdin=subprocess.DEVNULL,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )
            except Exception as e:
                self._cleanup()
                raise DTJConnectionError(f"Failed to start dtj-agent with --config: {e}")
            
            return str(socket_path)
        
        # Create temporary directory for socket (default behavior)
        self._temp_socket_dir = tempfile.TemporaryDirectory(prefix="dtj-agent-")
        socket_dir = Path(self._temp_socket_dir.name)
        socket_path = socket_dir / "agent.sock"
        
        # Ensure data_dir exists (use --data-dir flag)
        data_dir.mkdir(parents=True, exist_ok=True)
        
        # Start agent process with --data-dir flag  
        try:
            self._process = subprocess.Popen(
                [agent_binary, "--socket", str(socket_path), "--data-dir", str(data_dir)],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        except Exception as e:
            self._cleanup()
            raise DTJConnectionError(f"Failed to start dtj-agent: {e}")
        
        return str(socket_path)
    
    def stop_agent(self) -> None:
        """Stop the agent process and cleanup."""
        if self._process:
            try:
                self._process.terminate()
                self._process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self._process.kill()
                self._process.wait()
            except Exception:
                pass
            finally:
                self._process = None
        
        self._cleanup()
    
    def _cleanup(self) -> None:
        """Cleanup temporary resources."""
        if self._temp_socket_dir:
            try:
                self._temp_socket_dir.cleanup()
            except Exception:
                pass
            self._temp_socket_dir = None
    
    def _emit_warning(self) -> None:
        """Emit single RuntimeWarning about disabled tracing."""
        if not self._warning_emitted:
            warnings.warn(
                "dtj-agent not found; tracing disabled (no-op mode). "
                "Install dtj-agent or set DTJ_AGENT_PATH to enable.",
                RuntimeWarning,
                stacklevel=3,
            )
            self._warning_emitted = True
    
    def __enter__(self) -> "AgentDiscovery":
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        self.stop_agent()