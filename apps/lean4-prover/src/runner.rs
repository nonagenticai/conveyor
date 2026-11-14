//! Execute Lean 4 by subprocess: write the source into the pre-built lake
//! project, run `lake env lean <file>`, parse stderr into diagnostics.
//!
//! Three properties are load-bearing and were each a bug once:
//!
//! 1. **The child gets its own process group** (`setpgid` in `pre_exec`) and a
//!    timeout kills that WHOLE group. `lake env lean` is a wrapper: killing the
//!    `lake` pid alone leaves `lean` running and holding RAM on a 16 GiB host.
//! 2. **The child environment is replaced, not extended** — exactly `PATH` and
//!    `ELAN_HOME`. Elan resolves the toolchain from those; inheriting the
//!    server's own environment makes the subprocess depend on whatever the
//!    container happened to have.
//! 3. **`killpg` is only called on a real pid > 1.** `killpg(1)` is
//!    `kill(-1, SIGKILL)` — a broadcast that would kill the whole process tree,
//!    including the container's PID 1.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::config::Config;
use crate::metrics::Metrics;
use crate::models::{
    CheckRequest, CheckResponse, ElaborateRequest, ElaborateResponse, LeanDiagnostic, ProveRequest,
    ProveResponse, round_ms,
};

/// Tried in order when the caller names no tactics. Order is cheapest-and-most-
/// specific first; `aesop` is last because it is the slowest.
pub const DEFAULT_TACTICS: &[&str] = &[
    "norm_num", "simp", "ring", "linarith", "omega", "decide", "trivial", "exact?", "aesop",
];

#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
    pub elapsed_ms: f64,
}

#[derive(Debug, Clone)]
pub struct LeanRunner {
    cfg: Arc<Config>,
    metrics: Arc<Metrics>,
}

impl LeanRunner {
    pub fn new(cfg: Arc<Config>, metrics: Arc<Metrics>) -> Self {
        Self { cfg, metrics }
    }

    /// Run one Lean file. `requested` overrides the configured timeout, clamped
    /// to the hard ceiling.
    pub async fn run_lean(&self, source: &str, requested: Option<u64>) -> io::Result<RunOutcome> {
        let timeout_s = self.cfg.effective_timeout(requested);
        let started = Instant::now();

        let dir = Path::new(&self.cfg.project_dir);
        let path = temp_source_path(dir);
        tokio::fs::write(&path, source).await?;
        let _cleanup = RemoveOnDrop(path.clone());

        let mut cmd = Command::new(&self.cfg.lake_binary);
        cmd.arg("env")
            .arg(&self.cfg.lean_binary)
            .arg(&path)
            .current_dir(dir)
            // Replaced wholesale, not inherited — see the module note.
            .env_clear()
            .env("PATH", &self.cfg.subprocess_path)
            .env("ELAN_HOME", &self.cfg.elan_home)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        confine_to_process_group(&mut cmd, self.cfg.max_memory_mb);

        let mut child = cmd.spawn()?;
        let pid = child.id();
        let mut group = ProcessGroup(pid);

        let mut stdout = child.stdout.take().expect("stdout is piped");
        let mut stderr = child.stderr.take().expect("stderr is piped");
        let stdout_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            stdout.read_to_end(&mut buf).await.map(|_| buf)
        });
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            stderr.read_to_end(&mut buf).await.map(|_| buf)
        });

        let mut timed_out = false;
        let code = match tokio::time::timeout(Duration::from_secs(timeout_s), child.wait()).await {
            Ok(status) => status.map(|s| s.code().unwrap_or(1)).unwrap_or(1),
            Err(_) => {
                timed_out = true;
                kill_process_group(pid);
                let _ = child.wait().await;
                self.metrics
                    .lean_timeouts_total
                    .fetch_add(1, Ordering::Relaxed);
                1
            }
        };
        // The child is reaped, so its pid (and pgid) can be recycled: disarm
        // before anything else can drop us.
        group.disarm();

        // The pipes close when the child exits, so both readers finish here.
        let stdout = join_output(stdout_task).await;
        let stderr = join_output(stderr_task).await;
        if timed_out {
            return Ok(RunOutcome {
                code,
                stdout,
                stderr: format!("Lean timed out after {timeout_s}s"),
                elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
            });
        }
        Ok(RunOutcome {
            code,
            stdout,
            stderr,
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        })
    }

    pub async fn check(&self, req: &CheckRequest) -> io::Result<CheckResponse> {
        tracing::info!(source_len = req.source.len(), "checking lean source");
        let run = self.run_lean(&req.source, req.timeout).await?;
        let diagnostics = parse_diagnostics(&run.stderr);
        let has_errors = diagnostics.iter().any(|d| d.severity == "error");
        Ok(CheckResponse {
            valid: run.code == 0 && !has_errors,
            diagnostics,
            stdout: run.stdout,
            stderr: run.stderr,
            computation_time_ms: round_ms(run.elapsed_ms),
        })
    }

    pub async fn elaborate(&self, req: &ElaborateRequest) -> io::Result<ElaborateResponse> {
        let context = req.context.clone().unwrap_or_default();
        let source = format!("{context}\n#check ({})\n", req.term);
        tracing::info!(term_len = req.term.len(), "elaborating lean term");

        let run = self.run_lean(&source, req.timeout).await?;
        let diagnostics = parse_diagnostics(&run.stderr);
        let has_errors = diagnostics.iter().any(|d| d.severity == "error");

        let mut elaborated_type = run
            .stdout
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with("--"))
            .map(str::to_string);
        if elaborated_type.is_none() {
            elaborated_type = diagnostics
                .iter()
                .find(|d| d.severity == "information")
                .map(|d| d.message.clone());
        }

        Ok(ElaborateResponse {
            elaborated_type,
            valid: run.code == 0 && !has_errors,
            diagnostics,
            stdout: run.stdout,
            stderr: run.stderr,
            computation_time_ms: round_ms(run.elapsed_ms),
        })
    }

    pub async fn prove(&self, req: &ProveRequest) -> io::Result<ProveResponse> {
        let default_tactics: Vec<String> =
            DEFAULT_TACTICS.iter().map(|s| (*s).to_string()).collect();
        let tactics = req.tactics.clone().unwrap_or(default_tactics);
        let imports_block = match &req.imports {
            Some(imports) if !imports.is_empty() => {
                let mut block = imports
                    .iter()
                    .map(|i| format!("import {i}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                block.push_str("\n\n");
                block
            }
            _ => String::new(),
        };

        tracing::info!(
            theorem_len = req.theorem.len(),
            num_tactics = tactics.len(),
            "attempting proof"
        );
        let started = Instant::now();

        let mut all_diagnostics = Vec::new();
        let mut last_stdout = String::new();
        let mut last_stderr = String::new();

        for tactic in &tactics {
            let source = format!("{imports_block}{} := by {tactic}\n", req.theorem);
            let run = self.run_lean(&source, req.timeout).await?;

            let mut diagnostics = parse_diagnostics(&run.stderr);
            let has_errors = diagnostics.iter().any(|d| d.severity == "error");

            if run.code == 0 && !has_errors {
                tracing::info!(tactic = %tactic, "proof succeeded");
                return Ok(ProveResponse {
                    proven: true,
                    successful_tactic: Some(tactic.clone()),
                    diagnostics,
                    stdout: run.stdout,
                    stderr: run.stderr,
                    computation_time_ms: round_ms(started.elapsed().as_secs_f64() * 1000.0),
                });
            }

            for d in &mut diagnostics {
                d.message = format!("[{tactic}] {}", d.message);
            }
            all_diagnostics.extend(diagnostics);
            last_stdout = run.stdout;
            last_stderr = run.stderr;
        }

        tracing::info!("all tactics failed");
        Ok(ProveResponse {
            proven: false,
            successful_tactic: None,
            diagnostics: all_diagnostics,
            stdout: last_stdout,
            stderr: last_stderr,
            computation_time_ms: round_ms(started.elapsed().as_secs_f64() * 1000.0),
        })
    }

    /// Verify the Lean binary is callable. Bounded by the configured request
    /// budget but capped at 10 s: a healthy `lean --version` answers in well
    /// under a second, and a health probe must never hang on a wedged binary.
    pub async fn health_check(&self) -> (bool, String) {
        let probe_timeout = Duration::from_secs(self.cfg.timeout_s.min(10));
        let mut cmd = Command::new(&self.cfg.lean_binary);
        cmd.arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let fut = async {
            let out = cmd.output().await?;
            Ok::<_, io::Error>(out)
        };

        match tokio::time::timeout(probe_timeout, fut).await {
            Ok(Ok(out)) => {
                let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
                (true, version)
            }
            Ok(Err(e)) => (false, e.to_string()),
            Err(_) => (false, "lean --version timed out".to_string()),
        }
    }
}

async fn join_output(task: tokio::task::JoinHandle<io::Result<Vec<u8>>>) -> String {
    match task.await {
        Ok(Ok(bytes)) => String::from_utf8_lossy(&bytes).into_owned(),
        // A reader task that panicked or was cancelled yields no output; the
        // diagnostic fields stay empty rather than the request failing.
        _ => String::new(),
    }
}

fn temp_source_path(dir: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    dir.join(format!("_check_{}_{n}_{nanos}.lean", std::process::id()))
}

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct ProcessGroup(Option<u32>);

impl ProcessGroup {
    fn disarm(&mut self) {
        self.0 = None;
    }
}

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        // Covers request cancellation: the future is dropped, the child is not
        // waited on, and without this the Lean process outlives its slot.
        kill_process_group(self.0);
    }
}

fn kill_process_group(pid: Option<u32>) {
    #[cfg(unix)]
    if let Some(pid) = pid
        && pid > 1
    {
        // SAFETY: pgid == the child's pid because `pre_exec` called setpgid(0,0).
        unsafe {
            libc::killpg(pid as libc::pid_t, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
}

#[cfg(unix)]
fn confine_to_process_group(cmd: &mut Command, _max_memory_mb: u64) {
    // ⚠️ NO RLIMIT_AS. It was carried over from the Python service, where it was
    // a no-op: macOS ignores RLIMIT_AS, and the service only ever ran on darwin.
    // On Linux it is a hard cap on VIRTUAL ADDRESS SPACE, and Lean reserves far
    // more of that than it ever touches — so at the configured 4096 MB every
    // `lake env lean` died with
    //   "error: failed to read file '.../lib/lean/Std/Do/SPred/Notation.olean'"
    // (mmap of the olean fails; Lean reports it as an unreadable file). Reproduced
    // verbatim in the deployed pod on 2026-11-14 with `ulimit -v 4194304`, and it
    // compiles clean without the limit. RLIMIT_DATA is no better: it caps the
    // thread stacks too, and Lean then fails with "failed to create thread".
    //
    // The real bound is the container's memory limit (10 Gi in
    // clusters/strato/services/lean4-prover/deployment.yaml) together with the
    // admission lanes that cap concurrent proves. `max_memory_mb` stays in the
    // config and the ConfigMap for compatibility, but it is not enforced.
    // SAFETY: `pre_exec` runs in the forked child before exec. It only calls
    // async-signal-safe syscalls (setpgid) and allocates nothing.
    unsafe {
        cmd.pre_exec(move || {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn confine_to_process_group(_cmd: &mut Command, _max_memory_mb: u64) {}

/// Parse `file:line:col: severity: message` lines out of Lean's stderr.
///
/// This is the same grammar the Python service matched with
/// `^(?:.*?):(\d+):(\d+):\s*(error|warning|information):\s*(.*)$`, hand-written
/// so the service carries no regex engine. The non-greedy prefix means the
/// EARLIEST colon that starts a valid match wins, which is what the scan below
/// does.
pub fn parse_diagnostics(stderr: &str) -> Vec<LeanDiagnostic> {
    stderr.lines().filter_map(parse_diagnostic_line).collect()
}

fn parse_diagnostic_line(line: &str) -> Option<LeanDiagnostic> {
    for (idx, _) in line.char_indices().filter(|(_, c)| *c == ':') {
        if let Some(d) = parse_at(&line[idx..]) {
            return Some(d);
        }
    }
    None
}

fn parse_at(s: &str) -> Option<LeanDiagnostic> {
    let rest = s.strip_prefix(':')?;
    let (line_no, rest) = take_digits(rest)?;
    let rest = rest.strip_prefix(':')?;
    let (column, rest) = take_digits(rest)?;
    let rest = rest.strip_prefix(':')?;
    let rest = rest.trim_start();
    let (severity, rest) = take_severity(rest)?;
    let rest = rest.strip_prefix(':')?;
    let rest = rest.trim_start();
    Some(LeanDiagnostic {
        severity: severity.to_string(),
        line: Some(line_no),
        column: Some(column),
        message: rest.trim().to_string(),
    })
}

fn take_digits(s: &str) -> Option<(u32, &str)> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    s[..end].parse::<u32>().ok().map(|n| (n, &s[end..]))
}

fn take_severity(s: &str) -> Option<(&'static str, &str)> {
    for severity in ["error", "warning", "information"] {
        if let Some(rest) = s.strip_prefix(severity) {
            return Some((severity, rest));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression for the 2026-11-14 in-cluster failure. `confine_to_process_group`
    /// used to set `RLIMIT_AS` to the configured memory budget; on Linux that caps
    /// virtual address space, and Lean reserves several gigabytes of it before it
    /// does any work — every prove died with a bogus "failed to read file ...
    /// Notation.olean". macOS ignores RLIMIT_AS, which is why the Python original
    /// never showed it. The child must see an unlimited address space.
    #[tokio::test]
    async fn confinement_does_not_cap_the_address_space() {
        // Compare against the parent's own limit rather than asserting
        // "unlimited": the CI runner pod may carry a limit of its own, and the
        // property under test is that CONFINEMENT does not change it.
        async fn read(confine: bool) -> Option<String> {
            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg("ulimit -v");
            if confine {
                confine_to_process_group(&mut cmd, 4096);
            }
            let out = cmd.output().await.ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        }

        let (with, without) = (read(true).await, read(false).await);
        if cfg!(target_os = "linux") {
            assert!(
                with.is_some() && without.is_some(),
                "sh -c 'ulimit -v' must work on linux"
            );
        }
        assert_eq!(
            with, without,
            "confine_to_process_group must not set RLIMIT_AS"
        );
    }

    #[test]
    fn parses_an_error_diagnostic() {
        let diags = parse_diagnostics("file.lean:5:10: error: type mismatch");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, "error");
        assert_eq!(diags[0].line, Some(5));
        assert_eq!(diags[0].column, Some(10));
        assert_eq!(diags[0].message, "type mismatch");
    }

    #[test]
    fn parses_three_severities_in_order() {
        let stderr = "a.lean:1:2: error: one\nb.lean:3:4: warning: two\nc:5:6: information: three";
        let diags = parse_diagnostics(stderr);
        assert_eq!(diags.len(), 3);
        assert_eq!(diags[0].severity, "error");
        assert_eq!(diags[1].severity, "warning");
        assert_eq!(diags[2].severity, "information");
    }

    #[test]
    fn ignores_lines_without_a_diagnostic() {
        assert!(parse_diagnostics("").is_empty());
        assert!(parse_diagnostics("some ordinary output\nanother line").is_empty());
    }

    #[test]
    fn tolerates_whitespace_around_the_severity() {
        let diags = parse_diagnostics("x.lean:1:1:   warning:   spaced  ");
        assert_eq!(diags[0].severity, "warning");
        assert_eq!(diags[0].message, "spaced");
    }

    #[test]
    fn never_kills_pid_1_or_pid_0() {
        // A regression here is a container-wide SIGKILL broadcast.
        kill_process_group(Some(1));
        kill_process_group(Some(0));
        kill_process_group(None);
    }
}
