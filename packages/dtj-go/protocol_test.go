package dtj

import (
	"bytes"
	"encoding/binary"
	"testing"
)

func TestEncodeDecodeFrame(t *testing.T) {
	body := []byte{1, 2, 3, 4}
	frame, err := EncodeFrame(0x01, body)
	if err != nil {
		t.Fatalf("EncodeFrame failed: %v", err)
	}

	// Verify frame structure
	if len(frame) < 5 {
		t.Fatalf("frame too short: %d", len(frame))
	}
	length := binary.LittleEndian.Uint32(frame[0:4])
	if length != 5 { // 1 opcode + 4 body
		t.Fatalf("wrong frame length: %d", length)
	}
	if frame[4] != 0x01 {
		t.Fatalf("wrong opcode: 0x%02x", frame[4])
	}
	if !bytes.Equal(frame[5:], body) {
		t.Fatalf("body mismatch")
	}

	// Decode
	r := bytes.NewReader(frame)
	decoded, err := DecodeFrame(r)
	if err != nil {
		t.Fatalf("DecodeFrame failed: %v", err)
	}
	if decoded.Opcode != 0x01 {
		t.Fatalf("decoded opcode mismatch: 0x%02x", decoded.Opcode)
	}
	if !bytes.Equal(decoded.Body, body) {
		t.Fatalf("decoded body mismatch")
	}
}

func TestEncodeDecodeHello(t *testing.T) {
	frame, err := EncodeHello()
	if err != nil {
		t.Fatalf("EncodeHello failed: %v", err)
	}

	r := bytes.NewReader(frame)
	decoded, err := DecodeFrame(r)
	if err != nil {
		t.Fatalf("DecodeFrame failed: %v", err)
	}
	if decoded.Opcode != OpHello {
		t.Fatalf("wrong opcode: 0x%02x", decoded.Opcode)
	}

	version, err := DecodeHelloOk(decoded.Body)
	if err != nil {
		t.Fatalf("DecodeHelloOk failed: %v", err)
	}
	if version != ProtocolVersion {
		t.Fatalf("version mismatch: %d != %d", version, ProtocolVersion)
	}
}

func TestOpenSessionMetadata(t *testing.T) {
	meta, err := NewOpenSessionMetadata("test.dtj", "my-service", "1.0.0")
	if err != nil {
		t.Fatalf("NewOpenSessionMetadata failed: %v", err)
	}

	if meta.FileName != "test.dtj" {
		t.Fatalf("filename mismatch: %s", meta.FileName)
	}
	if meta.ProducerName != "my-service" {
		t.Fatalf("producer name mismatch: %s", meta.ProducerName)
	}
	if meta.ProducerVersion != "1.0.0" {
		t.Fatalf("producer version mismatch: %s", meta.ProducerVersion)
	}
	if meta.SessionID == [16]byte{} {
		t.Fatalf("session ID not generated")
	}
	if meta.StartUtcUnixMs == 0 {
		t.Fatalf("start time not set")
	}
	if meta.MonoOriginNs == 0 {
		t.Fatalf("mono origin not set")
	}
}

func TestOpenSessionMetadataValidation(t *testing.T) {
	// Test producer name too long
	_, err := NewOpenSessionMetadata("test.dtj", string(make([]byte, 33)), "1.0.0")
	if err == nil {
		t.Fatalf("expected error for long producer name")
	}

	// Test producer version too long
	_, err = NewOpenSessionMetadata("test.dtj", "my-service", string(make([]byte, 17)))
	if err == nil {
		t.Fatalf("expected error for long producer version")
	}
}

func TestEncodeOpenSession(t *testing.T) {
	meta, err := NewOpenSessionMetadata("test.dtj", "my-service", "1.0.0")
	if err != nil {
		t.Fatalf("NewOpenSessionMetadata failed: %v", err)
	}

	frame, err := EncodeOpenSession(meta)
	if err != nil {
		t.Fatalf("EncodeOpenSession failed: %v", err)
	}

	r := bytes.NewReader(frame)
	decoded, err := DecodeFrame(r)
	if err != nil {
		t.Fatalf("DecodeFrame failed: %v", err)
	}
	if decoded.Opcode != OpOpenSession {
		t.Fatalf("wrong opcode: 0x%02x", decoded.Opcode)
	}
}

func TestEncodeIntern(t *testing.T) {
	frame, err := EncodeIntern(DictKindDomain, "api")
	if err != nil {
		t.Fatalf("EncodeIntern failed: %v", err)
	}

	r := bytes.NewReader(frame)
	decoded, err := DecodeFrame(r)
	if err != nil {
		t.Fatalf("DecodeFrame failed: %v", err)
	}
	if decoded.Opcode != OpIntern {
		t.Fatalf("wrong opcode: 0x%02x", decoded.Opcode)
	}
	if decoded.Body[0] != DictKindDomain {
		t.Fatalf("wrong dict kind: %d", decoded.Body[0])
	}
}

func TestEncodeInternValidation(t *testing.T) {
	// Test name too long
	_, err := EncodeIntern(DictKindDomain, string(make([]byte, 1025)))
	if err == nil {
		t.Fatalf("expected error for long name")
	}
}

func TestDecodeInternOk(t *testing.T) {
	body := make([]byte, 4)
	binary.LittleEndian.PutUint32(body, 42)
	id, err := DecodeInternOk(body)
	if err != nil {
		t.Fatalf("DecodeInternOk failed: %v", err)
	}
	if id != 42 {
		t.Fatalf("id mismatch: %d", id)
	}
}

func TestEncodeAppendEvent(t *testing.T) {
	frame, err := EncodeAppendEvent(
		1000,
		1, 2, 3, 4,
		SeverityInfo,
		5,
		TypeTagI64,
		int64ToBytes(42),
	)
	if err != nil {
		t.Fatalf("EncodeAppendEvent failed: %v", err)
	}

	r := bytes.NewReader(frame)
	decoded, err := DecodeFrame(r)
	if err != nil {
		t.Fatalf("DecodeFrame failed: %v", err)
	}
	if decoded.Opcode != OpAppendEvent {
		t.Fatalf("wrong opcode: 0x%02x", decoded.Opcode)
	}
}

func TestDecodeAppendEventOk(t *testing.T) {
	body := make([]byte, 8)
	binary.LittleEndian.PutUint64(body, 123)
	seq, err := DecodeAppendEventOk(body)
	if err != nil {
		t.Fatalf("DecodeAppendEventOk failed: %v", err)
	}
	if seq != 123 {
		t.Fatalf("seq mismatch: %d", seq)
	}
}

func TestEncodeFinishSession(t *testing.T) {
	frame, err := EncodeFinishSession()
	if err != nil {
		t.Fatalf("EncodeFinishSession failed: %v", err)
	}

	r := bytes.NewReader(frame)
	decoded, err := DecodeFrame(r)
	if err != nil {
		t.Fatalf("DecodeFrame failed: %v", err)
	}
	if decoded.Opcode != OpFinishSession {
		t.Fatalf("wrong opcode: 0x%02x", decoded.Opcode)
	}
	if len(decoded.Body) != 0 {
		t.Fatalf("body should be empty")
	}
}

func TestEncodePing(t *testing.T) {
	frame, err := EncodePing()
	if err != nil {
		t.Fatalf("EncodePing failed: %v", err)
	}

	r := bytes.NewReader(frame)
	decoded, err := DecodeFrame(r)
	if err != nil {
		t.Fatalf("DecodeFrame failed: %v", err)
	}
	if decoded.Opcode != OpPing {
		t.Fatalf("wrong opcode: 0x%02x", decoded.Opcode)
	}
}

func TestEncodeValue(t *testing.T) {
	tests := []struct {
		name    string
		value   any
		wantTag TypeTag
		wantErr bool
	}{
		{"bool true", true, TypeTagBool, false},
		{"bool false", false, TypeTagBool, false},
		{"int8", int8(42), TypeTagI64, false},
		{"int16", int16(42), TypeTagI64, false},
		{"int32", int32(42), TypeTagI64, false},
		{"int64", int64(42), TypeTagI64, false},
		{"int", 42, TypeTagI64, false},
		{"uint8", uint8(42), TypeTagU64, false},
		{"uint16", uint16(42), TypeTagU64, false},
		{"uint32", uint32(42), TypeTagU64, false},
		{"uint64", uint64(42), TypeTagU64, false},
		{"uint", uint(42), TypeTagU64, false},
		{"float32", float32(3.14), TypeTagF64, false},
		{"float64", 3.14, TypeTagF64, false},
		{"bytes", []byte{1, 2, 3}, TypeTagBytes, false},
		{"string", "hello", 0, true}, // strings must be interned
		{"unsupported", struct{}{}, 0, true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tag, body, err := EncodeValue(tt.value)
			if tt.wantErr {
				if err == nil {
					t.Fatalf("expected error for %s", tt.name)
				}
				return
			}
			if err != nil {
				t.Fatalf("EncodeValue failed for %s: %v", tt.name, err)
			}
			if tag != tt.wantTag {
				t.Fatalf("tag mismatch for %s: 0x%02x != 0x%02x", tt.name, tag, tt.wantTag)
			}
			if len(body) == 0 {
				t.Fatalf("empty body for %s", tt.name)
			}
		})
	}
}

func TestSeverityFromString(t *testing.T) {
	tests := []struct {
		input   string
		want    Severity
		wantErr bool
	}{
		{"debug", SeverityDebug, false},
		{"info", SeverityInfo, false},
		{"warn", SeverityWarn, false},
		{"error", SeverityError, false},
		{"fatal", SeverityFatal, false},
		{"invalid", 0, true},
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			got, err := SeverityFromString(tt.input)
			if tt.wantErr {
				if err == nil {
					t.Fatalf("expected error for %s", tt.input)
				}
				return
			}
			if err != nil {
				t.Fatalf("SeverityFromString failed for %s: %v", tt.input, err)
			}
			if got != tt.want {
				t.Fatalf("severity mismatch for %s: %d != %d", tt.input, got, tt.want)
			}
		})
	}
}

func TestSeverityString(t *testing.T) {
	tests := []struct {
		sev  Severity
		want string
	}{
		{SeverityDebug, "debug"},
		{SeverityInfo, "info"},
		{SeverityWarn, "warn"},
		{SeverityError, "error"},
		{SeverityFatal, "fatal"},
		{Severity(99), "unknown"},
	}

	for _, tt := range tests {
		if tt.sev.String() != tt.want {
			t.Fatalf("Severity.String() mismatch for %d: %s != %s", tt.sev, tt.sev.String(), tt.want)
		}
	}
}
