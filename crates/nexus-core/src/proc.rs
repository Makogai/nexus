//! Running short-lived helper processes (git, `claude --version`, `claude auth status`).
//!
//! Always an argument vector, never a shell string (threat T1). No console window flashes on Windows.
//! Output is size-capped and the process is killed when it exceeds its timeout.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::{CoreError, Result};

const MAX_OUTPUT: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Output {
    pub status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Output {
    pub fn success(&self) -> bool {
        self.status == Some(0)
    }
    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
    pub fn stderr_str(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// A `Command` with platform defaults applied (no console window on Windows).
pub fn command(program: impl AsRef<Path>) -> Command {
    let mut cmd = Command::new(program.as_ref());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Run a prepared command to completion with a timeout, capturing (capped) stdout/stderr.
pub fn run(mut cmd: Command, timeout: Duration) -> Result<Output> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| CoreError::External {
        program: program.clone(),
        message: format!("failed to start: {e}"),
    })?;

    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let out_t = std::thread::spawn(move || read_capped(&mut stdout));
    let err_t = std::thread::spawn(move || read_capped(&mut stderr));

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CoreError::External {
                    program,
                    message: format!("timed out after {} s", timeout.as_secs()),
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(e) => {
                return Err(CoreError::External {
                    program,
                    message: format!("wait failed: {e}"),
                });
            }
        }
    };
    let stdout = out_t.join().unwrap_or_default();
    let stderr = err_t.join().unwrap_or_default();
    Ok(Output { status, stdout, stderr })
}

fn read_capped(r: &mut impl Read) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match r.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if buf.len() < MAX_OUTPUT {
                    let take = n.min(MAX_OUTPUT - buf.len());
                    buf.extend_from_slice(&chunk[..take]);
                }
            }
        }
    }
    buf
}
