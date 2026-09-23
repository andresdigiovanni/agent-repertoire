use crate::models::RunOutcome;
use crate::{RepError, Result};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const MAX_STREAM_BYTES: usize = 1024 * 1024;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

pub struct ExecutionRequest {
    pub interpreter: String,
    pub script: PathBuf,
    pub stdin_json: serde_json::Value,
    pub timeout: Duration,
}

fn read_capped(mut reader: impl Read) -> std::io::Result<(String, bool)> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut truncated = false;
    loop {
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        let remaining = MAX_STREAM_BYTES.saturating_sub(buf.len());
        if n > remaining {
            truncated = true;
        }
        buf.extend_from_slice(&chunk[..n.min(remaining)]);
    }
    Ok((String::from_utf8_lossy(&buf).into_owned(), truncated))
}

pub fn execute(req: ExecutionRequest) -> Result<RunOutcome> {
    let start = Instant::now();
    let mut child = Command::new(&req.interpreter)
        .arg(&req.script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| RepError::Spawn(format!("{}: {}", req.interpreter, e)))?;

    let payload = req.stdin_json.to_string();
    let writer = child.stdin.take().map(|mut w| {
        std::thread::spawn(move || {
            let _ = w.write_all(payload.as_bytes());
        })
    });
    let stdout_reader = child
        .stdout
        .take()
        .map(|p| std::thread::spawn(move || read_capped(p)));
    let stderr_reader = child
        .stderr
        .take()
        .map(|p| std::thread::spawn(move || read_capped(p)));

    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if start.elapsed() >= req.timeout {
            timed_out = true;
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    if let Some(handle) = writer {
        let _ = handle.join();
    }
    let (stdout, stdout_truncated) = match stdout_reader {
        Some(h) => h.join().unwrap_or_else(|_| Ok((String::new(), false)))?,
        None => (String::new(), false),
    };
    let (stderr, stderr_truncated) = match stderr_reader {
        Some(h) => h.join().unwrap_or_else(|_| Ok((String::new(), false)))?,
        None => (String::new(), false),
    };

    Ok(RunOutcome {
        exit_code: status.and_then(|s| s.code()),
        timed_out,
        stdout,
        stderr,
        duration_ms: start.elapsed().as_millis() as u64,
        truncated: stdout_truncated || stderr_truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    fn req(script_body: &str, stdin: serde_json::Value, timeout: Duration) -> ExecutionRequest {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("run.sh");
        std::fs::write(&script, script_body).unwrap();
        std::mem::forget(dir);
        ExecutionRequest {
            interpreter: "bash".into(),
            script,
            stdin_json: stdin,
            timeout,
        }
    }

    #[test]
    fn captures_stdout_and_exit_code() {
        let out = execute(req("echo hello", json!({}), Duration::from_secs(10))).unwrap();
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(out.stdout.trim(), "hello");
        assert!(!out.timed_out);
        assert!(!out.truncated);
    }

    #[test]
    fn receives_json_on_stdin() {
        let out = execute(req("cat", json!({"x": 1}), Duration::from_secs(10))).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["x"], 1);
    }

    #[test]
    fn nonzero_exit_captured() {
        let out = execute(req("echo oops >&2; exit 3", json!({}), Duration::from_secs(10))).unwrap();
        assert_eq!(out.exit_code, Some(3));
        assert_eq!(out.stderr.trim(), "oops");
    }

    #[test]
    fn timeout_kills_process() {
        let out = execute(req("exec sleep 30", json!({}), Duration::from_millis(300))).unwrap();
        assert!(out.timed_out);
        assert_eq!(out.exit_code, None);
        assert!(out.duration_ms < 5000);
    }

    #[test]
    fn large_output_truncated() {
        let out = execute(
            req("head -c 2097152 /dev/zero | tr '\\0' 'a'", json!({}), Duration::from_secs(30)),
        )
        .unwrap();
        assert!(out.truncated);
        assert_eq!(out.stdout.len(), MAX_STREAM_BYTES);
    }

    #[test]
    fn missing_interpreter_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("run.sh");
        std::fs::write(&script, "echo hi").unwrap();
        let out = execute(ExecutionRequest {
            interpreter: "definitely-not-a-real-interpreter-xyz".into(),
            script,
            stdin_json: json!({}),
            timeout: Duration::from_secs(5),
        });
        assert!(out.is_err());
    }
}
