package dtj

import (
	"bytes"
	"crypto/rand"
	"encoding/binary"
	"fmt"
	"io"
	"math"
	"time"
)

// Protocol constants
const (
	ProtocolVersion = 1
	MaxFrameSize    = 1_048_576 // 1 MiB
)

// Opcodes
const (
	// Client -> Server
	OpHello         = 0x01
	OpOpenSession   = 0x02
	OpAppendEvent   = 0x03
	OpFinishSession = 0x04
	OpPing          = 0x05
	OpIntern        = 0x06

	// Server -> Client
	OpHelloOk         = 0x81
	OpOpenSessionOk   = 0x82
	OpAppendEventOk   = 0x83
	OpFinishSessionOk = 0x84
	OpPong            = 0x85
	OpInternOk        = 0x86
	OpError           = 0xFF
)

// Dictionary kinds
const (
	DictKindDomain    = 1
	DictKindCategory  = 2
	DictKindEventName = 3
	DictKindString    = 4
)

// Severity levels (match dtj::Severity)
type Severity uint8

const (
	SeverityDebug Severity = 0
	SeverityInfo  Severity = 1
	SeverityWarn  Severity = 2
	SeverityError Severity = 3
	SeverityFatal Severity = 4
)

// Type tags (match dtj::Value)
type TypeTag uint8

const (
	TypeTagBool     TypeTag = 0x01
	TypeTagI32      TypeTag = 0x02
	TypeTagI64      TypeTag = 0x03
	TypeTagU32      TypeTag = 0x04
	TypeTagU64      TypeTag = 0x05
	TypeTagF32      TypeTag = 0x06
	TypeTagF64      TypeTag = 0x07
	TypeTagEnum     TypeTag = 0x08
	TypeTagVec2F32  TypeTag = 0x09
	TypeTagVec3F32  TypeTag = 0x0A
	TypeTagInterned TypeTag = 0x0B
	TypeTagBytes    TypeTag = 0x0C
)

// Frame represents a protocol frame
type Frame struct {
	Opcode byte
	Body   []byte
}

// EncodeFrame encodes a frame with length prefix
func EncodeFrame(opcode byte, body []byte) ([]byte, error) {
	length := 1 + len(body) // includes opcode
	if length > MaxFrameSize {
		return nil, &ProtocolError{Opcode: opcode, Msg: fmt.Sprintf("frame too large: %d > %d", length, MaxFrameSize)}
	}
	buf := make([]byte, 4+length)
	binary.LittleEndian.PutUint32(buf[0:4], uint32(length))
	buf[4] = opcode
	copy(buf[5:], body)
	return buf, nil
}

// DecodeFrame decodes a frame from a reader
func DecodeFrame(r io.Reader) (*Frame, error) {
	// Read length (4 bytes)
	lenBuf := make([]byte, 4)
	if _, err := io.ReadFull(r, lenBuf); err != nil {
		return nil, &ProtocolError{Msg: fmt.Sprintf("failed to read frame length: %v", err)}
	}
	length := binary.LittleEndian.Uint32(lenBuf)
	if length > MaxFrameSize {
		return nil, &ProtocolError{Msg: fmt.Sprintf("frame too large: %d", length)}
	}

	// Read payload (length bytes including opcode)
	payload := make([]byte, length)
	if _, err := io.ReadFull(r, payload); err != nil {
		return nil, &ProtocolError{Msg: fmt.Sprintf("failed to read frame payload: %v", err)}
	}

	return &Frame{
		Opcode: payload[0],
		Body:   payload[1:],
	}, nil
}

// EncodeHello encodes a Hello frame
func EncodeHello() ([]byte, error) {
	body := make([]byte, 4)
	binary.LittleEndian.PutUint32(body, ProtocolVersion)
	return EncodeFrame(OpHello, body)
}

// DecodeHelloOk decodes a HelloOk response
func DecodeHelloOk(body []byte) (uint32, error) {
	if len(body) != 4 {
		return 0, &ProtocolError{Msg: "HelloOk body must be 4 bytes"}
	}
	return binary.LittleEndian.Uint32(body), nil
}

// OpenSessionMetadata holds metadata for OpenSession
type OpenSessionMetadata struct {
	FileName        string
	SessionID       [16]byte
	StartUtcUnixMs  int64
	MonoOriginNs    uint64
	ProducerName    string
	ProducerVersion string
}

// NewOpenSessionMetadata creates metadata with auto-generated values
func NewOpenSessionMetadata(fileName, producerName, producerVersion string) (*OpenSessionMetadata, error) {
	if len(producerName) > 32 {
		return nil, &ValueError{Msg: "producer_name must be <= 32 bytes"}
	}
	if len(producerVersion) > 16 {
		return nil, &ValueError{Msg: "producer_version must be <= 16 bytes"}
	}

	var sessionID [16]byte
	if _, err := rand.Read(sessionID[:]); err != nil {
		return nil, &ProtocolError{Msg: fmt.Sprintf("failed to generate session ID: %v", err)}
	}

	return &OpenSessionMetadata{
		FileName:        fileName,
		SessionID:       sessionID,
		StartUtcUnixMs:  time.Now().UnixMilli(),
		MonoOriginNs:    uint64(time.Now().UnixNano()), // Using wall clock as approximation
		ProducerName:    producerName,
		ProducerVersion: producerVersion,
	}, nil
}

// EncodeOpenSession encodes an OpenSession frame
func EncodeOpenSession(meta *OpenSessionMetadata) ([]byte, error) {
	fileNameBytes := []byte(meta.FileName)
	producerNameBytes := []byte(meta.ProducerName)
	producerVersionBytes := []byte(meta.ProducerVersion)

	body := new(bytes.Buffer)
	binary.Write(body, binary.LittleEndian, uint16(len(fileNameBytes)))
	body.Write(fileNameBytes)
	body.Write(meta.SessionID[:])
	binary.Write(body, binary.LittleEndian, meta.StartUtcUnixMs)
	binary.Write(body, binary.LittleEndian, meta.MonoOriginNs)
	binary.Write(body, binary.LittleEndian, uint16(len(producerNameBytes)))
	body.Write(producerNameBytes)
	binary.Write(body, binary.LittleEndian, uint16(len(producerVersionBytes)))
	body.Write(producerVersionBytes)

	return EncodeFrame(OpOpenSession, body.Bytes())
}

// EncodeIntern encodes an Intern request
func EncodeIntern(kind uint8, name string) ([]byte, error) {
	nameBytes := []byte(name)
	if len(nameBytes) > 1024 {
		return nil, &ValueError{Msg: "name too long (max 1024 bytes)"}
	}

	body := new(bytes.Buffer)
	body.WriteByte(kind)
	binary.Write(body, binary.LittleEndian, uint16(len(nameBytes)))
	body.Write(nameBytes)

	return EncodeFrame(OpIntern, body.Bytes())
}

// DecodeInternOk decodes an InternOk response
func DecodeInternOk(body []byte) (uint32, error) {
	if len(body) != 4 {
		return 0, &ProtocolError{Msg: "InternOk body must be 4 bytes"}
	}
	return binary.LittleEndian.Uint32(body), nil
}

// EncodeAppendEvent encodes an AppendEvent frame with single field (MVP)
func EncodeAppendEvent(
	monotonicNs uint64,
	domainID, categoryID, eventNameID, correlationID uint32,
	severity Severity,
	fieldNameID uint32,
	typeTag TypeTag,
	valueBody []byte,
) ([]byte, error) {
	body := new(bytes.Buffer)
	binary.Write(body, binary.LittleEndian, monotonicNs)
	binary.Write(body, binary.LittleEndian, domainID)
	binary.Write(body, binary.LittleEndian, categoryID)
	binary.Write(body, binary.LittleEndian, eventNameID)
	binary.Write(body, binary.LittleEndian, correlationID)
	body.WriteByte(byte(severity))
	binary.Write(body, binary.LittleEndian, uint16(1)) // field_count = 1
	binary.Write(body, binary.LittleEndian, fieldNameID)
	body.WriteByte(byte(typeTag))
	body.Write([]byte{0, 0, 0}) // reserved
	body.Write(valueBody)

	return EncodeFrame(OpAppendEvent, body.Bytes())
}

// DecodeAppendEventOk decodes an AppendEventOk response
func DecodeAppendEventOk(body []byte) (uint64, error) {
	if len(body) != 8 {
		return 0, &ProtocolError{Msg: "AppendEventOk body must be 8 bytes"}
	}
	return binary.LittleEndian.Uint64(body), nil
}

// EncodeFinishSession encodes a FinishSession frame
func EncodeFinishSession() ([]byte, error) {
	return EncodeFrame(OpFinishSession, nil)
}

// EncodePing encodes a Ping frame
func EncodePing() ([]byte, error) {
	return EncodeFrame(OpPing, nil)
}

// DecodeError decodes an Error frame
func DecodeError(body []byte) string {
	return string(body)
}

// EncodeValue encodes a Go value to (typeTag, valueBody)
func EncodeValue(v any) (TypeTag, []byte, error) {
	switch val := v.(type) {
	case bool:
		b := byte(0)
		if val {
			b = 1
		}
		return TypeTagBool, []byte{b}, nil

	case int8:
		return TypeTagI64, int64ToBytes(int64(val)), nil
	case int16:
		return TypeTagI64, int64ToBytes(int64(val)), nil
	case int32:
		return TypeTagI64, int64ToBytes(int64(val)), nil
	case int64:
		return TypeTagI64, int64ToBytes(val), nil
	case int:
		// Check if fits in int64
		if val >= math.MinInt64 && val <= math.MaxInt64 {
			return TypeTagI64, int64ToBytes(int64(val)), nil
		}
		return 0, nil, &ValueError{Msg: fmt.Sprintf("int out of range: %d", val)}

	case uint8:
		return TypeTagU64, uint64ToBytes(uint64(val)), nil
	case uint16:
		return TypeTagU64, uint64ToBytes(uint64(val)), nil
	case uint32:
		return TypeTagU64, uint64ToBytes(uint64(val)), nil
	case uint64:
		return TypeTagU64, uint64ToBytes(val), nil
	case uint:
		return TypeTagU64, uint64ToBytes(uint64(val)), nil

	case float32:
		return TypeTagF64, float64ToBytes(float64(val)), nil
	case float64:
		return TypeTagF64, float64ToBytes(val), nil

	case string:
		// Strings must be interned separately
		return 0, nil, &ValueError{Msg: "strings must be interned first; use field_name_id from intern"}

	case []byte:
		if len(val) > math.MaxUint32 {
			return 0, nil, &ValueError{Msg: "bytes too long"}
		}
		body := make([]byte, 4+len(val))
		binary.LittleEndian.PutUint32(body[0:4], uint32(len(val)))
		copy(body[4:], val)
		return TypeTagBytes, body, nil

	default:
		return 0, nil, &ValueError{Msg: fmt.Sprintf("unsupported value type: %T", v)}
	}
}

func int64ToBytes(v int64) []byte {
	b := make([]byte, 8)
	binary.LittleEndian.PutUint64(b, uint64(v))
	return b
}

func uint64ToBytes(v uint64) []byte {
	b := make([]byte, 8)
	binary.LittleEndian.PutUint64(b, v)
	return b
}

func float64ToBytes(v float64) []byte {
	b := make([]byte, 8)
	binary.LittleEndian.PutUint64(b, math.Float64bits(v))
	return b
}

// SeverityFromString converts a string to Severity
func SeverityFromString(s string) (Severity, error) {
	switch s {
	case "debug":
		return SeverityDebug, nil
	case "info":
		return SeverityInfo, nil
	case "warn":
		return SeverityWarn, nil
	case "error":
		return SeverityError, nil
	case "fatal":
		return SeverityFatal, nil
	default:
		return 0, &ValueError{Msg: fmt.Sprintf("invalid severity: %s", s)}
	}
}

// String returns the string representation of Severity
func (s Severity) String() string {
	switch s {
	case SeverityDebug:
		return "debug"
	case SeverityInfo:
		return "info"
	case SeverityWarn:
		return "warn"
	case SeverityError:
		return "error"
	case SeverityFatal:
		return "fatal"
	default:
		return "unknown"
	}
}
