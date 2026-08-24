package dtj

import (
	"os"
	"path/filepath"
	"testing"
)

// TestE2E is an opt-in end-to-end test that requires a real dtj-agent binary
// and Unix socket support. It is skipped unless DTJ_RUN_AGENT_E2E=1 is set.
func TestE2E(t *testing.T) {
	if os.Getenv("DTJ_RUN_AGENT_E2E") != "1" {
		t.Skip("E2E test skipped. Set DTJ_RUN_AGENT_E2E=1 to run.")
	}

	// Create a temporary directory for traces
	tmpDir := t.TempDir()

	trace := Open(Config{
		ProducerName:    "e2e-test",
		ProducerVersion: "0.1.0",
		DataDir:         tmpDir,
	})
	if trace == nil {
		t.Fatal("Open returned nil")
	}
	defer trace.Close()

	// Emit various event types
	tests := []Event{
		{
			Domain:      "api",
			Category:    "request",
			Name:        "completed",
			Severity:    SeverityInfo,
			FieldName:   "duration_ms",
			Value:       12.5,
			Correlation: "request-42",
		},
		{
			Domain:    "test",
			Category:  "bool",
			Name:      "flag",
			Severity:  SeverityInfo,
			FieldName: "enabled",
			Value:     true,
		},
		{
			Domain:    "test",
			Category:  "int",
			Name:      "count",
			Severity:  SeverityInfo,
			FieldName: "value",
			Value:     42,
		},
		{
			Domain:    "test",
			Category:  "bigint",
			Name:      "large",
			Severity:  SeverityInfo,
			FieldName: "value",
			Value:     int64(123456789012345),
		},
		{
			Domain:    "test",
			Category:  "float",
			Name:      "ratio",
			Severity:  SeverityInfo,
			FieldName: "value",
			Value:     3.14159,
		},
		{
			Domain:    "test",
			Category:  "bytes",
			Name:      "payload",
			Severity:  SeverityInfo,
			FieldName: "data",
			Value:     []byte{0xDE, 0xAD, 0xBE, 0xEF},
		},
	}

	for _, event := range tests {
		if err := trace.Emit(event); err != nil {
			t.Fatalf("Emit failed: %v", err)
		}
	}

	// Close the session
	if err := trace.Close(); err != nil {
		t.Fatalf("Close failed: %v", err)
	}

	// Verify .dtj file was created
	files, err := filepath.Glob(filepath.Join(tmpDir, "*.dtj"))
	if err != nil {
		t.Fatalf("failed to glob dtj files: %v", err)
	}
	if len(files) == 0 {
		t.Fatal("no .dtj file created")
	}

	// Verify file has content and correct magic bytes
	for _, file := range files {
		content, err := os.ReadFile(file)
		if err != nil {
			t.Fatalf("failed to read file: %v", err)
		}
		if len(content) == 0 {
			t.Fatal("session file is empty")
		}
		// DTJ v1 magic: "DTJ\1" (0x44 0x54 0x4A 0x01)
		if len(content) < 4 || content[0] != 0x44 || content[1] != 0x54 || content[2] != 0x4A || content[3] != 0x01 {
			t.Fatalf("invalid DTJ magic bytes: %v", content[:4])
		}
	}
}

// TestE2ESeverityLevels tests all severity levels
func TestE2ESeverityLevels(t *testing.T) {
	if os.Getenv("DTJ_RUN_AGENT_E2E") != "1" {
		t.Skip("E2E test skipped. Set DTJ_RUN_AGENT_E2E=1 to run.")
	}

	tmpDir := t.TempDir()

	trace := Open(Config{
		ProducerName:    "e2e-test",
		ProducerVersion: "0.1.0",
		DataDir:         tmpDir,
	})
	if trace == nil {
		t.Fatal("Open returned nil")
	}
	defer trace.Close()

	severities := []Severity{SeverityDebug, SeverityInfo, SeverityWarn, SeverityError, SeverityFatal}
	for _, sev := range severities {
		if err := trace.Emit(Event{
			Domain:    "test",
			Category:  "severity",
			Name:      sev.String(),
			Severity:  sev,
			FieldName: "level",
			Value:     sev.String(),
		}); err != nil {
			t.Fatalf("Emit failed for %s: %v", sev, err)
		}
	}

	if err := trace.Close(); err != nil {
		t.Fatalf("Close failed: %v", err)
	}

	files, err := filepath.Glob(filepath.Join(tmpDir, "*.dtj"))
	if err != nil {
		t.Fatalf("failed to glob dtj files: %v", err)
	}
	if len(files) == 0 {
		t.Fatal("no .dtj file created")
	}
}

// TestE2EIdempotentClose tests that Close is idempotent
func TestE2EIdempotentClose(t *testing.T) {
	if os.Getenv("DTJ_RUN_AGENT_E2E") != "1" {
		t.Skip("E2E test skipped. Set DTJ_RUN_AGENT_E2E=1 to run.")
	}

	tmpDir := t.TempDir()

	trace := Open(Config{
		ProducerName:    "e2e-test",
		ProducerVersion: "0.1.0",
		DataDir:         tmpDir,
	})
	if trace == nil {
		t.Fatal("Open returned nil")
	}

	if err := trace.Emit(Event{
		Domain:    "test",
		Category:  "cat",
		Name:      "event",
		Severity:  SeverityInfo,
		FieldName: "field",
		Value:     1,
	}); err != nil {
		t.Fatalf("Emit failed: %v", err)
	}

	// Close twice
	if err := trace.Close(); err != nil {
		t.Fatalf("Close failed: %v", err)
	}
	if err := trace.Close(); err != nil {
		t.Fatalf("Close second call failed: %v", err)
	}
}
