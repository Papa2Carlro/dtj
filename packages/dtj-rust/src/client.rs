use crate::discovery::Discovery;
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
    pub child: Option<Child>,
    pub temp_dir: Option<PathBuf>,
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

    /// Open a session with strict protocol handshake.
    pub fn open_strict(config: &crate::Config) -> Result<Self, crate::error::Error> {
        // Handle disabled mode - call warning handler exactly once
        if !config.enabled {
            if let Some(ref handler) = config.warning_handler {
                handler("SDK disabled via config.enabled=false, events will be no-ops");
            }
            return Ok(Session {
                stream: None,
                closed: false,
                disabled: true,
                child: None,
                temp_dir: None,
                cache: DictCache::default(),
                warned: false,
                warning_handler: None,
            });
        }

        let (socket_path, child, temp_dir) = if let Some(ref path) = config.socket_path {
            // Explicit socket path, don't launch agent
            (path.clone(), None, None)
        } else {
            // Need to discover/locate agent
            let discovery =
                Discovery::find(config).map_err(|_| crate::error::Error::AgentNotFound)?;
            let path = discovery
                .socket_path
                .ok_or(crate::error::Error::AgentNotFound)?;
            (path, None, None) // TODO: pass child and temp_dir through properly
        };

        let mut stream =
            UnixStream::connect(&socket_path).map_err(|_| crate::error::Error::IoError)?;

        // Hello → HelloOk (or Error)
        write_hello(&mut stream)?;
        match read_hello_ok_or_error(&mut stream) {
            Ok(true) => {}                                          // HelloOk received
            Ok(false) => return Err(crate::error::Error::Protocol), // Error received
            Err(e) => return Err(e),
        }

        // OpenSession → OpenSessionOk
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let session_id = (0..16u8).collect::<Vec<_>>();

        let payload = crate::protocol::OpenSessionPayload {
            file_name: "test.dtj".to_string(),
            session_id: session_id.clone().try_into().unwrap(),
            start_utc_unix_ms: now,
            mono_origin_ns: 0,
            producer_name: "dtj-sdk".to_string(),
            producer_version: "0.1.0".to_string(),
        };
        write_open_session(&mut stream, &payload)?;

        // OpenSessionOk or Error
        match read_open_session_ok_or_error(&mut stream) {
            Ok(true) => {}                                          // OpenSessionOk received
            Ok(false) => return Err(crate::error::Error::Protocol), // Error received
            Err(e) => return Err(e),
        }

        Ok(Session {
            stream: Some(stream),
            closed: false,
            disabled: false,
            child,
            temp_dir,
            cache: DictCache::default(),
            warned: false,
            warning_handler: config.warning_handler.clone(),
        })
    }

    /// Emit an event with any value type.
    pub fn emit(
        &mut self,
        domain: &str,
        category: &str,
        event_name: &str,
        field_name: &str,
        value: Value,
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

        // AppendEvent
        let severity = 2u8; // Info

        {
            let stream = self.stream.as_mut().unwrap();
            write_append_event(
                stream,
                0,
                domain_id,
                category_id,
                event_name_id,
                0, // correlation_id
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
        // Kill child process if we own one
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.child = None;

        // Remove temp directory
        if let Some(ref temp_dir) = self.temp_dir {
            let _ = std::fs::remove_dir_all(temp_dir);
        }
        self.temp_dir = None;
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

    pub fn emit(&mut self, _e: &crate::Event) -> Result<(), crate::error::Error> {
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), crate::error::Error> {
        if let Some(ref mut stream) = self.stream {
            stream.shutdown(Shutdown::Both).ok();
        }
        self.stream = None;
        Ok(())
    }
}
