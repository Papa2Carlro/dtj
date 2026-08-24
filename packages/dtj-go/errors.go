package dtj

import (
	"errors"
	"fmt"
)

// Error types for dtj SDK
var (
	ErrProtocol         = errors.New("dtj: protocol error")
	ErrConnection       = errors.New("dtj: connection error")
	ErrAgentNotFound    = errors.New("dtj: agent not found")
	ErrValue            = errors.New("dtj: value error")
	ErrSession          = errors.New("dtj: session error")
	ErrAgentUnavailable = errors.New("dtj: agent unavailable")
)

// ProtocolError represents a protocol-level error from the agent
type ProtocolError struct {
	Opcode byte
	Msg    string
}

func (e *ProtocolError) Error() string {
	return fmt.Sprintf("dtj: protocol error (opcode 0x%02x): %s", e.Opcode, e.Msg)
}

func (e *ProtocolError) Unwrap() error {
	return ErrProtocol
}

// ConnectionError represents a connection error
type ConnectionError struct {
	Msg string
}

func (e *ConnectionError) Error() string {
	return fmt.Sprintf("dtj: connection error: %s", e.Msg)
}

func (e *ConnectionError) Unwrap() error {
	return ErrConnection
}

// AgentNotFoundError represents an agent not found error
type AgentNotFoundError struct {
	Msg string
}

func (e *AgentNotFoundError) Error() string {
	return fmt.Sprintf("dtj: agent not found: %s", e.Msg)
}

func (e *AgentNotFoundError) Unwrap() error {
	return ErrAgentNotFound
}

// ValueError represents a value encoding error
type ValueError struct {
	Msg string
}

func (e *ValueError) Error() string {
	return fmt.Sprintf("dtj: value error: %s", e.Msg)
}

func (e *ValueError) Unwrap() error {
	return ErrValue
}

// SessionError represents a session error
type SessionError struct {
	Msg string
}

func (e *SessionError) Error() string {
	return fmt.Sprintf("dtj: session error: %s", e.Msg)
}

func (e *SessionError) Unwrap() error {
	return ErrSession
}

// AgentUnavailableError represents an agent unavailable warning
type AgentUnavailableError struct {
	Msg string
}

func (e *AgentUnavailableError) Error() string {
	return fmt.Sprintf("dtj: agent unavailable: %s", e.Msg)
}

func (e *AgentUnavailableError) Unwrap() error {
	return ErrAgentUnavailable
}
