package dtj

import (
	"errors"
	"os"
	"testing"
)

func TestOpenDisabled(t *testing.T) {
	cfg := Config{
		ProducerName:    "test",
		ProducerVersion: "1.0.0",
		Enabled:         boolPtr(false),
	}
	sess := Open(cfg)
	if sess == nil {
		t.Fatalf("Open returned nil")
	}
	if !sess.closed {
		t.Fatalf("session should be closed (disabled)")
	}
	// Emit should be no-op
	if err := sess.Emit(Event{}); err != nil {
		t.Fatalf("Emit on disabled session should not error: %v", err)
	}
	// Close should be idempotent
	if err := sess.Close(); err != nil {
		t.Fatalf("Close on disabled session failed: %v", err)
	}
	if err := sess.Close(); err != nil {
		t.Fatalf("Close on disabled session second call failed: %v", err)
	}
}

func TestOpenStrictDisabled(t *testing.T) {
	cfg := Config{
		ProducerName:    "test",
		ProducerVersion: "1.0.0",
		Enabled:         boolPtr(false),
	}
	sess, err := OpenStrict(cfg)
	if err != nil {
		t.Fatalf("OpenStrict returned error: %v", err)
	}
	if sess == nil {
		t.Fatalf("OpenStrict returned nil")
	}
	if !sess.closed {
		t.Fatalf("session should be closed (disabled)")
	}
}

func TestOpenMissingAgent(t *testing.T) {
	os.Setenv("PATH", "/empty/path")
	os.Setenv("DTJ_AGENT_PATH", "")
	defer func() {
		os.Unsetenv("PATH")
		os.Unsetenv("DTJ_AGENT_PATH")
	}()

	cfg := Config{
		ProducerName:    "test",
		ProducerVersion: "1.0.0",
		DataDir:         "/tmp/test",
	}
	sess := Open(cfg)
	if sess == nil {
		t.Fatalf("Open returned nil")
	}
	if !sess.closed {
		t.Fatalf("session should be closed (no agent)")
	}
}

func TestOpenStrictMissingAgent(t *testing.T) {
	os.Setenv("PATH", "/empty/path")
	os.Setenv("DTJ_AGENT_PATH", "")
	defer func() {
		os.Unsetenv("PATH")
		os.Unsetenv("DTJ_AGENT_PATH")
	}()

	cfg := Config{
		ProducerName:    "test",
		ProducerVersion: "1.0.0",
		DataDir:         "/tmp/test",
	}
	sess, err := OpenStrict(cfg)
	if err != nil {
		t.Fatalf("OpenStrict returned error: %v", err)
	}
	if sess == nil {
		t.Fatalf("OpenStrict returned nil")
	}
	if !sess.closed {
		t.Fatalf("session should be closed (no agent)")
	}
}

func TestWarningHandler(t *testing.T) {
	var warned bool
	var warnedErr error

	cfg := Config{
		ProducerName:    "test",
		ProducerVersion: "1.0.0",
		DataDir:         "/tmp/test",
		WarningHandler: func(err error) {
			warned = true
			warnedErr = err
		},
	}

	os.Setenv("PATH", "/empty/path")
	os.Setenv("DTJ_AGENT_PATH", "")
	defer func() {
		os.Unsetenv("PATH")
		os.Unsetenv("DTJ_AGENT_PATH")
	}()

	sess := Open(cfg)
	if sess == nil {
		t.Fatalf("Open returned nil")
	}

	// Trigger warning by calling Emit
	sess.Emit(Event{
		Domain:    "test",
		Category:  "cat",
		Name:      "event",
		Severity:  SeverityInfo,
		FieldName: "field",
		Value:     1,
	})

	if !warned {
		t.Fatalf("warning handler not called")
	}
	var agentUnavail *AgentUnavailableError
	if !errors.As(warnedErr, &agentUnavail) {
		t.Fatalf("warning should be AgentUnavailableError, got: %T", warnedErr)
	}
}

func TestWarningHandlerOnce(t *testing.T) {
	warnCount := 0

	cfg := Config{
		ProducerName:    "test",
		ProducerVersion: "1.0.0",
		DataDir:         "/tmp/test",
		WarningHandler: func(err error) {
			warnCount++
		},
	}

	os.Setenv("PATH", "/empty/path")
	os.Setenv("DTJ_AGENT_PATH", "")
	defer func() {
		os.Unsetenv("PATH")
		os.Unsetenv("DTJ_AGENT_PATH")
	}()

	sess := Open(cfg)

	// Call Emit multiple times
	for i := 0; i < 5; i++ {
		sess.Emit(Event{
			Domain:    "test",
			Category:  "cat",
			Name:      "event",
			Severity:  SeverityInfo,
			FieldName: "field",
			Value:     i,
		})
	}

	if warnCount != 1 {
		t.Fatalf("warning handler called %d times, expected 1", warnCount)
	}
}

func TestEmitUnsupportedValue(t *testing.T) {
	cfg := Config{
		ProducerName:    "test",
		ProducerVersion: "1.0.0",
		Enabled:         boolPtr(false),
	}
	sess := Open(cfg)

	// On disabled session, Emit should be no-op and return nil
	err := sess.Emit(Event{
		Domain:    "test",
		Category:  "cat",
		Name:      "event",
		Severity:  SeverityInfo,
		FieldName: "field",
		Value:     struct{}{}, // unsupported type
	})

	if err != nil {
		t.Fatalf("Emit on disabled session should not error: %v", err)
	}
}

func TestCloseIdempotent(t *testing.T) {
	cfg := Config{
		ProducerName:    "test",
		ProducerVersion: "1.0.0",
		Enabled:         boolPtr(false),
	}
	sess := Open(cfg)

	if err := sess.Close(); err != nil {
		t.Fatalf("Close failed: %v", err)
	}
	if err := sess.Close(); err != nil {
		t.Fatalf("Close second call failed: %v", err)
	}
}

func TestConfigDefaults(t *testing.T) {
	cfg := Config{
		ProducerName:    "test",
		ProducerVersion: "1.0.0",
	}

	// Enabled should default to true (nil pointer means true)
	if cfg.Enabled != nil {
		t.Fatalf("Enabled should be nil by default")
	}
}

func boolPtr(b bool) *bool {
	return &b
}
