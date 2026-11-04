//! PTY manager (ADR-0007): spawns processes in pseudo-terminals, streams raw output to the webview over
//! binary channels, and keeps a bounded ring buffer per terminal so views can re-attach.
//!
//! Only processes spawned here are ever signalled or killed — by handle, never by PID or name (threat T16).

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tauri::ipc::{Channel, InvokeResponseBody};

/// Default scrollback kept for re-attach.
pub const RING_CAPACITY: usize = 512 * 1024;
const READ_CHUNK: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct SpawnRequest {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// Complete environment; the inherited environment is cleared first.
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
}

struct OutputState {
    ring: VecDeque<u8>,
    capacity: usize,
    subscriber: Option<Channel<InvokeResponseBody>>,
}

impl OutputState {
    fn push(&mut self, bytes: &[u8]) {
        if bytes.len() >= self.capacity {
            self.ring.clear();
            self.ring.extend(&bytes[bytes.len() - self.capacity..]);
            return;
        }
        let overflow = (self.ring.len() + bytes.len()).saturating_sub(self.capacity);
        self.ring.drain(..overflow);
        self.ring.extend(bytes);
    }
}

struct Terminal {
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    output: Mutex<OutputState>,
    running: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Default)]
pub struct PtyManager {
    terminals: Mutex<HashMap<String, Arc<Terminal>>>,
}

impl PtyManager {
    fn get(&self, id: &str) -> Result<Arc<Terminal>, String> {
        lock(&self.terminals)
            .get(id)
            .cloned()
            .ok_or_else(|| "this terminal is no longer attached to NEXUS".to_string())
    }

    /// Spawn `req` in a new PTY registered under `id`. `on_exit` runs once with the exit code after the
    /// process has ended and its final output has been drained. Returns the OS process id when known.
    pub fn spawn(
        &self,
        id: &str,
        req: SpawnRequest,
        on_exit: impl FnOnce(Option<i64>) + Send + 'static,
    ) -> Result<Option<u32>, String> {
        let size = PtySize {
            rows: req.rows.clamp(5, 500),
            cols: req.cols.clamp(20, 1000),
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = native_pty_system().openpty(size).map_err(|e| format!("cannot open a terminal: {e}"))?;

        let mut cmd = CommandBuilder::new(&req.program);
        cmd.args(&req.args);
        cmd.cwd(&req.cwd);
        cmd.env_clear();
        for (k, v) in &req.env {
            cmd.env(k, v);
        }
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("cannot start {}: {e}", req.program.display()))?;
        drop(pair.slave);

        let pid = child.process_id();
        let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let term = Arc::new(Terminal {
            writer: Mutex::new(Some(writer)),
            killer: Mutex::new(child.clone_killer()),
            master: Mutex::new(Some(pair.master)),
            output: Mutex::new(OutputState {
                ring: VecDeque::new(),
                capacity: RING_CAPACITY,
                subscriber: None,
            }),
            running: AtomicBool::new(true),
        });
        lock(&self.terminals).insert(id.to_string(), term.clone());

        let reader_term = term.clone();
        let reader_thread = std::thread::Builder::new()
            .name(format!("pty-read-{id}"))
            .spawn(move || read_loop(reader, &reader_term))
            .map_err(|e| e.to_string())?;

        let waiter_term = term;
        std::thread::Builder::new()
            .name(format!("pty-wait-{id}"))
            .spawn(move || {
                let code = child.wait().ok().map(|s| i64::from(s.exit_code()));
                drop(child);
                waiter_term.running.store(false, Ordering::SeqCst);
                // Give the PTY a moment to flush, then close it so the reader sees EOF (ConPTY keeps the
                // pipe open until the pseudo console itself is closed).
                std::thread::sleep(Duration::from_millis(150));
                lock(&waiter_term.writer).take();
                lock(&waiter_term.master).take();
                let _ = reader_thread.join();
                on_exit(code);
            })
            .map_err(|e| e.to_string())?;
        Ok(pid)
    }

    /// Subscribe a channel to a terminal's output. Replays the ring buffer first; the replay and the
    /// subscription happen under one lock, so no output is lost or duplicated.
    pub fn attach(&self, id: &str, channel: Channel<InvokeResponseBody>) -> Result<bool, String> {
        let term = self.get(id)?;
        let mut out = lock(&term.output);
        if !out.ring.is_empty() {
            let (a, b) = out.ring.as_slices();
            let mut replay = Vec::with_capacity(a.len() + b.len());
            replay.extend_from_slice(a);
            replay.extend_from_slice(b);
            // The terminal must not answer queries a second time: its replies would be typed into the program.
            let replay = strip_terminal_queries(&replay);
            channel.send(InvokeResponseBody::Raw(replay)).map_err(|e| e.to_string())?;
        }
        out.subscriber = Some(channel);
        Ok(term.running.load(Ordering::SeqCst))
    }

    pub fn write(&self, id: &str, bytes: &[u8]) -> Result<(), String> {
        let term = self.get(id)?;
        let mut w = lock(&term.writer);
        let writer = w.as_mut().ok_or_else(|| "the process has exited".to_string())?;
        writer.write_all(bytes).and_then(|_| writer.flush()).map_err(|e| e.to_string())
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let term = self.get(id)?;
        let master = lock(&term.master);
        if let Some(m) = master.as_ref() {
            m.resize(PtySize {
                rows: rows.clamp(5, 500),
                cols: cols.clamp(20, 1000),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Ctrl-C to the foreground program (non-destructive).
    pub fn interrupt(&self, id: &str) -> Result<(), String> {
        self.write(id, b"\x03")
    }

    /// Terminate the process NEXUS spawned (explicit user confirmation happens in the UI).
    pub fn terminate(&self, id: &str) -> Result<(), String> {
        let term = self.get(id)?;
        if !term.running.load(Ordering::SeqCst) {
            return Ok(());
        }
        match lock(&term.killer).kill() {
            Ok(()) => Ok(()),
            // portable-pty 0.9 on Windows inverts TerminateProcess's result: a successful kill is reported as
            // Err("operation completed successfully", os error 0). Anything else is a real failure.
            Err(e) if e.raw_os_error() == Some(0) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn is_running(&self, id: &str) -> bool {
        lock(&self.terminals)
            .get(id)
            .is_some_and(|t| t.running.load(Ordering::SeqCst))
    }

    /// Copy of the buffered output (tests, "copy all output").
    pub fn snapshot(&self, id: &str) -> Vec<u8> {
        self.get(id)
            .map(|t| lock(&t.output).ring.iter().copied().collect())
            .unwrap_or_default()
    }

    /// A terminal (running or exited but not yet released) exists for `id`.
    pub fn is_attached(&self, id: &str) -> bool {
        lock(&self.terminals).contains_key(id)
    }

    pub fn running_ids(&self) -> Vec<String> {
        lock(&self.terminals)
            .iter()
            .filter(|(_, t)| t.running.load(Ordering::SeqCst))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Forget an exited terminal (frees its buffer). Running terminals are kept.
    pub fn release(&self, id: &str) -> bool {
        let mut map = lock(&self.terminals);
        match map.get(id) {
            Some(t) if !t.running.load(Ordering::SeqCst) => {
                map.remove(id);
                true
            }
            _ => false,
        }
    }

    /// Kill every running child (application exit).
    pub fn terminate_all(&self) {
        for id in self.running_ids() {
            let _ = self.terminate(&id);
        }
    }
}

/// ConPTY (created with cursor inheritance) opens every session by asking the terminal for the cursor
/// position and blocks the child until it gets an answer. A fresh NEXUS terminal is always at 1;1, so the
/// PTY manager answers immediately — the child never waits for a view to attach — and the query is kept
/// out of the stream so the attached terminal does not answer it again.
const CURSOR_QUERY: &[u8] = b"[6n";

fn read_loop(mut reader: Box<dyn Read + Send>, term: &Terminal) {
    let mut buf = vec![0u8; READ_CHUNK];
    let mut handshake_pending = cfg!(windows);
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let mut chunk = &buf[..n];
                let owned;
                if handshake_pending {
                    handshake_pending = false;
                    if let Some(pos) = find(chunk, CURSOR_QUERY) {
                        if let Some(w) = lock(&term.writer).as_mut() {
                            let _ = w.write_all(b"[1;1R").and_then(|_| w.flush());
                        }
                        owned = [&chunk[..pos], &chunk[pos + CURSOR_QUERY.len()..]].concat();
                        chunk = &owned;
                    }
                }
                if chunk.is_empty() {
                    continue;
                }
                let mut out = lock(&term.output);
                out.push(chunk);
                let failed = out
                    .subscriber
                    .as_ref()
                    .is_some_and(|ch| ch.send(InvokeResponseBody::Raw(chunk.to_vec())).is_err());
                if failed {
                    out.subscriber = None;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Remove terminal *queries* (device status, cursor position, device attributes) from replayed output.
fn strip_terminal_queries(bytes: &[u8]) -> Vec<u8> {
    const QUERIES: &[&[u8]] = &[b"[6n", b"[?6n", b"[5n", b"[c", b"[0c", b"[>c", b"[>0c"];
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    'outer: while i < bytes.len() {
        if bytes[i] == 0x1b {
            for q in QUERIES {
                if bytes[i..].starts_with(q) {
                    i += q.len();
                    continue 'outer;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_strips_terminal_queries_only() {
        let raw = b"[6nhello [31mred[0m[c world[>0c";
        assert_eq!(strip_terminal_queries(raw), b"hello [31mred[0m world".to_vec());
    }

    #[test]
    fn ring_buffer_keeps_most_recent_bytes() {
        let mut o = OutputState {
            ring: VecDeque::new(),
            capacity: 8,
            subscriber: None,
        };
        o.push(b"abcdef");
        o.push(b"ghij");
        assert_eq!(o.ring.iter().copied().collect::<Vec<_>>(), b"cdefghij");
        o.push(b"0123456789");
        assert_eq!(o.ring.iter().copied().collect::<Vec<_>>(), b"23456789");
    }
}
