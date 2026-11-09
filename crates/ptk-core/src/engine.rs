//! Client for the Python engine (`engine/ptk_engine`).
//!
//! The engine runs as a child process and speaks JSON-RPC 2.0 over
//! stdin/stdout, one JSON message per line. Its stderr is passed through for
//! diagnostics. Requests without a matching response id are protocol errors;
//! messages without an id are notifications (for example `progress`).
//!
//! The interpreter is found in this order:
//!
//! 1. `PTK_PYTHON`, an explicit interpreter.
//! 2. The managed environment `ptk doctor` creates in the data directory.
//! 3. `python3` (`python` on Windows) on `PATH`, with the engine source on
//!    `PYTHONPATH`. The engine itself only needs the standard library.
//!
//! The engine source is found through `PTK_ENGINE_DIR`, next to the `ptk`
//! executable (`engine/` or `../share/porting-toolkit/engine`), or in this
//! source checkout.

use std::env;
use std::ffi::OsString;
use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use thiserror::Error;

use crate::paths;

/// Version of the Rust<->Python protocol. Bumped on breaking changes.
pub const PROTOCOL_VERSION: u32 = 1;

/// JSON-RPC error codes used by the engine.
pub mod codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
    /// A normal failure, e.g. a folder that does not exist.
    pub const ENGINE_ERROR: i64 = -32000;
}

/// Errors talking to the engine.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("the Python engine was not found: {0}\nRun `ptk doctor` to set it up.")]
    NotFound(String),
    #[error("could not start {program}: {source}\nRun `ptk doctor` to set up the engine.")]
    Spawn {
        program: String,
        source: std::io::Error,
    },
    #[error("engine I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("the engine stopped unexpectedly ({0}); see the messages above")]
    Exited(String),
    #[error("engine protocol error: {0}")]
    Protocol(String),
    #[error("engine protocol {found} does not match ptk's {expected}; run `ptk doctor`")]
    ProtocolMismatch { expected: u32, found: u32 },
    #[error("{message}")]
    Rpc {
        code: i64,
        message: String,
        data: Option<Value>,
    },
}

/// Where the interpreter came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineOrigin {
    EnvOverride,
    Managed,
    SystemPython,
}

impl fmt::Display for EngineOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            EngineOrigin::EnvOverride => "PTK_PYTHON",
            EngineOrigin::Managed => "managed environment",
            EngineOrigin::SystemPython => "system Python",
        })
    }
}

/// How to start the engine.
#[derive(Clone, Debug)]
pub struct EngineCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, OsString)>,
    pub origin: EngineOrigin,
    pub engine_dir: Option<PathBuf>,
}

impl EngineCommand {
    /// Runs `python -m ptk_engine rpc` with `program`.
    pub fn new(program: PathBuf, origin: EngineOrigin, engine_dir: Option<PathBuf>) -> Self {
        let mut env = vec![
            ("PYTHONUNBUFFERED".to_owned(), OsString::from("1")),
            ("PYTHONIOENCODING".to_owned(), OsString::from("utf-8")),
        ];
        if let Some(dir) = &engine_dir {
            let mut paths = vec![dir.clone()];
            if let Some(old) = env::var_os("PYTHONPATH") {
                paths.extend(env::split_paths(&old));
            }
            if let Ok(joined) = env::join_paths(paths) {
                env.push(("PYTHONPATH".to_owned(), joined));
            }
        }
        EngineCommand {
            program,
            args: vec!["-m".into(), "ptk_engine".into(), "rpc".into()],
            env,
            origin,
            engine_dir,
        }
    }
}

/// The engine source folder (the one containing `ptk_engine/`).
pub fn engine_source_dir() -> Option<PathBuf> {
    let has_pkg = |d: &Path| d.join("ptk_engine").join("__init__.py").is_file();
    if let Some(dir) = env::var_os("PTK_ENGINE_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir)).filter(|d| has_pkg(d));
    }
    let mut candidates = Vec::new();
    if let Ok(exe) = env::current_exe()
        && let Some(bin) = exe.parent()
    {
        candidates.push(bin.join("engine"));
        candidates.push(bin.join("../share/porting-toolkit/engine"));
    }
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../engine"));
    candidates
        .into_iter()
        .find(|d| has_pkg(d))
        .map(|d| d.canonicalize().unwrap_or(d))
}

/// Finds an executable on `PATH`.
pub fn which(name: &str) -> Option<PathBuf> {
    let exts: Vec<String> = if cfg!(windows) {
        vec![".exe".into(), ".cmd".into(), String::new()]
    } else {
        vec![String::new()]
    };
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        for ext in &exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Decides how to start the engine (see the module docs).
pub fn locate() -> Result<EngineCommand, EngineError> {
    let source = engine_source_dir();
    if let Some(py) = env::var_os("PTK_PYTHON").filter(|v| !v.is_empty()) {
        return Ok(EngineCommand::new(
            py.into(),
            EngineOrigin::EnvOverride,
            source,
        ));
    }
    let managed = paths::engine_env_python();
    if managed.is_file() {
        return Ok(EngineCommand::new(managed, EngineOrigin::Managed, source));
    }
    let Some(source) = source else {
        return Err(EngineError::NotFound(
            "no managed environment and no engine source folder (set PTK_ENGINE_DIR)".into(),
        ));
    };
    let python = which(if cfg!(windows) { "python" } else { "python3" })
        .ok_or_else(|| EngineError::NotFound("python3 is not on PATH".into()))?;
    Ok(EngineCommand::new(
        python,
        EngineOrigin::SystemPython,
        Some(source),
    ))
}

/// What `engine.info` returns.
#[derive(Clone, Debug, Deserialize)]
pub struct EngineInfo {
    pub name: String,
    pub version: String,
    pub protocol: u32,
    pub python: String,
    #[serde(default)]
    pub executable: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub engines: Vec<String>,
    #[serde(default)]
    pub methods: Vec<String>,
}

/// A running engine process.
pub struct EngineClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    pub info: EngineInfo,
    pub command: EngineCommand,
}

impl EngineClient {
    /// Locates and starts the engine.
    pub fn start() -> Result<Self, EngineError> {
        Self::spawn(locate()?)
    }

    /// Starts the engine and checks the protocol version.
    pub fn spawn(command: EngineCommand) -> Result<Self, EngineError> {
        let mut cmd = Command::new(&command.program);
        cmd.args(&command.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        for (k, v) in &command.env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().map_err(|source| EngineError::Spawn {
            program: command.program.display().to_string(),
            source,
        })?;
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        let mut client = EngineClient {
            child,
            stdin,
            stdout,
            next_id: 1,
            info: EngineInfo {
                name: String::new(),
                version: String::new(),
                protocol: 0,
                python: String::new(),
                executable: String::new(),
                categories: Vec::new(),
                engines: Vec::new(),
                methods: Vec::new(),
            },
            command,
        };
        let info: EngineInfo = client.call_as("engine.info", json!({}))?;
        if info.protocol != PROTOCOL_VERSION {
            return Err(EngineError::ProtocolMismatch {
                expected: PROTOCOL_VERSION,
                found: info.protocol,
            });
        }
        client.info = info;
        Ok(client)
    }

    /// Calls a method and returns its raw result.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, EngineError> {
        self.call_with(method, params, &mut |_, _| {})
    }

    /// Calls a method and deserializes its result.
    pub fn call_as<T: DeserializeOwned>(
        &mut self,
        method: &str,
        params: Value,
    ) -> Result<T, EngineError> {
        let value = self.call(method, params)?;
        serde_json::from_value(value)
            .map_err(|e| EngineError::Protocol(format!("unexpected result of {method}: {e}")))
    }

    /// Calls a method; notifications that arrive before the response are
    /// passed to `on_notification(method, params)`.
    pub fn call_with(
        &mut self,
        method: &str,
        params: Value,
        on_notification: &mut dyn FnMut(&str, &Value),
    ) -> Result<Value, EngineError> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        {
            let stdin = self
                .stdin
                .as_mut()
                .ok_or_else(|| EngineError::Protocol("engine input is closed".into()))?;
            let mut line = serde_json::to_vec(&request).expect("request serializes");
            line.push(b'\n');
            if stdin.write_all(&line).and_then(|_| stdin.flush()).is_err() {
                return Err(self.exited());
            }
        }
        let mut line = String::new();
        loop {
            line.clear();
            if self.stdout.read_line(&mut line)? == 0 {
                return Err(self.exited());
            }
            let text = line.trim();
            if text.is_empty() {
                continue;
            }
            let msg: Value = serde_json::from_str(text).map_err(|e| {
                let shown: String = text.chars().take(200).collect();
                EngineError::Protocol(format!("not JSON-RPC ({e}): {shown}"))
            })?;
            match msg.get("id") {
                None | Some(Value::Null) if msg.get("method").is_some() => {
                    let m = msg["method"].as_str().unwrap_or_default();
                    on_notification(m, msg.get("params").unwrap_or(&Value::Null));
                    continue;
                }
                Some(v) if v.as_u64() == Some(id) => {}
                other => {
                    return Err(EngineError::Protocol(format!(
                        "expected a response to request {id}, got id {other:?}"
                    )));
                }
            }
            if let Some(err) = msg.get("error") {
                return Err(EngineError::Rpc {
                    code: err
                        .get("code")
                        .and_then(Value::as_i64)
                        .unwrap_or(codes::INTERNAL_ERROR),
                    message: err
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown engine error")
                        .to_owned(),
                    data: err.get("data").cloned(),
                });
            }
            return msg.get("result").cloned().ok_or_else(|| {
                EngineError::Protocol("response has neither result nor error".into())
            });
        }
    }

    fn exited(&mut self) -> EngineError {
        let status = match self.child.try_wait() {
            Ok(Some(s)) => s.to_string(),
            _ => "closed its output".into(),
        };
        EngineError::Exited(status)
    }

    /// Asks the engine to exit and waits for it.
    pub fn shutdown(mut self) -> Result<(), EngineError> {
        let _ = self.call("shutdown", json!({}));
        self.stop();
        Ok(())
    }

    fn stop(&mut self) {
        // Closing stdin makes the engine's read loop end.
        self.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for EngineClient {
    fn drop(&mut self) {
        if self.stdin.is_some() {
            self.stop();
        }
    }
}
