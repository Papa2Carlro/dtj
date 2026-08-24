package dtj

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestFindAgentExplicitPath(t *testing.T) {
	// Create a fake executable
	tmpDir := t.TempDir()
	fakeAgent := filepath.Join(tmpDir, "dtj-agent")
	if err := os.WriteFile(fakeAgent, []byte("#!/bin/sh\necho 'fake agent'"), 0755); err != nil {
		t.Fatalf("failed to create fake agent: %v", err)
	}

	d := &AgentDiscovery{AgentPath: fakeAgent}
	path, err := d.FindAgent()
	if err != nil {
		t.Fatalf("FindAgent failed: %v", err)
	}
	if path != fakeAgent {
		t.Fatalf("path mismatch: %s != %s", path, fakeAgent)
	}
}

func TestFindAgentExplicitPathNotFound(t *testing.T) {
	d := &AgentDiscovery{AgentPath: "/nonexistent/dtj-agent"}
	_, err := d.FindAgent()
	if err == nil {
		t.Fatalf("expected error for nonexistent agent path")
	}
}

func TestFindAgentEnvVar(t *testing.T) {
	tmpDir := t.TempDir()
	fakeAgent := filepath.Join(tmpDir, "dtj-agent")
	if err := os.WriteFile(fakeAgent, []byte("#!/bin/sh\necho 'fake agent'"), 0755); err != nil {
		t.Fatalf("failed to create fake agent: %v", err)
	}

	os.Setenv("DTJ_AGENT_PATH", fakeAgent)
	defer os.Unsetenv("DTJ_AGENT_PATH")

	d := &AgentDiscovery{}
	path, err := d.FindAgent()
	if err != nil {
		t.Fatalf("FindAgent failed: %v", err)
	}
	if path != fakeAgent {
		t.Fatalf("path mismatch: %s != %s", path, fakeAgent)
	}
}

func TestFindAgentPathLookup(t *testing.T) {
	tmpDir := t.TempDir()
	fakeAgent := filepath.Join(tmpDir, "dtj-agent")
	if err := os.WriteFile(fakeAgent, []byte("#!/bin/sh\necho 'fake agent'"), 0755); err != nil {
		t.Fatalf("failed to create fake agent: %v", err)
	}

	oldPath := os.Getenv("PATH")
	os.Setenv("PATH", tmpDir+string(os.PathListSeparator)+oldPath)
	defer os.Setenv("PATH", oldPath)

	d := &AgentDiscovery{}
	path, err := d.FindAgent()
	if err != nil {
		t.Fatalf("FindAgent failed: %v", err)
	}
	if path != fakeAgent {
		t.Fatalf("path mismatch: %s != %s", path, fakeAgent)
	}
}

func TestFindAgentNotFound(t *testing.T) {
	os.Setenv("PATH", "/empty/path")
	os.Setenv("DTJ_AGENT_PATH", "")
	defer func() {
		os.Unsetenv("PATH")
		os.Unsetenv("DTJ_AGENT_PATH")
	}()

	d := &AgentDiscovery{}
	path, err := d.FindAgent()
	if err != nil {
		t.Fatalf("FindAgent failed: %v", err)
	}
	if path != "" {
		t.Fatalf("expected empty path, got: %s", path)
	}
}

func TestStartAgentWithSocketPath(t *testing.T) {
	d := &AgentDiscovery{SocketPath: "/tmp/existing.sock"}
	path, err := d.StartAgent()
	if err != nil {
		t.Fatalf("StartAgent failed: %v", err)
	}
	if path != "/tmp/existing.sock" {
		t.Fatalf("path mismatch: %s", path)
	}
}

func TestStartAgentNotFound(t *testing.T) {
	os.Setenv("PATH", "/empty/path")
	os.Setenv("DTJ_AGENT_PATH", "")
	defer func() {
		os.Unsetenv("PATH")
		os.Unsetenv("DTJ_AGENT_PATH")
	}()

	d := &AgentDiscovery{DataDir: "/tmp/test"}
	_, err := d.StartAgent()
	if err == nil {
		t.Fatalf("expected error for missing agent")
	}
}

func TestStopAgentIdempotent(t *testing.T) {
	tmpDir := t.TempDir()
	fakeAgent := filepath.Join(tmpDir, "dtj-agent")
	if err := os.WriteFile(fakeAgent, []byte("#!/bin/sh\nsleep 10"), 0755); err != nil {
		t.Fatalf("failed to create fake agent: %v", err)
	}

	d := &AgentDiscovery{AgentPath: fakeAgent, DataDir: tmpDir}
	socketPath, err := d.StartAgent()
	if err != nil {
		t.Fatalf("StartAgent failed: %v", err)
	}

	// Stop twice - should be idempotent
	if err := d.StopAgent(); err != nil {
		t.Fatalf("StopAgent failed: %v", err)
	}
	if err := d.StopAgent(); err != nil {
		t.Fatalf("StopAgent second call failed: %v", err)
	}

	// Temp dir should be cleaned up
	if _, err := os.Stat(filepath.Dir(socketPath)); !os.IsNotExist(err) {
		t.Fatalf("temp dir not cleaned up")
	}
}

func TestWaitForSocketTimeout(t *testing.T) {
	err := WaitForSocket("/tmp/nonexistent.sock", 100*time.Millisecond)
	if err == nil {
		t.Fatalf("expected timeout error")
	}
}
