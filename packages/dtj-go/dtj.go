package dtj

// This file provides the main package documentation and re-exports key types.
// The dtj package provides a Go SDK for the DTJ (Debug Trace Journal) tracing system.
//
// Architecture:
//   Go Application → dtj SDK → local dtj-agent → Rust SessionWriter → .dtj files
//
// The SDK is a thin middleware that communicates with a local dtj-agent binary
// via Unix domain socket using a versioned binary protocol. The SDK never writes
// .dtj bytes directly - all serialization is handled by the agent.
//
// Basic usage:
//
//	trace := dtj.Open(dtj.Config{
//	    ProducerName:    "my-go-service",
//	    ProducerVersion: "0.1.0",
//	    DataDir:         "./traces",
//	})
//	defer trace.Close()
//
//	trace.Emit(dtj.Event{
//	    Domain:      "api",
//	    Category:    "request",
//	    Name:        "completed",
//	    Severity:    dtj.Info,
//	    FieldName:   "duration_ms",
//	    Value:       12.5,
//	    Correlation: "request-42",
//	})
//
// If dtj-agent is not available, the SDK enters a disabled/no-op mode and emits
// a single warning via the configured WarningHandler. The application continues
// running normally without creating any .dtj files.
