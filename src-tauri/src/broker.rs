// PAIGE Desktop - Rust native file authority broker (Increment 1B).
//
// This module is the security authority for local file access. The frontend
// and any future model/agent interact with it only through the typed Tauri
// commands below. Nothing here grants a general-purpose filesystem bridge.
//
// Scope for Increment 1B: a single owner-approved root (the lab directory),
// read and write on regular text files inside it, session-scoped grants, and
// explicit owner approval for every write. No terminal, no git, no inference.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

// Compile-time allowlist of roots the owner may grant. A future model or
// agent cannot extend this list at runtime: grant_root() rejects anything that
// is not one of these roots (or a subdirectory of one).
const ALLOWED_ROOTS: &[&str] = &[r"C:\Users\tonig\paige-desktop-lab"];

const MAX_FILE_BYTES: usize = 64 * 1024;
const APPROVAL_TTL_SECS: u64 = 300;

#[derive(Clone, Serialize)]
pub struct BrokerStatus {
    pub granted: bool,
    pub root: Option<String>,
    pub read: bool,
    pub write: bool,
    pub pending_proposals: usize,
    pub session_id: String,
    pub allowed_roots: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct FileEntry {
    pub rel_path: String,
    pub size: u64,
}

#[derive(Clone, Serialize)]
pub struct FileReadResult {
    pub path: String,
    pub content: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Clone, Serialize)]
pub struct ProposalInfo {
    pub proposal_id: String,
    pub path: String,
    pub current_sha256: String,
    pub approval_token: String,
    pub expires_in_seconds: u64,
    pub new_size: usize,
}

#[derive(Clone, Serialize)]
pub struct WriteReceipt {
    pub path: String,
    pub before_sha256: String,
    pub after_sha256: String,
    pub bytes_written: usize,
    pub readback_verified: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ProposalStatus {
    Pending,
    Applying,
    Approved,
    Applied,
    Denied,
    Expired,
    Cancelled,
    Failed,
}

struct Grant {
    root: PathBuf, // canonical
    read: bool,
    write: bool,
}

#[allow(dead_code)]
struct Proposal {
    id: String,
    path: PathBuf, // canonical, inside root
    rel_path: String,
    new_content: String,
    expected_sha256: String,
    current_sha256: String,
    status: ProposalStatus,
    created_at: u64,
    approved_at: Option<u64>,
    approval_token: String,
}

pub struct BrokerState {
    allowed_roots: Vec<PathBuf>,
    grant: Option<Grant>,
    proposals: HashMap<String, Proposal>,
    session_id: String,
    next_id: u64,
    revoked: bool,
}

impl BrokerState {
    pub fn new() -> Self {
        let mut allowed = Vec::new();
        for r in ALLOWED_ROOTS {
            if let Ok(c) = std::fs::canonicalize(r) {
                // Reject roots that are themselves reparse points.
                if !is_reparse_point(&c) {
                    allowed.push(c);
                }
            }
        }
        Self {
            allowed_roots: allowed,
            grant: None,
            proposals: HashMap::new(),
            session_id: format!("sess-{}", now_secs()),
            next_id: 1,
            revoked: false,
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

fn is_reparse_point(p: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if let Ok(m) = std::fs::symlink_metadata(p) {
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
            return (m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0;
        }
        true
    }
    #[cfg(not(windows))]
    {
        std::fs::symlink_metadata(p).map(|m| m.file_type().is_symlink()).unwrap_or(true)
    }
}

fn display_path(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s
    }
}

fn validate_text(content: &str) -> Result<(), String> {
    if content.len() > MAX_FILE_BYTES {
        return Err(format!("content exceeds the {MAX_FILE_BYTES}-byte limit"));
    }
    if content.contains('\0') {
        return Err("binary content (NUL byte) is not allowed".to_string());
    }
    Ok(())
}

// Validate a user-supplied relative path against the granted root.
// Defends against traversal, absolute/UNC/prefix forms, alternate data
// streams, reparse points and paths that resolve outside the root.
fn validate_path(root: &Path, input: &str) -> Result<PathBuf, String> {
    if input.is_empty() {
        return Err("empty path".to_string());
    }
    if input.starts_with(r"\\") || input.starts_with("//") {
        return Err("UNC and device paths are not allowed".to_string());
    }
    let p = PathBuf::from(input);
    if p.is_absolute() {
        return Err("absolute paths are not allowed".to_string());
    }
    for comp in p.components() {
        match comp {
            Component::Prefix(_) => return Err("path prefixes are not allowed".to_string()),
            Component::RootDir => return Err("rooted paths are not allowed".to_string()),
            Component::ParentDir => return Err("parent traversal ('..') is not allowed".to_string()),
            Component::CurDir => {}
            Component::Normal(c) => {
                let s = c.to_string_lossy();
                if s.is_empty() {
                    return Err("empty path component".to_string());
                }
                if s.contains(':') {
                    return Err("alternate data streams and drive letters are not allowed".to_string());
                }
            }
        }
    }

    let candidate = root.join(&p);

    // Check every existing component for reparse points before canonicalizing.
    let mut cur = root.to_path_buf();
    for comp in p.components() {
        if let Component::Normal(c) = comp {
            cur = cur.join(c);
            if cur.exists() {
                if is_reparse_point(&cur) {
                    return Err("symlinks, junctions and reparse points are not allowed".to_string());
                }
            }
        }
    }

    let canon_root = fs::canonicalize(root).map_err(|e| format!("root inaccessible: {e}"))?;
    let canon = fs::canonicalize(&candidate)
        .map_err(|e| format!("path does not exist or is inaccessible: {e}"))?;
    if !canon.starts_with(&canon_root) {
        return Err("path resolves outside the authorized root".to_string());
    }
    let meta = fs::metadata(&canon).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("target is not a regular file".to_string());
    }
    Ok(canon)
}

fn require_grant(state: &BrokerState, need_write: bool) -> Result<PathBuf, String> {
    match &state.grant {
        Some(g) if g.read && (!need_write || g.write) => Ok(g.root.clone()),
        Some(_) => Err("the current grant does not include the requested operation".to_string()),
        None => Err("no active authorization; grant a root first".to_string()),
    }
}

fn list_files_recursive(root: &Path, out: &mut Vec<FileEntry>, cur: &Path, depth: usize) {
    if depth > 6 || out.len() >= 1000 {
        return;
    }
    if let Ok(rd) = fs::read_dir(cur) {
        for entry in rd.flatten() {
            if out.len() >= 1000 {
                return;
            }
            let path = entry.path();
            // Skip reparse points entirely.
            if is_reparse_point(&path) {
                continue;
            }

            let name = entry.file_name();
            let name_s = name.to_string_lossy();
            if name_s.starts_with('.') { continue; }
            if let Ok(ft) = entry.file_type() {
                if ft.is_dir() {
                    list_files_recursive(root, out, &path, depth + 1);
                } else if ft.is_file() {
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    if path.strip_prefix(root).is_ok() {
                        out.push(FileEntry {
                            rel_path: display_path(path.strip_prefix(root).unwrap()),
                            size,
                        });
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn broker_status(state: tauri::State<Mutex<BrokerState>>) -> BrokerStatus {
    let s = state.lock().unwrap();
    match &s.grant {
        Some(g) => BrokerStatus {
            granted: true,
            root: Some(display_path(&g.root)),
            read: g.read,
            write: g.write,
            pending_proposals: s.proposals.values().filter(|p| p.status == ProposalStatus::Pending).count(),
            session_id: s.session_id.clone(),
            allowed_roots: s.allowed_roots.iter().map(|p| display_path(p)).collect(),
        },
        None => BrokerStatus {
            granted: false,
            root: None,
            read: false,
            write: false,
            pending_proposals: 0,
            session_id: s.session_id.clone(),
            allowed_roots: s.allowed_roots.iter().map(|p| display_path(p)).collect(),
        },
    }
}

#[tauri::command]
pub fn broker_grant_root(
    state: tauri::State<Mutex<BrokerState>>,
    root: String,
    read: bool,
    write: bool,
) -> Result<BrokerStatus, String> {
    if !read && !write {
        return Err("at least one of read/write must be granted".to_string());
    }
    let canon = fs::canonicalize(&root).map_err(|e| format!("root inaccessible: {e}"))?;
    if !canon.is_dir() {
        return Err("root is not a directory".to_string());
    }
    if is_reparse_point(&canon) {
        return Err("root must not be a reparse point".to_string());
    }
    let s = state.lock().unwrap();
    // The requested root must be one of the compile-time allowlisted roots or a
    // subdirectory of one. An agent cannot grant itself an arbitrary root.
    let allowed = s.allowed_roots.iter().any(|r| canon == *r || canon.starts_with(r));
    if !allowed {
        return Err("root is not in the broker allowlist".to_string());
    }
    drop(s);

    let mut s = state.lock().unwrap();
    // Cancel any pending proposals from a previous grant.
    for p in s.proposals.values_mut() {
        if p.status == ProposalStatus::Pending || p.status == ProposalStatus::Approved || p.status == ProposalStatus::Applying {
            p.status = ProposalStatus::Cancelled;
        }
    }
    s.revoked = false;
    s.grant = Some(Grant { root: canon, read, write });
    Ok(BrokerStatus {
        granted: true,
        root: s.grant.as_ref().map(|g| display_path(&g.root)),
        read,
        write,
        pending_proposals: 0,
        session_id: s.session_id.clone(),
        allowed_roots: s.allowed_roots.iter().map(|p| display_path(p)).collect(),
    })
}

#[tauri::command]
pub fn broker_revoke(state: tauri::State<Mutex<BrokerState>>) -> Result<BrokerStatus, String> {
    let mut s = state.lock().unwrap();
    s.grant = None;
    s.revoked = true;
    for p in s.proposals.values_mut() {
        if p.status == ProposalStatus::Pending || p.status == ProposalStatus::Approved || p.status == ProposalStatus::Applying {
            p.status = ProposalStatus::Cancelled;
        }
    }
    Ok(BrokerStatus {
        granted: false,
        root: None,
        read: false,
        write: false,
        pending_proposals: 0,
        session_id: s.session_id.clone(),
        allowed_roots: s.allowed_roots.iter().map(|p| display_path(p)).collect(),
    })
}

#[tauri::command]
pub fn broker_list_files(state: tauri::State<Mutex<BrokerState>>) -> Result<Vec<FileEntry>, String> {
    let root = require_grant(&state.lock().unwrap(), false)?;
    let mut out = Vec::new();
    list_files_recursive(&root, &mut out, &root, 0);
    Ok(out)
}

#[tauri::command]
pub fn broker_read_file(
    state: tauri::State<Mutex<BrokerState>>,
    path: String,
) -> Result<FileReadResult, String> {
    let root = require_grant(&state.lock().unwrap(), false)?;
    let canon = validate_path(&root, &path)?;
    let meta = fs::metadata(&canon).map_err(|e| e.to_string())?;
    if meta.len() as usize > MAX_FILE_BYTES {
        return Err("file exceeds the read size limit".to_string());
    }
    let bytes = fs::read(&canon).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err("file exceeds the read size limit".to_string());
    }
    let content = String::from_utf8(bytes.clone()).map_err(|_| "file is not valid UTF-8 text".to_string())?;
    if content.contains('\0') {
        return Err("binary content is not allowed".to_string());
    }
    let sha = sha256_hex(&bytes);
    Ok(FileReadResult {
        path: display_path(&canon),
        content,
        sha256: sha,
        size: bytes.len() as u64,
    })
}

#[tauri::command]
pub fn broker_propose_write(
    state: tauri::State<Mutex<BrokerState>>,
    path: String,
    new_content: String,
    expected_sha256: String,
) -> Result<ProposalInfo, String> {
    validate_text(&new_content)?;
    let root = require_grant(&state.lock().unwrap(), true)?;
    let canon = validate_path(&root, &path)?;

    let meta = fs::metadata(&canon).map_err(|e| e.to_string())?;
    if meta.len() as usize > MAX_FILE_BYTES {
        return Err("file exceeds the read size limit".to_string());
    }
    let bytes = fs::read(&canon).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err("file exceeds the read size limit".to_string());
    }
    let current_sha = sha256_hex(&bytes);
    if current_sha != expected_sha256 {
        return Err(format!(
            "expected checksum {expected_sha256} does not match current file {current_sha}; file changed since proposal"
        ));
    }

    let mut s = state.lock().unwrap();
    // Generate an approval token the owner must echo back.
    let token: String = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        s.next_id.hash(&mut h);
        now_secs().hash(&mut h);
        format!("{:08x}", h.finish() & 0xFFFFFFFF)
    };
    let id = format!("prop-{}", s.next_id);
    s.next_id += 1;

    let rel = display_path(canon.strip_prefix(&root).unwrap());
    let proposal = Proposal {
        id: id.clone(),
        path: canon.clone(),
        rel_path: rel,
        new_content: new_content.clone(),
        expected_sha256: expected_sha256.clone(),
        current_sha256: current_sha.clone(),
        status: ProposalStatus::Pending,
        created_at: now_secs(),
        approved_at: None,
        approval_token: token.clone(),
    };
    s.proposals.insert(id.clone(), proposal);

    Ok(ProposalInfo {
        proposal_id: id,
        path: display_path(&canon),
        current_sha256: current_sha,
        approval_token: token,
        expires_in_seconds: APPROVAL_TTL_SECS,
        new_size: new_content.len(),
    })
}

#[tauri::command]
pub fn broker_approve(
    state: tauri::State<Mutex<BrokerState>>,
    proposal_id: String,
    approval_token: String,
) -> Result<String, String> {
    let mut s = state.lock().unwrap();
    let p = s.proposals.get_mut(&proposal_id).ok_or("unknown proposal")?;
    if p.status != ProposalStatus::Pending {
        return Err(format!("proposal is not pending (status: {:?})", p.status));
    }
    if now_secs().saturating_sub(p.created_at) > APPROVAL_TTL_SECS {
        p.status = ProposalStatus::Expired;
        return Err("approval expired".to_string());
    }
    if p.approval_token != approval_token {
        return Err("approval token does not match".to_string());
    }
    p.status = ProposalStatus::Approved;
    p.approved_at = Some(now_secs());
    Ok("approved".to_string())
}

#[tauri::command]
pub fn broker_deny(
    state: tauri::State<Mutex<BrokerState>>,
    proposal_id: String,
) -> Result<String, String> {
    let mut s = state.lock().unwrap();
    let p = s.proposals.get_mut(&proposal_id).ok_or("unknown proposal")?;
    if p.status != ProposalStatus::Pending {
        return Err("proposal is not pending".to_string());
    }
    p.status = ProposalStatus::Denied;
    Ok("denied".to_string())
}

#[tauri::command]
pub fn broker_cancel(
    state: tauri::State<Mutex<BrokerState>>,
    proposal_id: String,
) -> Result<String, String> {
    let mut s = state.lock().unwrap();
    let p = s.proposals.get_mut(&proposal_id).ok_or("unknown proposal")?;
    if p.status == ProposalStatus::Applied {
        return Err("cannot cancel an already-applied write; the change is committed".to_string());
    }
    if p.status == ProposalStatus::Applying {
        return Err("cannot cancel a write that is already in progress".to_string());
    }
    p.status = ProposalStatus::Cancelled;
    Ok("cancelled".to_string())
}

#[tauri::command]
pub fn broker_apply(
    state: tauri::State<Mutex<BrokerState>>,
    proposal_id: String,
) -> Result<WriteReceipt, String> {
    // Validate approval first, then re-validate the target path and checksum.
    let (path, rel_path, new_content, expected_sha) = {
        let mut s = state.lock().unwrap();
        if s.revoked {
            return Err("authorization has been revoked".to_string());
        }
        let p = s.proposals.get_mut(&proposal_id).ok_or("unknown proposal")?;
        if p.status != ProposalStatus::Approved {
            return Err("proposal is not approved".to_string());
        }
        let approved_at = p.approved_at.unwrap_or(p.created_at);
        if now_secs().saturating_sub(approved_at) > APPROVAL_TTL_SECS {
            p.status = ProposalStatus::Expired;
            return Err("approval expired".to_string());
        }
        (p.path.clone(), p.rel_path.clone(), p.new_content.clone(), p.expected_sha256.clone())
    };

    let root = require_grant(&state.lock().unwrap(), true)?;
    // Re-validate the path against the current root (TOCTOU guard).
    let canon = validate_path(&root, &rel_path)?;
    if canon != path {
        return Err("target path changed between proposal and apply".to_string());
    }

    // Re-check the on-disk checksum.
    let before_bytes = fs::read(&canon).map_err(|e| e.to_string())?;
    let before_sha = sha256_hex(&before_bytes);
    if before_sha != expected_sha {
        return Err(format!(
            "file changed since approval (expected {expected_sha}, found {before_sha}); apply aborted"
        ));
    }

    // Bounded size check on the new content.
    if new_content.len() > MAX_FILE_BYTES {
        return Err("new content exceeds the size limit".to_string());
    }

    // Mark Applying under the lock right before the write (concurrency guard).
    {
        let mut s = state.lock().unwrap();
        if let Some(p) = s.proposals.get_mut(&proposal_id) {
            p.status = ProposalStatus::Applying;
        }
    }

    // Atomic replacement: write to a temp file in the same directory, fsync,
    // then rename. Rename on the same volume is atomic on Windows.
    let dir = canon.parent().ok_or("target has no parent directory")?;
    let tmp = dir.join(format!(".paige-broker-tmp-{}", proposal_id));
    if let Err(e) = fs::write(&tmp, new_content.as_bytes()) {
        let mut s = state.lock().unwrap();
        if let Some(p) = s.proposals.get_mut(&proposal_id) { p.status = ProposalStatus::Failed; }
        return Err(format!("temp write failed: {e}"));
    }
    if let Ok(f) = fs::OpenOptions::new().write(true).open(&tmp) {
        let _ = f.sync_all();
    }
    // If rename fails, clean up the temp file and mark the proposal failed.
    if let Err(e) = fs::rename(&tmp, &canon) {
        let _ = fs::remove_file(&tmp);
        let mut s = state.lock().unwrap();
        if let Some(p) = s.proposals.get_mut(&proposal_id) { p.status = ProposalStatus::Failed; }
        return Err(format!("atomic replace failed: {e}"));
    }
    // Best-effort directory fsync (may fail on some filesystems; documented).
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }

    // Readback verification.
    let after_bytes = fs::read(&canon).map_err(|e| e.to_string())?;
    let after_sha = sha256_hex(&after_bytes);
    let expected_after = sha256_hex(new_content.as_bytes());
    let verified = after_sha == expected_after;

    // Update proposal status.
    {
        let mut s = state.lock().unwrap();
        if let Some(p) = s.proposals.get_mut(&proposal_id) {
            p.status = if verified { ProposalStatus::Applied } else { ProposalStatus::Cancelled };
            p.current_sha256 = after_sha.clone();
        }
    }

    if !verified {
        let mut s = state.lock().unwrap();
        if let Some(p) = s.proposals.get_mut(&proposal_id) {
            p.status = ProposalStatus::Failed;
        }
        return Err("readback verification failed".to_string());
    }

    Ok(WriteReceipt {
        path: display_path(&canon),
        before_sha256: before_sha,
        after_sha256: after_sha,
        bytes_written: new_content.len(),
        readback_verified: true,
    })
}

// Unit tests for the pure validation logic.
#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root() -> PathBuf {
        let d = std::env::temp_dir().join("paige-broker-test-root");
        let _ = fs::create_dir_all(&d);
        // create a sample file
        let _ = fs::write(d.join("hello.txt"), "hello");
        d
    }

    #[test]
    fn rejects_traversal() {
        let r = tmp_root();
        assert!(validate_path(&r, "../secret.txt").is_err());
        assert!(validate_path(&r, "sub/../../etc/passwd").is_err());
    }

    #[test]
    fn rejects_absolute_and_unc() {
        let r = tmp_root();
        assert!(validate_path(&r, r"C:\Windows\System32\drivers\etc\hosts").is_err());
        assert!(validate_path(&r, r"\\server\share\file.txt").is_err());
    }

    #[test]
    fn rejects_ads() {
        let r = tmp_root();
        assert!(validate_path(&r, "hello.txt:stream").is_err());
    }

    #[test]
    fn accepts_in_scope_file() {
        let r = tmp_root();
        let p = validate_path(&r, "hello.txt").unwrap();
        assert!(p.starts_with(fs::canonicalize(&r).unwrap()));
    }

    #[test]
    fn rejects_missing_file() {
        let r = tmp_root();
        assert!(validate_path(&r, "does-not-exist.txt").is_err());
    }
}