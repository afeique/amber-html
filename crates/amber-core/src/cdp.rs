//! The real CDP transport over Chromium's `--remote-debugging-pipe`.
//!
//! This is the lowest layer of the browser stack (Plans.md). It spawns a
//! pinned Chromium with the debug **pipe** (no open debugging port, no
//! WebSocket — see the security rationale in §13) and exchanges raw,
//! NUL-delimited CDP JSON over two file descriptors inherited by the child:
//!
//! * **fd 3** — the browser *reads* the commands we *write*.
//! * **fd 4** — the browser *writes* responses/events that we *read*.
//!
//! High-level operations (navigate, settle, `captureSnapshot`,
//! `captureScreenshot`, `printToPDF`, …) are NOT implemented here; they are
//! built on top of [`PipeCdp::send`] / [`PipeCdp::events`] by a separate layer.
//!
//! ## Concurrency model (no tokio)
//!
//! A single background thread owns the read end (fd 4), parses the byte stream
//! into complete `\0`-terminated frames via [`FrameReader`], and routes each
//! decoded message:
//!
//! * a message with an `id` → the [`mpsc::Sender`] registered by the matching
//!   in-flight [`send`](PipeCdp::send) caller (correlation via
//!   `Mutex<HashMap<u64, Sender>>`);
//! * a message with a `method` and no `id` → the events channel.
//!
//! Callers of [`send`](PipeCdp::send) block on their own `mpsc::Receiver` with a
//! timeout. The write end (fd 3) is guarded by its own `Mutex` so concurrent
//! sends serialize their frames without interleaving bytes.
//!
//! **Platform:** Unix only for now (fd inheritance via `command-fds`); the
//! Windows branch is a clearly-marked `unimplemented!`.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{json, Value};

/// How long a [`PipeCdp::send`] call waits for its matching response before
/// giving up with [`CdpError::Timeout`].
const DEFAULT_SEND_TIMEOUT: Duration = Duration::from_secs(30);

/// Buffer size for each read from the fd-4 pipe.
const READ_CHUNK: usize = 64 * 1024;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors produced by the CDP pipe transport.
///
/// This type is local to this module; the integration layer maps it onto the
/// crate-wide `crate::error::Error` (see the module docs / final report).
#[derive(Debug, thiserror::Error)]
pub enum CdpError {
    /// Failed to spawn the Chromium child process.
    #[error("failed to spawn Chromium: {0}")]
    Spawn(#[source] std::io::Error),

    /// An I/O error on one of the pipes (read/write/setup).
    #[error("CDP pipe I/O error: {0}")]
    Io(#[source] std::io::Error),

    /// The browser returned a CDP `error` object for a command.
    #[error("CDP protocol error {code}: {message}")]
    Protocol {
        /// CDP error code (e.g. `-32000`).
        code: i64,
        /// Human-readable CDP error message.
        message: String,
    },

    /// A response carried a payload we could not parse as expected.
    #[error("malformed CDP message: {0}")]
    Malformed(String),

    /// No response arrived within the timeout.
    #[error("timed out waiting for CDP response to id {0}")]
    Timeout(u64),

    /// The pipe (and thus the connection) was closed — usually the browser
    /// exited or the reader thread died.
    #[error("CDP connection closed")]
    ConnectionClosed,
}

// ---------------------------------------------------------------------------
// FrameReader — the NUL-framing codec (the testable core)
// ---------------------------------------------------------------------------

/// Splits an incoming byte stream into `\0`-terminated frames.
///
/// CDP-over-pipe terminates each JSON message with a single NUL byte. The bytes
/// arriving from the pipe do not respect message boundaries: one read may
/// contain several messages, a fraction of a message, or a message split across
/// reads. `FrameReader` is a pure accumulator that absorbs arbitrary byte
/// chunks via [`push`](FrameReader::push) and yields each complete frame (with
/// the trailing NUL stripped) as it becomes available. It performs no I/O and
/// has no knowledge of pipes or JSON, which makes it exhaustively unit-testable.
#[derive(Debug, Default)]
pub struct FrameReader {
    /// Bytes accumulated since the last NUL terminator.
    buf: Vec<u8>,
}

impl FrameReader {
    /// Create an empty reader.
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Feed a chunk of bytes, returning every complete frame the chunk
    /// completed. Each returned `Vec<u8>` is the message payload **without** its
    /// trailing NUL. A frame split across several `push` calls is buffered until
    /// its terminator arrives; a trailing partial frame stays buffered.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for &byte in chunk {
            if byte == 0 {
                // End of a frame: hand off the accumulated bytes and reset.
                out.push(std::mem::take(&mut self.buf));
            } else {
                self.buf.push(byte);
            }
        }
        out
    }

    /// Number of bytes currently buffered for the in-progress (incomplete)
    /// frame. Used by tests to assert there is no dangling partial frame.
    pub fn pending_len(&self) -> usize {
        self.buf.len()
    }
}

// ---------------------------------------------------------------------------
// Correlation — pure id allocation + routing decision (testable core)
// ---------------------------------------------------------------------------

/// Classification of an inbound CDP message, derived purely from its JSON shape.
///
/// Factored out so the routing decision can be unit-tested without a thread,
/// pipe, or browser.
#[derive(Debug, PartialEq, Eq)]
enum Routed {
    /// A command response correlated by its `id`.
    Response(u64),
    /// An unsolicited event (`method` present, no `id`).
    Event,
    /// Neither a usable response nor an event (ignored).
    Ignored,
}

/// Decide where a decoded message goes, without performing any side effect.
fn classify(msg: &Value) -> Routed {
    match msg.get("id").and_then(Value::as_u64) {
        Some(id) => Routed::Response(id),
        None if msg.get("method").is_some() => Routed::Event,
        None => Routed::Ignored,
    }
}

/// Monotonic command-id allocator (CDP ids must be unique per connection).
#[derive(Debug)]
struct IdAllocator {
    next: AtomicU64,
}

impl IdAllocator {
    fn new() -> Self {
        // CDP ids are arbitrary u64s; start at 1 so 0 is never a valid id.
        Self {
            next: AtomicU64::new(1),
        }
    }

    fn alloc(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Shared routing state (held by both the public handle and the reader thread)
// ---------------------------------------------------------------------------

/// Outcome delivered to a waiting `send` caller for a given command id.
type ResponseResult = Result<Value, CdpError>;

/// State shared between [`PipeCdp`] and its background reader thread.
struct Shared {
    /// id → one-shot sender for the caller currently awaiting that id.
    pending: Mutex<HashMap<u64, Sender<ResponseResult>>>,
    /// Sink for CDP events (`method` messages with no `id`).
    event_tx: Sender<Value>,
    /// Set once the reader thread observes EOF/error so new sends fail fast.
    closed: Mutex<bool>,
}

impl Shared {
    /// Mark the connection closed and fail every still-pending caller.
    fn mark_closed(&self) {
        if let Ok(mut closed) = self.closed.lock() {
            *closed = true;
        }
        if let Ok(mut pending) = self.pending.lock() {
            for (_, tx) in pending.drain() {
                let _ = tx.send(Err(CdpError::ConnectionClosed));
            }
        }
    }

    fn is_closed(&self) -> bool {
        self.closed.lock().map(|g| *g).unwrap_or(true)
    }
}

// ---------------------------------------------------------------------------
// PipeCdp — the transport
// ---------------------------------------------------------------------------

/// Hand-rolled CDP client over Chromium's debug pipe — the only transport.
///
/// Owns the spawned Chromium child, the write end of fd 3, the correlation
/// state, and the join handle of the background reader thread. Dropping it kills
/// the child (see [`Drop`]).
pub struct PipeCdp {
    /// The spawned Chromium process.
    child: Child,
    /// Write end of fd-3 (browser-reads); guarded so concurrent sends do not
    /// interleave bytes. `&PipeWriter: Write`, so we never need `&mut`.
    writer: Mutex<os_pipe::PipeWriter>,
    /// Monotonic command-id source.
    ids: IdAllocator,
    /// Shared routing state.
    shared: Arc<Shared>,
    /// Receiver for CDP events; taken out via [`PipeCdp::events`].
    event_rx: Mutex<Option<Receiver<Value>>>,
    /// Background reader thread handle (joined on drop, best-effort).
    reader: Option<JoinHandle<()>>,
    /// Per-call response timeout.
    timeout: Duration,
    /// Temp user-data-dir, removed on drop.
    user_data_dir: std::path::PathBuf,
}

impl PipeCdp {
    /// Spawn a pinned Chromium with `--remote-debugging-pipe` and connect over
    /// the inherited pipe file descriptors.
    ///
    /// Launches:
    /// ```text
    /// <chromium> --headless=new --remote-debugging-pipe --no-first-run \
    ///            --no-default-browser-check --user-data-dir=<temp> [extra_args...]
    /// ```
    /// mapping one OS pipe to the child's **fd 3** (browser reads our commands)
    /// and another to **fd 4** (browser writes responses/events). A background
    /// thread starts reading fd 4 immediately.
    ///
    /// Unix only; the Windows path is `unimplemented!`.
    pub fn spawn(chromium: &Path, extra_args: &[String]) -> Result<PipeCdp, CdpError> {
        Self::spawn_with_timeout(chromium, extra_args, DEFAULT_SEND_TIMEOUT)
    }

    /// Like [`spawn`](PipeCdp::spawn) but with an explicit per-command timeout
    /// (handy for tests / callers that want a tighter bound).
    pub fn spawn_with_timeout(
        chromium: &Path,
        extra_args: &[String],
        timeout: Duration,
    ) -> Result<PipeCdp, CdpError> {
        // Two OS pipes:
        //   cmd:  we WRITE -> browser READS  (becomes child fd 3)
        //   resp: browser WRITES -> we READ  (becomes child fd 4)
        let (cmd_read, cmd_write) = os_pipe::pipe().map_err(CdpError::Io)?;
        let (resp_read, resp_write) = os_pipe::pipe().map_err(CdpError::Io)?;

        // Unique temp user-data-dir so concurrent instances never collide.
        let user_data_dir = std::env::temp_dir().join(format!(
            "amber-html-cdp-{}-{}",
            std::process::id(),
            next_instance_seq()
        ));
        std::fs::create_dir_all(&user_data_dir).map_err(CdpError::Io)?;

        let mut cmd = Command::new(chromium);
        cmd.arg("--headless=new")
            .arg("--remote-debugging-pipe")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg(format!("--user-data-dir={}", user_data_dir.display()))
            .args(extra_args)
            // We never use the child's stdio for protocol traffic; keep stdin
            // closed and let stdout/stderr go to the parent's (Chromium logs to
            // stderr). The pipe lives entirely on fd 3/4.
            .stdin(Stdio::null());

        Self::configure_pipe_fds(&mut cmd, cmd_read, resp_write)?;

        let child = cmd.spawn().map_err(CdpError::Spawn)?;
        // The child now owns the inherited copies of cmd_read/resp_write; our
        // originals were consumed by `configure_pipe_fds`. We keep cmd_write
        // (to send) and resp_read (to receive).

        let (event_tx, event_rx) = mpsc::channel::<Value>();
        let shared = Arc::new(Shared {
            pending: Mutex::new(HashMap::new()),
            event_tx,
            closed: Mutex::new(false),
        });

        // Background reader thread: owns resp_read, parses frames, routes them.
        let reader_shared = Arc::clone(&shared);
        let reader = std::thread::Builder::new()
            .name("amber-cdp-reader".into())
            .spawn(move || reader_loop(resp_read, reader_shared))
            .map_err(CdpError::Io)?;

        Ok(PipeCdp {
            child,
            writer: Mutex::new(cmd_write),
            ids: IdAllocator::new(),
            shared,
            event_rx: Mutex::new(Some(event_rx)),
            reader: Some(reader),
            timeout,
            user_data_dir,
        })
    }

    /// Map our pipe ends onto the child's fd 3 (cmd read) and fd 4 (resp write).
    #[cfg(unix)]
    fn configure_pipe_fds(
        cmd: &mut Command,
        cmd_read: os_pipe::PipeReader,
        resp_write: os_pipe::PipeWriter,
    ) -> Result<(), CdpError> {
        use command_fds::{CommandFdExt, FdMapping};
        use std::os::fd::OwnedFd;

        // os_pipe ends convert directly into OwnedFd (From<PipeReader/Writer>),
        // which is exactly what FdMapping.parent_fd wants. command-fds holds the
        // OwnedFds open until after fork/exec and dup2's them to the requested
        // child fd numbers, so we don't hand-roll pre_exec/dup2 ourselves.
        let mappings = vec![
            FdMapping {
                parent_fd: OwnedFd::from(cmd_read),
                child_fd: 3,
            },
            FdMapping {
                parent_fd: OwnedFd::from(resp_write),
                child_fd: 4,
            },
        ];
        cmd.fd_mappings(mappings)
            .map_err(|e| CdpError::Io(std::io::Error::other(e.to_string())))?;
        Ok(())
    }

    #[cfg(not(unix))]
    fn configure_pipe_fds(
        _cmd: &mut Command,
        _cmd_read: os_pipe::PipeReader,
        _resp_write: os_pipe::PipeWriter,
    ) -> Result<(), CdpError> {
        // Windows uses inherited HANDLEs (lpReserved2 / STARTUPINFO) rather than
        // numbered fds; Chromium's pipe transport reads HANDLEs passed at
        // positions 3/4. This requires a Windows-specific spawn path that is not
        // yet implemented (Unix first — see Plans.md).
        unimplemented!("CDP debug pipe on Windows: fd 3/4 HANDLE inheritance not yet implemented");
    }

    /// Send a CDP command and block until its response arrives.
    ///
    /// Assigns the next id, writes `{"id":N,"method":..,"params":..}` + `\0` to
    /// fd 3, then waits (up to the configured timeout) for the reader thread to
    /// deliver the matching response, returning its `result` object (or `{}` if
    /// the response had none). A CDP `error` object becomes
    /// [`CdpError::Protocol`].
    ///
    /// Uses `&self` (interior mutability) so the higher layer can share one
    /// transport and issue sends without exclusive borrows.
    pub fn send(&self, method: &str, params: Value) -> Result<Value, CdpError> {
        if self.shared.is_closed() {
            return Err(CdpError::ConnectionClosed);
        }

        let id = self.ids.alloc();
        let (tx, rx) = mpsc::channel::<ResponseResult>();

        // Register before writing so a fast response can never race ahead of us.
        {
            let mut pending = self
                .shared
                .pending
                .lock()
                .map_err(|_| CdpError::ConnectionClosed)?;
            pending.insert(id, tx);
        }

        let frame = encode_command(id, method, &params);
        if let Err(e) = self.write_frame(&frame) {
            // Drop our registration so we don't leak it.
            if let Ok(mut pending) = self.shared.pending.lock() {
                pending.remove(&id);
            }
            return Err(e);
        }

        match rx.recv_timeout(self.timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => {
                // Reclaim the slot; a late response will simply be ignored.
                if let Ok(mut pending) = self.shared.pending.lock() {
                    pending.remove(&id);
                }
                Err(CdpError::Timeout(id))
            }
            Err(RecvTimeoutError::Disconnected) => Err(CdpError::ConnectionClosed),
        }
    }

    /// Take the receiver for CDP events (`method` messages with no `id`).
    ///
    /// Returns the single events `Receiver`; subsequent calls return `None`
    /// (there is exactly one event stream per connection). The higher layer's
    /// settle engine consumes lifecycle/Network events from here.
    pub fn events(&self) -> Option<Receiver<Value>> {
        self.event_rx.lock().ok().and_then(|mut g| g.take())
    }

    /// Write one NUL-terminated frame to fd 3 under the writer lock.
    fn write_frame(&self, frame: &[u8]) -> Result<(), CdpError> {
        let mut w = self
            .writer
            .lock()
            .map_err(|_| CdpError::ConnectionClosed)?;
        w.write_all(frame).map_err(CdpError::Io)?;
        w.flush().map_err(CdpError::Io)?;
        Ok(())
    }
}

/// Encode a CDP command as a NUL-terminated JSON frame.
fn encode_command(id: u64, method: &str, params: &Value) -> Vec<u8> {
    // `params` is always sent (Chromium tolerates `{}`); omit nothing fancy.
    let msg = json!({ "id": id, "method": method, "params": params });
    let mut bytes = serde_json::to_vec(&msg).expect("serializing a CDP command cannot fail");
    bytes.push(0);
    bytes
}

/// Background reader: pull bytes from fd 4, frame them, route each message.
fn reader_loop(mut resp_read: os_pipe::PipeReader, shared: Arc<Shared>) {
    let mut framer = FrameReader::new();
    let mut chunk = vec![0u8; READ_CHUNK];

    loop {
        let n = match resp_read.read(&mut chunk) {
            Ok(0) => break, // EOF: browser closed fd 4.
            Ok(n) => n,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break, // I/O error: treat as connection closed.
        };

        for frame in framer.push(&chunk[..n]) {
            dispatch_frame(&frame, &shared);
        }
    }

    shared.mark_closed();
}

/// Parse one raw frame and deliver it to the right destination.
fn dispatch_frame(frame: &[u8], shared: &Shared) {
    // Chromium may emit an empty frame between messages; ignore it.
    if frame.is_empty() {
        return;
    }
    let msg: Value = match serde_json::from_slice(frame) {
        Ok(v) => v,
        // A malformed frame is non-fatal: skip it and keep the stream alive.
        Err(_) => return,
    };

    match classify(&msg) {
        Routed::Response(id) => {
            let sender = shared
                .pending
                .lock()
                .ok()
                .and_then(|mut pending| pending.remove(&id));
            if let Some(tx) = sender {
                let _ = tx.send(interpret_response(msg));
            }
            // No waiter (e.g. timed-out caller): drop silently.
        }
        Routed::Event => {
            let _ = shared.event_tx.send(msg);
        }
        Routed::Ignored => {}
    }
}

/// Turn a correlated response object into a `result` value or a protocol error.
fn interpret_response(mut msg: Value) -> ResponseResult {
    if let Some(err) = msg.get("error") {
        let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
        let message = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown CDP error")
            .to_string();
        return Err(CdpError::Protocol { code, message });
    }
    // Successful responses carry `result`; default to an empty object so
    // result-less commands (e.g. some enable/disable calls) succeed cleanly.
    match msg.get_mut("result") {
        Some(result) => Ok(result.take()),
        None => Ok(json!({})),
    }
}

/// Process-local monotonic counter to keep temp dir names unique.
fn next_instance_seq() -> u64 {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    SEQ.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Trait bridge + Drop
// ---------------------------------------------------------------------------

impl crate::browser::CdpTransport for PipeCdp {
    fn send(&mut self, method: &str, params: Value) -> crate::error::Result<Value> {
        // Delegate to the inherent `&self` method; map the transport error onto
        // the crate-wide error type. The integration layer may replace this
        // mapping with a richer variant.
        PipeCdp::send(self, method, params)
            .map_err(|e| crate::error::Error::Fetch(e.to_string()))
    }
}

impl Drop for PipeCdp {
    fn drop(&mut self) {
        // Killing the child closes its fd-4 write end, which lets the reader
        // thread observe EOF and exit on its own.
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(handle) = self.reader.take() {
            let _ = handle.join();
        }
        // Best-effort cleanup of the throwaway profile directory.
        let _ = std::fs::remove_dir_all(&self.user_data_dir);
    }
}

// ---------------------------------------------------------------------------
// Tests — the framing codec + pure correlation logic (no browser, no pipe)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn frames_as_strings(framer: &mut FrameReader, chunk: &[u8]) -> Vec<String> {
        framer
            .push(chunk)
            .into_iter()
            .map(|f| String::from_utf8(f).unwrap())
            .collect()
    }

    #[test]
    fn frame_single_complete_message() {
        let mut fr = FrameReader::new();
        let out = frames_as_strings(&mut fr, b"{\"id\":1}\0");
        assert_eq!(out, vec!["{\"id\":1}".to_string()]);
        assert_eq!(fr.pending_len(), 0);
    }

    #[test]
    fn frame_multiple_messages_in_one_chunk() {
        let mut fr = FrameReader::new();
        let out = frames_as_strings(&mut fr, b"aaa\0bbb\0ccc\0");
        assert_eq!(out, vec!["aaa", "bbb", "ccc"]);
        assert_eq!(fr.pending_len(), 0);
    }

    #[test]
    fn frame_split_across_chunk_boundaries() {
        let mut fr = FrameReader::new();
        // A single message arrives in three pieces, terminator in the last.
        assert!(frames_as_strings(&mut fr, b"{\"meth").is_empty());
        assert!(frames_as_strings(&mut fr, b"od\":\"x").is_empty());
        let out = frames_as_strings(&mut fr, b"\"}\0");
        assert_eq!(out, vec!["{\"method\":\"x\"}"]);
        assert_eq!(fr.pending_len(), 0);
    }

    #[test]
    fn frame_terminator_split_from_payload() {
        let mut fr = FrameReader::new();
        // Payload in one chunk, lone NUL in the next.
        assert!(frames_as_strings(&mut fr, b"payload").is_empty());
        assert_eq!(fr.pending_len(), 7);
        let out = frames_as_strings(&mut fr, b"\0");
        assert_eq!(out, vec!["payload"]);
        assert_eq!(fr.pending_len(), 0);
    }

    #[test]
    fn frame_empty_input_yields_nothing() {
        let mut fr = FrameReader::new();
        assert!(fr.push(b"").is_empty());
        assert_eq!(fr.pending_len(), 0);
    }

    #[test]
    fn frame_trailing_partial_stays_buffered() {
        let mut fr = FrameReader::new();
        let out = frames_as_strings(&mut fr, b"done\0half-");
        assert_eq!(out, vec!["done"]);
        // "half-" is buffered, awaiting its terminator.
        assert_eq!(fr.pending_len(), 5);
        // It completes on the next chunk.
        let out2 = frames_as_strings(&mut fr, b"way\0");
        assert_eq!(out2, vec!["half-way"]);
        assert_eq!(fr.pending_len(), 0);
    }

    #[test]
    fn frame_consecutive_nuls_produce_empty_frames() {
        let mut fr = FrameReader::new();
        let out = frames_as_strings(&mut fr, b"a\0\0b\0");
        // Middle empty frame is preserved by the codec (dispatch ignores it).
        assert_eq!(out, vec!["a", "", "b"]);
    }

    #[test]
    fn frame_one_byte_at_a_time() {
        let mut fr = FrameReader::new();
        let mut collected: Vec<String> = Vec::new();
        for &b in b"hi\0yo\0" {
            collected.extend(frames_as_strings(&mut fr, &[b]));
        }
        assert_eq!(collected, vec!["hi", "yo"]);
        assert_eq!(fr.pending_len(), 0);
    }

    // --- correlation / classification -------------------------------------

    #[test]
    fn classify_response_by_id() {
        let v = json!({"id": 7, "result": {"ok": true}});
        assert_eq!(classify(&v), Routed::Response(7));
    }

    #[test]
    fn classify_error_response_is_still_a_response() {
        let v = json!({"id": 9, "error": {"code": -32000, "message": "boom"}});
        assert_eq!(classify(&v), Routed::Response(9));
    }

    #[test]
    fn classify_event_by_method_without_id() {
        let v = json!({"method": "Page.loadEventFired", "params": {}});
        assert_eq!(classify(&v), Routed::Event);
    }

    #[test]
    fn classify_ignores_garbage() {
        let v = json!({"params": {"x": 1}});
        assert_eq!(classify(&v), Routed::Ignored);
    }

    #[test]
    fn id_allocator_is_monotonic_and_starts_at_one() {
        let ids = IdAllocator::new();
        assert_eq!(ids.alloc(), 1);
        assert_eq!(ids.alloc(), 2);
        assert_eq!(ids.alloc(), 3);
    }

    // --- encode -----------------------------------------------------------

    #[test]
    fn encode_command_appends_nul_and_is_valid_json() {
        let frame = encode_command(42, "Page.navigate", &json!({"url": "https://e.com"}));
        assert_eq!(*frame.last().unwrap(), 0u8, "frame must end in NUL");
        let parsed: Value = serde_json::from_slice(&frame[..frame.len() - 1]).unwrap();
        assert_eq!(parsed["id"], 42);
        assert_eq!(parsed["method"], "Page.navigate");
        assert_eq!(parsed["params"]["url"], "https://e.com");
    }

    #[test]
    fn encode_then_frame_roundtrips() {
        // The encoder's output, fed through the decoder, yields exactly one
        // frame that parses back to the same logical message.
        let frame = encode_command(1, "Target.getTargets", &json!({}));
        let mut fr = FrameReader::new();
        let out = fr.push(&frame);
        assert_eq!(out.len(), 1);
        let parsed: Value = serde_json::from_slice(&out[0]).unwrap();
        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["method"], "Target.getTargets");
    }

    // --- response interpretation -----------------------------------------

    #[test]
    fn interpret_success_extracts_result() {
        let r = interpret_response(json!({"id": 1, "result": {"frameId": "abc"}}));
        assert_eq!(r.unwrap(), json!({"frameId": "abc"}));
    }

    #[test]
    fn interpret_missing_result_is_empty_object() {
        let r = interpret_response(json!({"id": 1}));
        assert_eq!(r.unwrap(), json!({}));
    }

    #[test]
    fn interpret_error_maps_to_protocol() {
        let r = interpret_response(json!({
            "id": 1,
            "error": {"code": -32601, "message": "method not found"}
        }));
        match r {
            Err(CdpError::Protocol { code, message }) => {
                assert_eq!(code, -32601);
                assert_eq!(message, "method not found");
            }
            other => panic!("expected Protocol error, got {other:?}"),
        }
    }

    #[test]
    fn interpret_error_tolerates_missing_fields() {
        let r = interpret_response(json!({"id": 1, "error": {}}));
        match r {
            Err(CdpError::Protocol { code, message }) => {
                assert_eq!(code, 0);
                assert_eq!(message, "unknown CDP error");
            }
            other => panic!("expected Protocol error, got {other:?}"),
        }
    }

    // --- dispatch routing (exercises the real shared state, no I/O) --------

    #[test]
    fn dispatch_routes_response_to_waiter() {
        let (event_tx, _event_rx) = mpsc::channel();
        let shared = Shared {
            pending: Mutex::new(HashMap::new()),
            event_tx,
            closed: Mutex::new(false),
        };
        let (tx, rx) = mpsc::channel::<ResponseResult>();
        shared.pending.lock().unwrap().insert(5, tx);

        let frame = b"{\"id\":5,\"result\":{\"v\":1}}";
        dispatch_frame(frame, &shared);

        let got = rx.recv_timeout(Duration::from_secs(1)).unwrap().unwrap();
        assert_eq!(got, json!({"v": 1}));
        // The slot is consumed.
        assert!(shared.pending.lock().unwrap().is_empty());
    }

    #[test]
    fn dispatch_routes_event_to_event_channel() {
        let (event_tx, event_rx) = mpsc::channel();
        let shared = Shared {
            pending: Mutex::new(HashMap::new()),
            event_tx,
            closed: Mutex::new(false),
        };

        let frame = b"{\"method\":\"Network.requestWillBeSent\",\"params\":{\"x\":2}}";
        dispatch_frame(frame, &shared);

        let ev = event_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(ev["method"], "Network.requestWillBeSent");
        assert_eq!(ev["params"]["x"], 2);
    }

    #[test]
    fn dispatch_ignores_malformed_and_unmatched() {
        let (event_tx, event_rx) = mpsc::channel();
        let shared = Shared {
            pending: Mutex::new(HashMap::new()),
            event_tx,
            closed: Mutex::new(false),
        };
        // Not valid JSON: silently dropped.
        dispatch_frame(b"{not json", &shared);
        // Valid response but nobody is waiting: silently dropped.
        dispatch_frame(b"{\"id\":99,\"result\":{}}", &shared);
        // Empty frame: dropped.
        dispatch_frame(b"", &shared);
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn mark_closed_fails_pending_callers() {
        let (event_tx, _event_rx) = mpsc::channel();
        let shared = Shared {
            pending: Mutex::new(HashMap::new()),
            event_tx,
            closed: Mutex::new(false),
        };
        let (tx, rx) = mpsc::channel::<ResponseResult>();
        shared.pending.lock().unwrap().insert(1, tx);

        shared.mark_closed();

        assert!(shared.is_closed());
        match rx.recv_timeout(Duration::from_secs(1)).unwrap() {
            Err(CdpError::ConnectionClosed) => {}
            other => panic!("expected ConnectionClosed, got {other:?}"),
        }
    }

    // --- real-browser smoke test (requires a local Chromium) --------------

    #[test]
    #[ignore = "requires a real Chromium binary; run with AMBER_CHROMIUM_PATH set"]
    fn smoke_spawn_and_get_version() {
        let path = std::env::var("AMBER_CHROMIUM_PATH")
            .expect("set AMBER_CHROMIUM_PATH to the Chromium binary");
        let cdp = PipeCdp::spawn(Path::new(&path), &[]).expect("spawn");
        let result = cdp
            .send("Browser.getVersion", json!({}))
            .expect("Browser.getVersion");
        assert!(
            result.get("product").is_some(),
            "expected a product field, got {result}"
        );
    }
}
