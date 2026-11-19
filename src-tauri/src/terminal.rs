// PAIGE Desktop - scoped native terminal / process execution authority (Increment 1C).
//
// This module is the security authority for local subprocess execution. It is
// intentionally designed so the same process-spawn/stdio/kill machinery can be
// reused later for native MCP `stdio` server connections: a single managed
// process engine, not a duplicate one.
//
// Scope for Increment 1C:
// - Allowlisted development commands only.
// - Execution confined to authorized project roots (currently the lab directory).
// - Risk-based authority: read/standard-write run automatically; privileged or
//   materially destructive actions require explicit owner approval.
// - Output streaming, stdin write, process kill, and verifiable receipts.
// - No shell injection, no unrestricted PATH execution, no global system commands.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

// --- shared helpers with broker ---
#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptGenRandom(
        h_algorithm: *mut std::ffi::c_void,
        p_buffer: *mut u8,
        cb_buffer: u32,
        dw_flags: u32,
    ) -> i32;
}
const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x00000002;

fn os_random_bytes(buf: &mut [u8]) -> Result<(), String> {
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status != 0 {
        Err(format!("BCryptGenRandom failed: 0x{status:08x}"))
    } else {
        Ok(())
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn display_path(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s
    }
}

// Compile-time authorized project roots. A command may only run inside one of
// these roots (or a subdirectory). An agent cannot extend this list at runtime.
const ALLOWED_ROOTS: &[&str] = &[r"C:\Users\tonig\paige-desktop-lab"];

const APPROVAL_TTL_SECS: u64 = 300;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TerminalRisk {
    Read,
    StandardWrite,
    Privileged,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalExecuteRequest {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Clone, Serialize)]
pub struct TerminalSession {
    pub session_id: String,
    pub pid: u32,
    pub command_line: String,
    pub cwd: String,
    pub status: String,
    pub started_at: u64,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalExecuteResponse {
    Session(TerminalSession),
    ApprovalRequired {
        token: String,
        reason: String,
        command_line: String,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TerminalInputRequest {
    pub session_id: String,
    pub input: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TerminalOutputRequest {
    pub session_id: String,
}

#[derive(Clone, Serialize)]
pub struct TerminalReceipt {
    pub session_id: String,
    pub command_line: String,
    pub cwd: String,
    pub exit_code: i32,
    pub started_at: u64,
    pub finished_at: u64,
    pub stdout_lines: usize,
    pub stderr_lines: usize,
    pub status: String,
}

#[derive(Clone, Serialize)]
pub struct TerminalOutputEvent {
    pub session_id: String,
    pub stream: String,
    pub line: String,
    pub timestamp: u64,
}

#[derive(Clone, Serialize)]
pub struct TerminalExitEvent {
    pub session_id: String,
    pub exit_code: i32,
    pub finished_at: u64,
    pub status: String,
}

// ---------------------------------------------------------------------------
// Command allowlist and risk classification
// ---------------------------------------------------------------------------

struct CommandProfile {
    name: &'static str,
    risk: TerminalRisk,
}

const ALLOWED_COMMANDS: &[CommandProfile] = &[
    CommandProfile { name: "cargo", risk: TerminalRisk::StandardWrite },
    CommandProfile { name: "node", risk: TerminalRisk::Read },
    CommandProfile { name: "npm", risk: TerminalRisk::StandardWrite },
    CommandProfile { name: "npx", risk: TerminalRisk::StandardWrite },
    CommandProfile { name: "pnpm", risk: TerminalRisk::StandardWrite },
    CommandProfile { name: "yarn", risk: TerminalRisk::StandardWrite },
    CommandProfile { name: "python", risk: TerminalRisk::Read },
    CommandProfile { name: "python3", risk: TerminalRisk::Read },
    CommandProfile { name: "pip", risk: TerminalRisk::StandardWrite },
    CommandProfile { name: "git", risk: TerminalRisk::StandardWrite },
    CommandProfile { name: "rustc", risk: TerminalRisk::Read },
    CommandProfile { name: "tsc", risk: TerminalRisk::Read },
    CommandProfile { name: "eslint", risk: TerminalRisk::Read },
    CommandProfile { name: "prettier", risk: TerminalRisk::StandardWrite },
    CommandProfile { name: "vitest", risk: TerminalRisk::Read },
    CommandProfile { name: "jest", risk: TerminalRisk::Read },
    CommandProfile { name: "mocha", risk: TerminalRisk::Read },
    CommandProfile { name: "cls", risk: TerminalRisk::Read },
    CommandProfile { name: "echo", risk: TerminalRisk::Read },
    CommandProfile { name: "dir", risk: TerminalRisk::Read },
];

const PRIVILEGED_NAMES: &[&str] = &[
    "del", "erase", "rd", "rmdir", "format", "diskpart", "reg", "net", "sc",
    "powercfg", "shutdown", "taskkill", "msiexec", "sdelete", "cipher", "takeown",
    "icacls", "cacls", "attrib", "fsutil", "vssadmin", "wbadmin", "dism", "sfc",
    "chkdsk", "defrag", "compact", "expand", "makecab", "extrac32", "robocopy",
    "xcopy", "move", "rename", "ren", "mklink", "cmd", "powershell", "pwsh",
    "bash", "sh", "zsh", "wsl", "curl", "wget", "certutil",
];

fn classify_command(name: &str, args: &[String]) -> TerminalRisk {
    let lower = name.to_lowercase();

    // Explicitly privileged system tooling is never auto-approved.
    if PRIVILEGED_NAMES.contains(&lower.as_str()) {
        return TerminalRisk::Privileged;
    }

    // Some subcommands of otherwise-allowed tools are materially destructive.
    if lower == "git" {
        if args.iter().any(|a| a == "clean" || a == "reset") {
            return TerminalRisk::Privileged;
        }
    }

    if let Some(profile) = ALLOWED_COMMANDS.iter().find(|p| p.name.eq_ignore_ascii_case(&lower)) {
        profile.risk
    } else {
        // Unknown commands are treated as privileged until explicitly allowed.
        TerminalRisk::Privileged
    }
}

fn command_description(risk: TerminalRisk, command_line: &str) -> String {
    match risk {
        TerminalRisk::Read => format!("Read-only command: {command_line}"),
        TerminalRisk::StandardWrite => format!("Standard write command: {command_line}"),
        TerminalRisk::Privileged => format!("Privileged/destructive command requires approval: {command_line}"),
    }
}

// Resolve a command name to an absolute executable path using PATH/PATHEXT.
// Rejects explicit relative/absolute paths that contain path separators.
fn resolve_executable(name: &str) -> Result<PathBuf, String> {
    if name.is_empty() {
        return Err("empty command".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err("command paths are not allowed; use a command name only".to_string());
    }

    let path_var = std::env::var("PATH").map_err(|_| "PATH environment variable is not set".to_string())?;
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    let exts: Vec<&str> = pathext.split(';').filter(|e| !e.is_empty()).collect();

    let base = name.to_lowercase();
    for dir in path_var.split(';') {
        if dir.is_empty() {
            continue;
        }
        let d = PathBuf::from(dir);
        // First try exact name (e.g. already has extension).
        let exact = d.join(name);
        if exact.is_file() {
            return Ok(exact);
        }
        for ext in &exts {
            let candidate = d.join(format!("{base}{ext}"));
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    Err(format!("command '{name}' not found in PATH"))
}

// ---------------------------------------------------------------------------
// Root / cwd validation
// ---------------------------------------------------------------------------

fn validate_cwd(cwd: &str, allowed_roots: &[PathBuf]) -> Result<PathBuf, String> {
    let p = PathBuf::from(cwd);
    if !p.is_absolute() {
        return Err("working directory must be absolute".to_string());
    }
    let canon = std::fs::canonicalize(&p)
        .map_err(|e| format!("working directory is inaccessible: {e}"))?;
    if !canon.is_dir() {
        return Err("working directory is not a directory".to_string());
    }
    let allowed = allowed_roots.iter().any(|r| canon == *r || canon.starts_with(r));
    if !allowed {
        return Err("working directory is outside authorized project roots".to_string());
    }
    Ok(canon)
}

// ---------------------------------------------------------------------------
// Process management
// ---------------------------------------------------------------------------

enum ControlMsg {
    Input(String),
    Kill,
}

enum SessionStatus {
    Running,
    Exited(i32),
    Killed,
}

struct ManagedSession {
    pid: u32,
    command_line: String,
    cwd: PathBuf,
    started_at: u64,
    status: SessionStatus,
    input_tx: mpsc::Sender<ControlMsg>,
    output: Vec<TerminalOutputEvent>,
}

pub struct TerminalState {
    next_id: u64,
    sessions: HashMap<String, ManagedSession>,
    receipts: Vec<TerminalReceipt>,
    pending: HashMap<String, PendingExecution>,
    allowed_roots: Vec<PathBuf>,
}

struct PendingExecution {
    request: TerminalExecuteRequest,
    resolved_exe: PathBuf,
    risk: TerminalRisk,
    command_line: String,
    created_at: u64,
}

impl TerminalState {
    pub fn new() -> Self {
        let mut allowed = Vec::new();
        for r in ALLOWED_ROOTS {
            if let Ok(c) = std::fs::canonicalize(r) {
                allowed.push(c);
            }
        }
        Self {
            next_id: 1,
            sessions: HashMap::new(),
            receipts: Vec::new(),
            pending: HashMap::new(),
            allowed_roots: allowed,
        }
    }
}

fn format_command_line(command: &str, args: &[String]) -> String {
    let mut parts = vec![command.to_string()];
    parts.extend(args.iter().cloned());
    // Simple, safe display; not shell-quoted.
    parts.join(" ")
}

fn spawn_and_manage(
    app: AppHandle,
    state: &Mutex<TerminalState>,
    request: TerminalExecuteRequest,
    resolved_exe: PathBuf,
    command_line: String,
    _risk: TerminalRisk,
) -> Result<TerminalSession, String> {
    let cwd = request
        .cwd
        .as_deref()
        .map(|c| validate_cwd(c, &state.lock().unwrap().allowed_roots))
        .transpose()?
        .unwrap_or_else(|| state.lock().unwrap().allowed_roots[0].clone());

    let mut cmd = Command::new(&resolved_exe);
    cmd.args(&request.args)
        .current_dir(&cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped());

    if !request.env.is_empty() {
        cmd.envs(&request.env);
    }

    let mut child = cmd.spawn().map_err(|e| format!("failed to spawn process: {e}"))?;
    let pid = child.id();
    let stdout = child.stdout.take().ok_or("failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("failed to capture stderr")?;
    let stdin = child.stdin.take().ok_or("failed to capture stdin")?;

    let session_id = {
        let mut s = state.lock().unwrap();
        let id = format!("term-{}", s.next_id);
        s.next_id += 1;
        id
    };

    let started_at = now_secs();
    let sid_out = session_id.clone();
    let sid_err = session_id.clone();
    let sid_exit = session_id.clone();
    let command_line_out = command_line.clone();
    let cwd_out = cwd.clone();

    // stdout reader
    let app_stdout = app.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(line) = line {
                let event = TerminalOutputEvent {
                    session_id: sid_out.clone(),
                    stream: "stdout".to_string(),
                    line,
                    timestamp: now_secs(),
                };
                let _ = app_stdout.emit("terminal-output", &event);
                if let Some(ts) = app_stdout.try_state::<Mutex<TerminalState>>() {
                    let mut s = ts.lock().unwrap();
                    if let Some(sess) = s.sessions.get_mut(&sid_out) {
                        if sess.output.len() < 10_000 {
                            sess.output.push(event);
                        }
                    }
                }
            }
        }
    });

    // stderr reader
    let app_stderr = app.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(line) = line {
                let event = TerminalOutputEvent {
                    session_id: sid_err.clone(),
                    stream: "stderr".to_string(),
                    line,
                    timestamp: now_secs(),
                };
                let _ = app_stderr.emit("terminal-output", &event);
                if let Some(ts) = app_stderr.try_state::<Mutex<TerminalState>>() {
                    let mut s = ts.lock().unwrap();
                    if let Some(sess) = s.sessions.get_mut(&sid_err) {
                        if sess.output.len() < 10_000 {
                            sess.output.push(event);
                        }
                    }
                }
            }
        }
    });

    // process manager / waiter / input / kill
    let (ctrl_tx, ctrl_rx) = mpsc::channel::<ControlMsg>();
    {
        let mut s = state.lock().unwrap();
        s.sessions.insert(
            session_id.clone(),
            ManagedSession {
                pid,
                command_line: command_line.clone(),
                cwd: cwd.clone(),
                started_at,
                status: SessionStatus::Running,
                input_tx: ctrl_tx,
                output: Vec::new(),
            },
        );
    }

    let app_manager = app.clone();
    let _state_manager = state;
    thread::spawn(move || {
        let mut stdin = stdin;
        let mut killed = false;
        let exit_code = loop {
            match ctrl_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(ControlMsg::Input(text)) => {
                    let _ = stdin.write_all(text.as_bytes());
                    let _ = stdin.write_all(b"\n");
                    let _ = stdin.flush();
                }
                Ok(ControlMsg::Kill) => {
                    killed = true;
                    let _ = kill_process_tree(pid);
                    // After asking Windows to kill the tree, wait for the child.
                    let _ = child.wait();
                    break -1;
                }
                Err(RecvTimeoutError::Timeout) => {
                    if let Ok(Some(status)) = child.try_wait() {
                        break status.code().unwrap_or(-1);
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    break child.wait().map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
                }
            }
        };

        let finished_at = now_secs();
        let status_str = if killed { "killed".to_string() } else { "exited".to_string() };

        let _ = app_manager.emit(
            "terminal-exit",
            &TerminalExitEvent {
                session_id: sid_exit.clone(),
                exit_code,
                finished_at,
                status: status_str.clone(),
            },
        );

        // Update state and store receipt.
        if let Some(ts) = app_manager.try_state::<Mutex<TerminalState>>() {
            let mut s = ts.lock().unwrap();
            if let Some(sess) = s.sessions.get_mut(&sid_exit) {
                sess.status = if killed {
                    SessionStatus::Killed
                } else {
                    SessionStatus::Exited(exit_code)
                };
            }
            s.receipts.push(TerminalReceipt {
                session_id: sid_exit.clone(),
                command_line: command_line_out.clone(),
                cwd: display_path(&cwd_out),
                exit_code,
                started_at,
                finished_at,
                stdout_lines: 0, // Approximate; frontend can count.
                stderr_lines: 0,
                status: status_str,
            });
        }
    });

    Ok(TerminalSession {
        session_id,
        pid,
        command_line,
        cwd: display_path(&cwd),
        status: "running".to_string(),
        started_at,
    })
}

fn kill_process_tree(pid: u32) -> Result<(), String> {
    let status = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .status()
        .map_err(|e| format!("failed to run taskkill: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("taskkill exited with {status}"))
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn terminal_execute(
    app: AppHandle,
    state: tauri::State<Mutex<TerminalState>>,
    request: TerminalExecuteRequest,
) -> Result<TerminalExecuteResponse, String> {
    let command = request.command.trim().to_string();
    let risk = classify_command(&command, &request.args);
    let resolved_exe = resolve_executable(&command)?;
    let command_line = format_command_line(&command, &request.args);

    // Privileged/destructive commands require explicit owner approval.
    if risk == TerminalRisk::Privileged {
        let mut raw = [0u8; 32];
        os_random_bytes(&mut raw).map_err(|e| format!("secure RNG failed: {e}"))?;
        let token = hex::encode(raw);
        let reason = command_description(risk, &command_line);
        {
            let mut s = state.lock().unwrap();
            s.pending.insert(
                token.clone(),
                PendingExecution {
                    request,
                    resolved_exe,
                    risk,
                    command_line: command_line.clone(),
                    created_at: now_secs(),
                },
            );
        }
        return Ok(TerminalExecuteResponse::ApprovalRequired {
            token,
            reason,
            command_line,
        });
    }

    let session = spawn_and_manage(app, state.inner(), request, resolved_exe, command_line, risk)?;
    Ok(TerminalExecuteResponse::Session(session))
}

#[tauri::command]
pub fn terminal_execute_approved(
    app: AppHandle,
    state: tauri::State<Mutex<TerminalState>>,
    token: String,
) -> Result<TerminalExecuteResponse, String> {
    let pending = {
        let mut s = state.lock().unwrap();
        let p = s.pending.remove(&token).ok_or("unknown or expired approval token")?;
        if now_secs().saturating_sub(p.created_at) > APPROVAL_TTL_SECS {
            return Err("approval token expired".to_string());
        }
        p
    };

    let session = spawn_and_manage(
        app,
        state.inner(),
        pending.request,
        pending.resolved_exe,
        pending.command_line,
        pending.risk,
    )?;
    Ok(TerminalExecuteResponse::Session(session))
}

#[tauri::command]
pub fn terminal_input(
    state: tauri::State<Mutex<TerminalState>>,
    request: TerminalInputRequest,
) -> Result<(), String> {
    let s = state.lock().unwrap();
    let sess = s
        .sessions
        .get(&request.session_id)
        .ok_or("unknown session")?;
    match sess.status {
        SessionStatus::Running => {
            sess.input_tx
                .send(ControlMsg::Input(request.input))
                .map_err(|_| "session input channel closed".to_string())?;
            Ok(())
        }
        _ => Err("session is not running".to_string()),
    }
}

#[tauri::command]
pub fn terminal_kill(state: tauri::State<Mutex<TerminalState>>, session_id: String) -> Result<(), String> {
    let s = state.lock().unwrap();
    let sess = s.sessions.get(&session_id).ok_or("unknown session")?;
    match sess.status {
        SessionStatus::Running => {
            sess.input_tx
                .send(ControlMsg::Kill)
                .map_err(|_| "session control channel closed".to_string())?;
            Ok(())
        }
        _ => Err("session is not running".to_string()),
    }
}

#[tauri::command]
pub fn terminal_list(state: tauri::State<Mutex<TerminalState>>) -> Vec<TerminalSession> {
    let s = state.lock().unwrap();
    s.sessions
        .iter()
        .map(|(id, sess)| TerminalSession {
            session_id: id.clone(),
            pid: sess.pid,
            command_line: sess.command_line.clone(),
            cwd: display_path(&sess.cwd),
            status: match sess.status {
                SessionStatus::Running => "running".to_string(),
                SessionStatus::Exited(c) => format!("exited:{c}"),
                SessionStatus::Killed => "killed".to_string(),
            },
            started_at: sess.started_at,
        })
        .collect()
}

#[tauri::command]
pub fn terminal_receipts(state: tauri::State<Mutex<TerminalState>>) -> Vec<TerminalReceipt> {
    state.lock().unwrap().receipts.clone()
}

#[tauri::command]
pub fn terminal_output(
    state: tauri::State<Mutex<TerminalState>>,
    request: TerminalOutputRequest,
) -> Vec<TerminalOutputEvent> {
    let s = state.lock().unwrap();
    s.sessions
        .get(&request.session_id)
        .map(|sess| sess.output.clone())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Unit tests for pure logic
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_known_commands() {
        assert_eq!(classify_command("cargo", &[]), TerminalRisk::StandardWrite);
        assert_eq!(classify_command("node", &[]), TerminalRisk::Read);
        assert_eq!(classify_command("dir", &[]), TerminalRisk::Read);
    }

    #[test]
    fn classify_privileged_names() {
        assert_eq!(classify_command("del", &[]), TerminalRisk::Privileged);
        assert_eq!(classify_command("powershell", &[]), TerminalRisk::Privileged);
        assert_eq!(classify_command("cmd", &[]), TerminalRisk::Privileged);
    }

    #[test]
    fn classify_git_destructive_subcommands() {
        assert_eq!(classify_command("git", &["clean".into(), "-fd".into()]), TerminalRisk::Privileged);
        assert_eq!(classify_command("git", &["status".into()]), TerminalRisk::StandardWrite);
    }

    #[test]
    fn classify_unknown_as_privileged() {
        assert_eq!(classify_command("unknown-tool", &[]), TerminalRisk::Privileged);
    }

    #[test]
    fn resolve_finds_common_commands() {
        // cargo should be on PATH in a normal dev environment.
        let r = resolve_executable("cargo");
        assert!(r.is_ok(), "cargo should resolve on PATH: {r:?}");
        let p = r.unwrap();
        assert!(p.is_file());
    }

    #[test]
    fn resolve_rejects_paths() {
        assert!(resolve_executable(r"C:\Windows\System32\cmd.exe").is_err());
        assert!(resolve_executable("..\\evil").is_err());
    }

    #[test]
    fn validate_cwd_inside_root() {
        let root = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let allowed = vec![root.clone()];
        let sub = root.join("paige-terminal-test-sub");
        let _ = std::fs::create_dir_all(&sub);
        let res = validate_cwd(&sub.to_string_lossy().to_string(), &allowed);
        assert!(res.is_ok(), "{res:?}");
        assert!(res.unwrap().starts_with(&root));
    }

    #[test]
    fn validate_cwd_rejects_outside_root() {
        let root = std::env::temp_dir();
        let allowed = vec![root.clone()];
        let res = validate_cwd(r"C:\Windows", &allowed);
        assert!(res.is_err());
    }
}
