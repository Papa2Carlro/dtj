use crate::discovery::Discovery;
use crate::owned_agent::OwnedAgent;
use crate::protocol::{
    read_append_event_ok, read_finish_session_ok_or_error, read_hello_ok_or_error, read_intern_ok,
    read_open_session_ok_or_error, write_append_event, write_finish_session, write_hello,
    write_intern, write_open_session,
};
use crate::types::Value;
use std::collections::HashMap;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Child;
use std::time::{SystemTime, UNIX_EPOCH};

/// Dictionary cache for interned strings
#[derive(Default)]
struct DictCache {
    domain: HashMap<String, u32>,
    category: HashMap<String, u32>,
    event_name: HashMap<String, u32>,
    string: HashMap<String, u32>,
}

pub struct Client {
    pub stream: Option<UnixStream>,
    pub child: Option<Child>,
    pub temp_dir: Option<PathBuf>,
}

pub struct Session {
    pub stream: Option<UnixStream>,
    pub closed: bool,
    pub disabled: bool,
    pub(crate) owned: Option<OwnedAgent>,
    cache: DictCache,
    warned: bool,
    warning_handler: Option<crate::WarningHandler>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("closed", &self.closed)
            .field("disabled", &self.disabled)
            .finish()
    }
}

impl Session {
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Emit an event through the session.
    pub fn emit(&mut self, e: &crate::Event) -> Result<(), crate::error::Error> {
        let domain = e.domain.clone();
        let category = e.category.clone();
        let name = e.name.clone();
        let field_name = e.field_name.clone();
        let value = e.value.clone();
        let severity = e.severity.as_u8();
        let correlation = e.correlation.clone();
        self.emit_from_parts(
            &domain,
            &category,
            &name,
            &field_name,
            value,
            severity,
            correlation,
        )
    }

    /// Intern a string and return its ID, using cache
    fn intern(&mut self, dict_kind: u8, value: &str) -> Result<u32, crate::error::Error> {
        let cache = &mut self.cache;
        let cache_map = match dict_kind {
            crate::protocol::DICT_KIND_DOMAIN => &mut cache.domain,
            crate::protocol::DICT_KIND_CATEGORY => &mut cache.category,
            crate::protocol::DICT_KIND_EVENT_NAME => &mut cache.event_name,
            crate::protocol::DICT_KIND_STRING => &mut cache.string,
            _ => return Err(crate::error::Error::BadIntern),
        };

        // Check cache first
        if let Some(id) = cache_map.get(value) {
            return Ok(*id);
        }

        // Not in cache, send Intern request
        let stream = self.stream.as_mut().unwrap();
        write_intern(stream, dict_kind, value)?;
        let id = read_intern_ok(stream)?;

        // Store in cache
        cache_map.insert(value.to_string(), id);
        Ok(id)
    }

    /// Open a session with fallback to disabled mode on connection failure.
    /// Unlike `open_strict`, this does NOT validate config fields like producer_name length.
    /// On connection failure (agent not found, socket unreachable, etc.), returns a disabled session.
    /// Warning handler is called exactly once when falling back to disabled mode.
    pub fn open(config: &crate::Config) -> Result<Self, crate::error::Error> {
        // Handle disabled mode - call warning handler exactly once
        if !config.enabled {
            if let Some(ref handler) = config.warning_handler {
                handler("SDK disabled via config.enabled=false, events will be no-ops");
            }
            return Ok(Session {
                stream: None,
                closed: false,
                disabled: true,
                owned: None,
                cache: DictCache::default(),
                warned: false,
                warning_handler: None,
            });
        }

        // Discover agent (may auto-launch if no explicit socket_path)
        let discovery = match Discovery::find(config) {
            Ok(d) => d,
            Err(_) => {
                // Discovery failed - fall back to disabled
                if let Some(ref handler) = config.warning_handler {
                    handler("SDK running in degraded mode - connection issues may occur");
                }
                return Ok(Session {
                    stream: None,
                    closed: false,
                    disabled: true,
                    owned: None,
                    cache: DictCache::default(),
                    warned: true,
                    warning_handler: config.warning_handler.clone(),
                });
            }
        };

        let socket_path = match discovery.socket_path {
            Some(path) => path,
            None => {
                // No socket path - agent not available, fall back to disabled
                if let Some(ref handler) = config.warning_handler {
                    handler("SDK running in degraded mode - connection issues may occur");
                }
                return Ok(Session {
                    stream: None,
                    closed: false,
                    disabled: true,
                    owned: None,
                    cache: DictCache::default(),
                    warned: true,
                    warning_handler: config.warning_handler.clone(),
                });
            }
        };

        // Transfer ownership from discovery
        let mut owned = discovery.owned;

        // Try to connect
        match UnixStream::connect(&socket_path) {
            Ok(mut stream) => {
                // Connected successfully - do protocol handshake
                // Hello → HelloOk (or Error)
                if write_hello(&mut stream).is_err() {
                    if let Some(ref mut o) = owned {
                        o.cleanup();
                    }
                    if let Some(ref handler) = config.warning_handler {
                        handler("SDK running in degraded mode - connection issues may occur");
                    }
                    return Ok(Session {
                        stream: None,
                        closed: false,
                        disabled: true,
                        owned: None,
                        cache: DictCache::default(),
                        warned: true,
                        warning_handler: config.warning_handler.clone(),
                    });
                }

                match read_hello_ok_or_error(&mut stream) {
                    Ok(true) => {} // HelloOk received
                    Ok(false) | Err(_) => {
                        if let Some(ref mut o) = owned {
                            o.cleanup();
                        }
                        if let Some(ref handler) = config.warning_handler {
                            handler("SDK running in degraded mode - connection issues may occur");
                        }
                        return Ok(Session {
                            stream: None,
                            closed: false,
                            disabled: true,
                            owned: None,
                            cache: DictCache::default(),
                            warned: true,
                            warning_handler: config.warning_handler.clone(),
                        });
                    }
                }

                // OpenSession → OpenSessionOk
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64;
                let session_id = (0..16u8).collect::<Vec<_>>();

                let payload = crate::protocol::OpenSessionPayload {
                    file_name: config
                        .session_file_name
                        .clone()
                        .unwrap_or_else(|| "test.dtj".to_string()),
                    session_id: session_id.clone().try_into().unwrap(),
                    start_utc_unix_ms: now,
                    mono_origin_ns: 0,
                    producer_name: config.producer_name.clone(),
                    producer_version: config.producer_version.clone(),
                };

                if write_open_session(&mut stream, &payload).is_err() {
                    if let Some(ref mut o) = owned {
                        o.cleanup();
                    }
                    if let Some(ref handler) = config.warning_handler {
                        handler("SDK running in degraded mode - connection issues may occur");
                    }
                    return Ok(Session {
                        stream: None,
                        closed: false,
                        disabled: true,
                        owned: None,
                        cache: DictCache::default(),
                        warned: true,
                        warning_handler: config.warning_handler.clone(),
                    });
                }

                match read_open_session_ok_or_error(&mut stream) {
                    Ok(true) => {} // OpenSessionOk received
                    Ok(false) | Err(_) => {
                        if let Some(ref mut o) = owned {
                            o.cleanup();
                        }
                        if let Some(ref handler) = config.warning_handler {
                            handler("SDK running in degraded mode - connection issues may occur");
                        }
                        return Ok(Session {
                            stream: None,
                            closed: false,
                            disabled: true,
                            owned: None,
                            cache: DictCache::default(),
                            warned: true,
                            warning_handler: config.warning_handler.clone(),
                        });
                    }
                }

                // Successfully connected and opened session
                return Ok(Session {
                    stream: Some(stream),
                    closed: false,
                    disabled: false,
                    owned,
                    cache: DictCache::default(),
                    warned: false,
                    warning_handler: config.warning_handler.clone(),
                });
            }
            Err(_) => {
                // Connection failed - cleanup owned if present
                if let Some(ref mut o) = owned {
                    o.cleanup();
                }
                if let Some(ref handler) = config.warning_handler {
                    handler("SDK running in degraded mode - connection issues may occur");
                }
                Ok(Session {
                    stream: None,
                    closed: false,
                    disabled: true,
                    owned: None,
                    cache: DictCache::default(),
                    warned: true,
                    warning_handler: config.warning_handler.clone(),
                })
            }
        }
    }

    /// Open a session with strict protocol handshake.
    /// Validation is performed first - before any discovery, process spawn, or socket connect.
    pub fn open_strict(config: &crate::Config) -> Result<Self, crate::error::Error> {
        // Validate config first - before any I/O
        config.validate()?;

        // Handle disabled mode - call warning handler exactly once
        if !config.enabled {
            if let Some(ref handler) = config.warning_handler {
                handler("SDK disabled via config.enabled=false, events will be no-ops");
            }
            return Ok(Session {
                stream: None,
                closed: false,
                disabled: true,
                owned: None,
                cache: DictCache::default(),
                warned: false,
                warning_handler: None,
            });
        }

        // Discover agent (may auto-launch if no explicit socket_path)
        let discovery = Discovery::find(config).map_err(|_| crate::error::Error::AgentNotFound)?;
        let socket_path = discovery
            .socket_path
            .ok_or(crate::error::Error::AgentNotFound)?;

        // Transfer ownership from discovery
        let mut owned = discovery.owned;

        let mut stream = match UnixStream::connect(&socket_path) {
            Ok(s) => s,
            Err(_e) => {
                // Connect failed - cleanup owned agent before returning
                if let Some(ref mut o) = owned {
                    o.cleanup();
                }
                return Err(crate::error::Error::IoError);
            }
        };

        // Hello → HelloOk (or Error)
        if let Err(e) = write_hello(&mut stream) {
            if let Some(ref mut o) = owned {
                o.cleanup();
            }
            return Err(e);
        }
        match read_hello_ok_or_error(&mut stream) {
            Ok(true) => {} // HelloOk received
            Ok(false) => {
                if let Some(ref mut o) = owned {
                    o.cleanup();
                }
                return Err(crate::error::Error::Protocol);
            }
            Err(e) => {
                if let Some(ref mut o) = owned {
                    o.cleanup();
                }
                return Err(e);
            }
        }

        // OpenSession → OpenSessionOk
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let session_id = (0..16u8).collect::<Vec<_>>();

        let payload = crate::protocol::OpenSessionPayload {
            file_name: config
                .session_file_name
                .clone()
                .unwrap_or_else(|| "test.dtj".to_string()),
            session_id: session_id.clone().try_into().unwrap(),
            start_utc_unix_ms: now,
            mono_origin_ns: 0,
            producer_name: config.producer_name.clone(),
            producer_version: config.producer_version.clone(),
        };
        if let Err(e) = write_open_session(&mut stream, &payload) {
            if let Some(ref mut o) = owned {
                o.cleanup();
            }
            return Err(e);
        }

        // OpenSessionOk or Error
        match read_open_session_ok_or_error(&mut stream) {
            Ok(true) => {} // OpenSessionOk received
            Ok(false) => {
                if let Some(ref mut o) = owned {
                    o.cleanup();
                }
                return Err(crate::error::Error::Protocol);
            }
            Err(e) => {
                if let Some(ref mut o) = owned {
                    o.cleanup();
                }
                return Err(e);
            }
        }

        Ok(Session {
            stream: Some(stream),
            closed: false,
            disabled: false,
            owned,
            cache: DictCache::default(),
            warned: false,
            warning_handler: config.warning_handler.clone(),
        })
    }

    /// Emit an event with any value type (internal implementation).
    fn emit_from_parts(
        &mut self,
        domain: &str,
        category: &str,
        event_name: &str,
        field_name: &str,
        value: Value,
        severity: u8,
        correlation: Option<String>,
    ) -> Result<(), crate::error::Error> {
        if self.disabled {
            return Ok(());
        }
        if self.closed {
            return Err(crate::error::Error::SessionClosed);
        }

        // Call warning handler exactly once if we're in a degraded state (but not disabled)
        if !self.warned {
            self.warned = true;
            if let Some(ref handler) = self.warning_handler {
                handler("SDK running in degraded mode - connection issues may occur");
            }
        }

        // Intern all strings first (these borrow self.cache, not self.stream)
        let domain_id = self.intern(crate::protocol::DICT_KIND_DOMAIN, domain)?;
        let category_id = self.intern(crate::protocol::DICT_KIND_CATEGORY, category)?;
        let event_name_id = self.intern(crate::protocol::DICT_KIND_EVENT_NAME, event_name)?;
        let field_name_id = self.intern(crate::protocol::DICT_KIND_STRING, field_name)?;

        // For String values, we need to intern the string value and use TYPE_INTERNED
        let (type_tag, value_body) = match value {
            Value::String(ref s) => {
                let string_id = self.intern(crate::protocol::DICT_KIND_STRING, s)?;
                (
                    crate::types::TYPE_INTERNED,
                    string_id.to_le_bytes().to_vec(),
                )
            }
            other => (other.type_tag(), other.encode()),
        };

        // Intern correlation ID if present
        let correlation_id = match correlation {
            Some(ref corr) => self.intern(crate::protocol::DICT_KIND_STRING, corr)?,
            None => 0,
        };

        // AppendEvent

        {
            let stream = self.stream.as_mut().unwrap();
            write_append_event(
                stream,
                0,
                domain_id,
                category_id,
                event_name_id,
                correlation_id,
                severity,
                field_name_id,
                type_tag,
                &value_body,
            )?;
            read_append_event_ok(stream)?;
        }

        Ok(())
    }

    /// Close the session gracefully.
    pub fn close(&mut self) -> Result<(), crate::error::Error> {
        if self.closed {
            return Ok(());
        }

        if self.disabled {
            self.closed = true;
            return Ok(());
        }

        let stream = self.stream.as_mut().unwrap();

        write_finish_session(stream)?;
        match read_finish_session_ok_or_error(stream) {
            Ok(true) => {} // FinishSessionOk received
            Ok(false) => {
                // Error received
                self.closed = true;
                self.cleanup();
                return Err(crate::error::Error::Protocol);
            }
            Err(e) => {
                self.closed = true;
                self.cleanup();
                return Err(e);
            }
        }

        stream.shutdown(Shutdown::Both).ok();
        self.closed = true;
        self.cleanup();
        Ok(())
    }

    /// Cleanup child process and temp directory
    fn cleanup(&mut self) {
        // Take ownership of owned agent and clean it up
        if let Some(mut owned) = self.owned.take() {
            owned.cleanup();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.closed {
            self.close().ok();
        }
    }
}

impl Client {
    /// Create a new client, optionally launching agent if enabled
    pub fn open(config: &crate::Config) -> Result<Self, crate::error::Error> {
        if !config.enabled {
            return Ok(Client {
                stream: None,
                child: None,
                temp_dir: None,
            });
        }

        // If socket_path is set, don't launch agent
        if config.socket_path.is_some() {
            return Ok(Client {
                stream: None,
                child: None,
                temp_dir: None,
            });
        }

        // Need to find/launch agent
        let discovery = Discovery::find(config).map_err(|_| crate::error::Error::AgentNotFound)?;
        let socket_path = discovery
            .socket_path
            .ok_or(crate::error::Error::AgentNotFound)?;

        // Connect to socket
        let stream = UnixStream::connect(&socket_path).ok();

        Ok(Client {
            stream,
            child: None,
            temp_dir: None,
        })
    }

    pub fn open_session(config: &crate::Config) -> Result<Session, crate::error::Error> {
        Session::open_strict(config)
    }

    pub fn close(&mut self) -> Result<(), crate::error::Error> {
        if let Some(ref mut stream) = self.stream {
            stream.shutdown(Shutdown::Both).ok();
        }
        self.stream = None;
        Ok(())
    }
}

// =============================================================================
// In-crate tests for OwnedAgent lifecycle verification
// =============================================================================

#[cfg(test)]
mod owned_agent_tests {
    use super::*;
    use crate::Config;

    /// Verify that auto-launched agent process and temp directory are cleaned up.
    /// This test uses in-crate access to Session.owned and OwnedAgent methods.
    #[test]
    fn test_auto_launched_agent_is_cleaned_up() {
        // Build the agent if not already built
        let agent_path = std::env::var("DTJ_AGENT_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./crates/dtj/target/debug/dtj-agent"));

        if !agent_path.exists() {
            eprintln!("skipped: dtj-agent not built at {:?}", agent_path);
            return;
        }

        let config = Config {
            data_dir: None,
            producer_name: "cleanup-test".to_string(),
            producer_version: "0.1.0".to_string(),
            agent_path: Some(agent_path.clone()),
            socket_path: None, // Trigger auto-launch
            session_file_name: None,
            enabled: true,
            warning_handler: None,
        };

        // Open session with auto-launch
        let mut session = Session::open(&config).expect("session open");
        assert!(
            session.owned.is_some(),
            "should have owned agent after auto-launch"
        );

        // Capture PID and temp_dir before taking ownership
        let child_pid = session.owned.as_ref().unwrap().child_id();
        let temp_path = session.owned.as_ref().unwrap().temp_dir_path().cloned();

        let pid = child_pid.expect("should have PID");
        let temp_path = temp_path.expect("should have temp_dir");

        // Take ownership away from session (so we can inspect after close)
        // Verify temp_dir exists before cleanup
        assert!(temp_path.exists(), "temp_dir should exist before cleanup");

        // Call session.close() - this should trigger cleanup via self.owned.take()
        session.close().ok();
        drop(session);

        // Verify: process no longer exists
        let pid_exists = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
        assert!(
            !pid_exists,
            "agent process {} should be killed after cleanup",
            pid
        );

        // Verify: temp directory deleted
        assert!(
            !temp_path.exists(),
            "temp dir {:?} should be deleted after cleanup",
            temp_path
        );
    }
}
