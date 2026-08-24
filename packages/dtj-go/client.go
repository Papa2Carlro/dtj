package dtj

import (
	"encoding/binary"
	"fmt"
	"net"
	"os"
	"sync"
	"time"
)

// WarningHandler is a function type for handling warnings
type WarningHandler func(error)

// Config holds configuration for opening a trace session
type Config struct {
	DataDir         string
	ProducerName    string
	ProducerVersion string
	AgentPath       string
	SocketPath      string
	SessionFileName string
	Enabled         *bool
	WarningHandler  WarningHandler
}

// Event represents a trace event to emit
type Event struct {
	Domain      string
	Category    string
	Name        string
	Severity    Severity
	FieldName   string
	Value       any
	Correlation string
}

// Session represents a trace session
type Session struct {
	conn      net.Conn
	discovery *AgentDiscovery
	metadata  *OpenSessionMetadata
	closed    bool
	mu        sync.Mutex
	startTime time.Time

	// Dictionary caches
	domainCache    map[string]uint32
	categoryCache  map[string]uint32
	eventNameCache map[string]uint32
	stringCache    map[string]uint32

	warningHandler WarningHandler
	warningOnce    bool
}

// Open opens a new trace session (returns disabled session if agent unavailable)
func Open(cfg Config) *Session {
	enabled := true
	if cfg.Enabled != nil {
		enabled = *cfg.Enabled
	}

	if !enabled {
		return newNoOpSession(cfg.WarningHandler)
	}

	sess, _ := OpenStrict(cfg)
	return sess
}

// OpenStrict opens a new trace session (returns error if agent unavailable)
func OpenStrict(cfg Config) (*Session, error) {
	enabled := true
	if cfg.Enabled != nil {
		enabled = *cfg.Enabled
	}

	if !enabled {
		return newNoOpSession(cfg.WarningHandler), nil
	}

	// Default warning handler
	warningHandler := cfg.WarningHandler
	if warningHandler == nil {
		warningHandler = func(err error) {
			// Default: print to stderr
			fmt.Fprintf(os.Stderr, "dtj warning: %v\n", err)
		}
	}

	discovery := &AgentDiscovery{
		AgentPath:  cfg.AgentPath,
		SocketPath: cfg.SocketPath,
		DataDir:    cfg.DataDir,
	}

	// Check if agent exists
	agentBinary, err := discovery.FindAgent()
	if err != nil {
		return nil, err
	}
	if agentBinary == "" {
		warningHandler(&AgentUnavailableError{Msg: "dtj-agent not found. Install dtj-agent or set DTJ_AGENT_PATH. Tracing disabled."})
		return newNoOpSession(warningHandler), nil
	}

	// Start or connect to agent
	socketPath, err := discovery.StartAgent()
	if err != nil {
		return nil, err
	}

	// Connect to socket with retry
	conn, err := ConnectWithRetry(socketPath, 5*time.Second)
	if err != nil {
		discovery.StopAgent()
		return nil, err
	}

	// Hello handshake
	helloFrame, err := EncodeHello()
	if err != nil {
		conn.Close()
		discovery.StopAgent()
		return nil, err
	}
	if err := WriteFrame(conn, helloFrame); err != nil {
		conn.Close()
		discovery.StopAgent()
		return nil, err
	}

	frame, err := ReadFrame(conn)
	if err != nil {
		conn.Close()
		discovery.StopAgent()
		return nil, err
	}
	if frame.Opcode == OpError {
		conn.Close()
		discovery.StopAgent()
		return nil, &ProtocolError{Opcode: OpError, Msg: DecodeError(frame.Body)}
	}
	if frame.Opcode != OpHelloOk {
		conn.Close()
		discovery.StopAgent()
		return nil, &ProtocolError{Opcode: frame.Opcode, Msg: "expected HelloOk"}
	}
	version, err := DecodeHelloOk(frame.Body)
	if err != nil {
		conn.Close()
		discovery.StopAgent()
		return nil, err
	}
	if version != ProtocolVersion {
		conn.Close()
		discovery.StopAgent()
		return nil, &ProtocolError{Msg: fmt.Sprintf("protocol version mismatch: %d != %d", version, ProtocolVersion)}
	}

	// Generate metadata
	sessionFileName := cfg.SessionFileName
	if sessionFileName == "" {
		sessionFileName = fmt.Sprintf("session-%d.dtj", time.Now().UnixMilli())
	}

	metadata, err := NewOpenSessionMetadata(sessionFileName, cfg.ProducerName, cfg.ProducerVersion)
	if err != nil {
		conn.Close()
		discovery.StopAgent()
		return nil, err
	}

	// OpenSession
	openFrame, err := EncodeOpenSession(metadata)
	if err != nil {
		conn.Close()
		discovery.StopAgent()
		return nil, err
	}
	if err := WriteFrame(conn, openFrame); err != nil {
		conn.Close()
		discovery.StopAgent()
		return nil, err
	}

	frame, err = ReadFrame(conn)
	if err != nil {
		conn.Close()
		discovery.StopAgent()
		return nil, err
	}
	if frame.Opcode == OpError {
		conn.Close()
		discovery.StopAgent()
		return nil, &ProtocolError{Opcode: OpError, Msg: DecodeError(frame.Body)}
	}
	if frame.Opcode != OpOpenSessionOk {
		conn.Close()
		discovery.StopAgent()
		return nil, &ProtocolError{Opcode: frame.Opcode, Msg: "expected OpenSessionOk"}
	}

	sess := &Session{
		conn:           conn,
		discovery:      discovery,
		metadata:       metadata,
		startTime:      time.Now(),
		domainCache:    make(map[string]uint32),
		categoryCache:  make(map[string]uint32),
		eventNameCache: make(map[string]uint32),
		stringCache:    make(map[string]uint32),
		warningHandler: warningHandler,
	}

	return sess, nil
}

// newNoOpSession creates a disabled no-op session
func newNoOpSession(warningHandler WarningHandler) *Session {
	return &Session{
		closed:         true,
		warningHandler: warningHandler,
		domainCache:    make(map[string]uint32),
		categoryCache:  make(map[string]uint32),
		eventNameCache: make(map[string]uint32),
		stringCache:    make(map[string]uint32),
	}
}

// Emit emits a single event with one field
func (s *Session) Emit(event Event) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.closed {
		return nil // No-op on disabled session
	}

	// Validate severity
	if event.Severity > SeverityFatal {
		return &ValueError{Msg: fmt.Sprintf("invalid severity: %d", event.Severity)}
	}

	// Get or intern dictionary entries
	domainID, err := s.getOrIntern(DictKindDomain, event.Domain, s.domainCache)
	if err != nil {
		return err
	}
	categoryID, err := s.getOrIntern(DictKindCategory, event.Category, s.categoryCache)
	if err != nil {
		return err
	}
	eventNameID, err := s.getOrIntern(DictKindEventName, event.Name, s.eventNameCache)
	if err != nil {
		return err
	}
	correlationID := uint32(0)
	if event.Correlation != "" {
		correlationID, err = s.getOrIntern(DictKindString, event.Correlation, s.stringCache)
		if err != nil {
			return err
		}
	}
	fieldNameID, err := s.getOrIntern(DictKindString, event.FieldName, s.stringCache)
	if err != nil {
		return err
	}

	// Encode value
	var typeTag TypeTag
	var valueBody []byte

	if strVal, ok := event.Value.(string); ok {
		// String values are interned
		valueID, err := s.getOrIntern(DictKindString, strVal, s.stringCache)
		if err != nil {
			return err
		}
		typeTag = TypeTagInterned
		valueBody = make([]byte, 4)
		binary.LittleEndian.PutUint32(valueBody, valueID)
	} else {
		typeTag, valueBody, err = EncodeValue(event.Value)
		if err != nil {
			return err
		}
	}

	// Current monotonic timestamp (relative to session start)
	monotonicNs := uint64(time.Since(s.startTime).Nanoseconds())

	// Send AppendEvent
	frame, err := EncodeAppendEvent(
		monotonicNs,
		domainID, categoryID, eventNameID, correlationID,
		event.Severity,
		fieldNameID,
		typeTag,
		valueBody,
	)
	if err != nil {
		return err
	}
	if err := WriteFrame(s.conn, frame); err != nil {
		return &ConnectionError{Msg: fmt.Sprintf("failed to write frame: %v", err)}
	}

	// Read response
	respFrame, err := ReadFrame(s.conn)
	if err != nil {
		return err
	}
	if respFrame.Opcode == OpError {
		return &ProtocolError{Opcode: OpError, Msg: DecodeError(respFrame.Body)}
	}
	if respFrame.Opcode != OpAppendEventOk {
		return &ProtocolError{Opcode: respFrame.Opcode, Msg: "expected AppendEventOk"}
	}

	_, err = DecodeAppendEventOk(respFrame.Body)
	return err
}

// getOrIntern gets or interns a dictionary entry
func (s *Session) getOrIntern(kind uint8, name string, cache map[string]uint32) (uint32, error) {
	if id, ok := cache[name]; ok {
		return id, nil
	}

	frame, err := EncodeIntern(kind, name)
	if err != nil {
		return 0, err
	}
	if err := WriteFrame(s.conn, frame); err != nil {
		return 0, &ConnectionError{Msg: fmt.Sprintf("failed to write frame: %v", err)}
	}

	respFrame, err := ReadFrame(s.conn)
	if err != nil {
		return 0, err
	}
	if respFrame.Opcode == OpError {
		return 0, &ProtocolError{Opcode: OpError, Msg: DecodeError(respFrame.Body)}
	}
	if respFrame.Opcode != OpInternOk {
		return 0, &ProtocolError{Opcode: respFrame.Opcode, Msg: "expected InternOk"}
	}

	id, err := DecodeInternOk(respFrame.Body)
	if err != nil {
		return 0, err
	}

	cache[name] = id
	return id, nil
}

// Close closes the session and cleans up
func (s *Session) Close() error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.closed {
		return nil // Idempotent
	}
	s.closed = true

	if s.conn != nil {
		// Send FinishSession
		finishFrame, err := EncodeFinishSession()
		if err == nil {
			WriteFrame(s.conn, finishFrame)
			// Read response (ignore errors)
			ReadFrame(s.conn)
		}

		s.conn.Close()
		s.conn = nil
	}

	if s.discovery != nil {
		s.discovery.StopAgent()
		s.discovery = nil
	}

	return nil
}

// emitWarning emits a warning via the warning handler
func (s *Session) emitWarning(err error) {
	if !s.warningOnce && s.warningHandler != nil {
		s.warningOnce = true
		s.warningHandler(err)
	}
}
