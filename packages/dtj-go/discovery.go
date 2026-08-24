package dtj

import (
	"encoding/binary"
	"fmt"
	"io"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"time"
)

// AgentDiscovery handles discovery and lifecycle of dtj-agent binary
type AgentDiscovery struct {
	AgentPath  string
	SocketPath string
	DataDir    string

	process     *exec.Cmd
	tempDir     string
	warningOnce bool
}

// FindAgent finds dtj-agent binary using discovery order
func (d *AgentDiscovery) FindAgent() (string, error) {
	// 1. Explicit AgentPath
	if d.AgentPath != "" {
		info, err := os.Stat(d.AgentPath)
		if err != nil {
			return "", &AgentNotFoundError{Msg: fmt.Sprintf("specified agent_path not found: %s", d.AgentPath)}
		}
		if info.Mode()&0111 == 0 {
			return "", &AgentNotFoundError{Msg: fmt.Sprintf("specified agent_path not executable: %s", d.AgentPath)}
		}
		return d.AgentPath, nil
	}

	// 2. DTJ_AGENT_PATH environment variable
	if envPath := os.Getenv("DTJ_AGENT_PATH"); envPath != "" {
		info, err := os.Stat(envPath)
		if err == nil && info.Mode()&0111 != 0 {
			return envPath, nil
		}
	}

	// 3. PATH lookup
	if path, err := exec.LookPath("dtj-agent"); err == nil {
		return path, nil
	}

	return "", nil // Not found
}

// StartAgent starts dtj-agent and returns socket path
func (d *AgentDiscovery) StartAgent() (string, error) {
	// If SocketPath provided, don't start agent (connect to existing)
	if d.SocketPath != "" {
		return d.SocketPath, nil
	}

	agentBinary, err := d.FindAgent()
	if err != nil {
		return "", err
	}
	if agentBinary == "" {
		d.emitWarning()
		return "", &AgentNotFoundError{Msg: "dtj-agent not found. Install dtj-agent or set DTJ_AGENT_PATH. Tracing disabled."}
	}

	// Create temporary directory for socket
	tempDir, err := os.MkdirTemp("", "dtj-agent-")
	if err != nil {
		return "", &ConnectionError{Msg: fmt.Sprintf("failed to create temp dir: %v", err)}
	}
	d.tempDir = tempDir
	socketPath := filepath.Join(tempDir, "agent.sock")

	// Ensure data_dir exists
	dataDir := d.DataDir
	if dataDir == "" {
		cwd, _ := os.Getwd()
		dataDir = filepath.Join(cwd, "traces")
	}
	if err := os.MkdirAll(dataDir, 0755); err != nil {
		d.cleanup()
		return "", &ConnectionError{Msg: fmt.Sprintf("failed to create data dir: %v", err)}
	}

	// Start agent process
	d.process = exec.Command(agentBinary, "--socket", socketPath, "--data-dir", dataDir)
	d.process.Stdin = nil
	d.process.Stdout = nil
	d.process.Stderr = nil

	if err := d.process.Start(); err != nil {
		d.cleanup()
		return "", &ConnectionError{Msg: fmt.Sprintf("failed to start dtj-agent: %v", err)}
	}

	return socketPath, nil
}

// StopAgent stops the agent process and cleans up
func (d *AgentDiscovery) StopAgent() error {
	if d.process != nil && d.process.Process != nil {
		// Try graceful termination
		d.process.Process.Signal(os.Interrupt)

		// Wait with timeout
		done := make(chan error, 1)
		go func() {
			done <- d.process.Wait()
		}()

		select {
		case <-done:
			// Process exited
		case <-time.After(5 * time.Second):
			// Force kill
			d.process.Process.Kill()
			<-done
		}
		d.process = nil
	}

	d.cleanup()
	return nil
}

// cleanup removes temporary resources
func (d *AgentDiscovery) cleanup() {
	if d.tempDir != "" {
		os.RemoveAll(d.tempDir)
		d.tempDir = ""
	}
}

// emitWarning emits a single warning about disabled tracing
func (d *AgentDiscovery) emitWarning() {
	if !d.warningOnce {
		d.warningOnce = true
		// Warning will be handled by the caller via WarningHandler
	}
}

// WaitForSocket waits for the socket to become available
func WaitForSocket(socketPath string, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		conn, err := net.Dial("unix", socketPath)
		if err == nil {
			conn.Close()
			return nil
		}
		time.Sleep(10 * time.Millisecond)
	}
	return &ConnectionError{Msg: fmt.Sprintf("timeout waiting for socket: %s", socketPath)}
}

// ConnectWithRetry connects to a Unix socket with retry
func ConnectWithRetry(socketPath string, timeout time.Duration) (net.Conn, error) {
	deadline := time.Now().Add(timeout)
	var lastErr error

	for time.Now().Before(deadline) {
		conn, err := net.Dial("unix", socketPath)
		if err == nil {
			return conn, nil
		}
		lastErr = err
		time.Sleep(10 * time.Millisecond)
	}

	return nil, &ConnectionError{Msg: fmt.Sprintf("failed to connect to agent at %s: %v", socketPath, lastErr)}
}

// ReadFrame reads a complete frame from a connection
func ReadFrame(conn net.Conn) (*Frame, error) {
	// Read length (4 bytes)
	lenBuf := make([]byte, 4)
	if _, err := io.ReadFull(conn, lenBuf); err != nil {
		return nil, &ConnectionError{Msg: fmt.Sprintf("failed to read frame length: %v", err)}
	}
	length := binary.LittleEndian.Uint32(lenBuf)
	if length > MaxFrameSize {
		return nil, &ProtocolError{Msg: fmt.Sprintf("frame too large: %d", length)}
	}

	// Read payload (length bytes including opcode)
	payload := make([]byte, length)
	if _, err := io.ReadFull(conn, payload); err != nil {
		return nil, &ConnectionError{Msg: fmt.Sprintf("failed to read frame payload: %v", err)}
	}

	return &Frame{
		Opcode: payload[0],
		Body:   payload[1:],
	}, nil
}

// WriteFrame writes a frame to a connection
func WriteFrame(conn net.Conn, frame []byte) error {
	_, err := conn.Write(frame)
	return err
}
