// PAIGE Desktop - Rust native file authority broker (Increment 1B).
//
// This module is the security authority for local file access. The frontend
// and any future model/agent interact with it only through the typed Tauri
// commands below. Nothing here grants a general-purpose filesystem bridge.
//
// Scope for Increment 1B: a single owner-approved root (the lab directory),
// read and write on regular text files inside it, session-scoped grants, and
// explicit owner approval for every write. No terminal, no git, no inference.

use crate::project_access::{self, FolderIdentity};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

// --- OS-backed secure randomness (BCryptGenRandom) ---
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

pub(crate) fn os_random_bytes(buf: &mut [u8]) -> Result<(), String> {
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

// --- Native owner-mediated confirmation dialog (MessageBoxW) ---
#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(
        hwnd: *mut std::ffi::c_void,
        text: *const u16,
        caption: *const u16,
        utype: u32,
    ) -> i32;
}
const MB_YESNO: u32 = 0x00000004;
const MB_ICONQUESTION: u32 = 0x00000020;
const MB_DEFBUTTON2: u32 = 0x00000100;
const IDYES: i32 = 6;

pub(crate) fn native_confirm(message: &str, caption: &str) -> bool {
    let msg: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    let cap: Vec<u16> = caption.encode_utf16().chain(std::iter::once(0)).collect();
    let r = unsafe { MessageBoxW(std::ptr::null_mut(), msg.as_ptr(), cap.as_ptr(), MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2) };
    r == IDYES
}

// ANT-174 (superseded by ANT-173 Slice 1): this file previously carried a
// compile-time allowlist of roots the owner may grant. The dynamic Computer
// Access scope registry is now the single authority; grants outside it are
// rejected there.
const MAX_FILE_BYTES: usize = 64 * 1024;
const MAX_PROJECT_TEXT_BYTES: usize = 512 * 1024;
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
    pub remembered_roots: Vec<String>,
    pub terminal_available: bool,
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
    pub before_sha256: String,
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
    pub outcome: String,
}

#[derive(Clone, Serialize)]
pub struct OperationRecord {
    pub timestamp: u64,
    pub session_id: String,
    pub proposal_id: String,
    pub path: String,
    pub before_sha256: String,
    pub after_sha256: String,
    pub outcome: String,
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
    _directory: Option<fs::File>,
    remembered: Option<FolderIdentity>,
    read: bool,
    write: bool,
}

#[allow(dead_code)]
struct Proposal {
    id: String,
    path: PathBuf, // canonical, inside root
    rel_path: String,
    new_content: Vec<u8>,
    expected_sha256: String,
    before_sha256: String,
    proposed_sha256: String,
    status: ProposalStatus,
    created_at: u64,
    approved_at: Option<u64>,
    approval_token: String,
    force_readback_failure: bool,
}

pub struct BrokerState {
    // Root authority no longer lives here: every check consults the single
    // dynamic scope registry (crate::scope) shared with the terminal and the
    // AI tool path.
    grant: Option<Grant>,
    session_projects: HashMap<String, Grant>,
    suppressed_projects: HashSet<String>,
    proposals: HashMap<String, Proposal>,
    session_id: String,
    next_id: u64,
    revoked: bool,
    operations: Vec<OperationRecord>,
}

impl BrokerState {
    pub fn new() -> Self {
        Self {
            grant: None,
            session_projects: HashMap::new(),
            suppressed_projects: HashSet::new(),
            proposals: HashMap::new(),
            session_id: format!("sess-{}", now_secs()),
            next_id: 1,
            revoked: false,
            operations: Vec::new(),
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

pub(crate) fn is_reparse_point(p: &Path) -> bool {
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

fn is_expired(created_at: u64, approved_at: Option<u64>, now: u64) -> bool {
    let base = approved_at.unwrap_or(created_at);
    now.saturating_sub(base) > APPROVAL_TTL_SECS
}

pub(crate) fn validate_text(content: &str) -> Result<(), String> {
    validate_text_bytes(content, MAX_FILE_BYTES)
}

pub(crate) fn validate_targeted_text(content: &str) -> Result<(), String> {
    validate_text_bytes(content, MAX_PROJECT_TEXT_BYTES)
}

fn validate_text_bytes(content: &str, limit: usize) -> Result<(), String> {
    if content.len() > limit {
        return Err(format!("content exceeds the {limit}-byte limit"));
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
                if s.ends_with(' ') || s.ends_with('.') {
                    return Err("trailing spaces or dots are not allowed (Windows path aliasing)".to_string());
                }
                // Control characters are never valid in Windows filenames.
                if s.chars().any(|ch| ch.is_control()) {
                    return Err("control characters are not allowed".to_string());
                }
                // Reserved DOS device names (case-insensitive, with or without extension).
                let upper = s.to_uppercase();
                let stem = upper.split('.').next().unwrap_or("");
                let reserved = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4",
                    "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3",
                    "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"];
                if reserved.contains(&stem) {
                    return Err("reserved Windows device name is not allowed".to_string());
                }
                // Heuristic: 8.3 short-name aliases contain a tilde followed by a digit.
                if s.contains('~') && s.chars().any(|ch| ch.is_ascii_digit()) {
                    return Err("8.3 short-name alias is not allowed".to_string());
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
    let canon = match fs::canonicalize(&candidate) {
        Ok(c) => c,
        Err(_) => {
            // File may not exist yet (new file). Canonicalize the parent and
            // join the filename to validate the path is inside the root.
            let parent = candidate.parent().ok_or("invalid path")?;
            let filename = candidate.file_name().ok_or("invalid filename")?;
            let canon_parent = fs::canonicalize(parent)
                .map_err(|e| format!("path does not exist or is inaccessible: {e}"))?;
            canon_parent.join(filename)
        }
    };
    if !canon.starts_with(&canon_root) {
        return Err("path resolves outside the authorized root".to_string());
    }
    if canon.exists() {
        let meta = fs::metadata(&canon).map_err(|e| e.to_string())?;
        if !meta.is_file() {
            return Err("target is not a regular file".to_string());
        }
    }
    Ok(canon)
}

fn require_grant(state: &BrokerState, need_write: bool) -> Result<PathBuf, String> {
    let grant = state.grant.as_ref().ok_or("no active authorization; grant a root first")?;
    if !grant.read || (need_write && !grant.write) {
        return Err("the current grant does not include the requested operation".into());
    }
    // The Computer Access registry is the standing authority. A session minted
    // before the owner removed or downgraded a root stops working here, so
    // revocation reaches every read, browse and write boundary.
    let (scope_read, scope_write) = crate::scope::effective_access(&grant.root);
    if !scope_read || (need_write && !scope_write) {
        return Err("The active project is no longer in the authorized Computer Access scope.".into());
    }
    if let Some(identity) = &grant.remembered {
        let current = hold_project_directory(&grant.root).and_then(|file| FolderIdentity::read(&file));
        if current.as_ref() != Ok(identity) || project_access::load()?.get(&display_path(&grant.root)) != Some(identity) {
            return Err("Remembered access changed. Authorize this project again.".into());
        }
    }
    Ok(grant.root.clone())
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
            // Dependency trees are never useful source context and previously
            // crowded the 1000-file listing cap with thousands of package files.
            if name_s.eq_ignore_ascii_case("node_modules") { continue; }
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
pub fn broker_operations(state: tauri::State<Mutex<BrokerState>>) -> Vec<OperationRecord> {
    state.lock().unwrap().operations.clone()
}

#[derive(Clone, Serialize)]
pub struct ReconcileResult {
    pub proposal_id: String,
    pub path: String,
    pub current_sha256: String,
    pub before_sha256: String,
    pub after_sha256: String,
    pub outcome: String,
}

#[tauri::command]
#[cfg(feature = "insecure-test-bypass")]
pub fn broker_test_force_readback_failure(
    state: tauri::State<Mutex<BrokerState>>,
    proposal_id: String,
    fail: bool,
) -> Result<String, String> {
    let mut s = state.lock().unwrap();
    let p = s.proposals.get_mut(&proposal_id).ok_or("unknown proposal")?;
    p.force_readback_failure = fail;
    Ok(format!("force_readback_failure set to {fail}"))
}

#[tauri::command]
pub fn broker_reconcile(
    state: tauri::State<Mutex<BrokerState>>,
    proposal_id: String,
) -> Result<ReconcileResult, String> {
    let (path, before, after) = {
        let s = state.lock().unwrap();
        let p = s.proposals.get(&proposal_id).ok_or("unknown proposal")?;
        (p.path.clone(), p.before_sha256.clone(), p.proposed_sha256.clone())
    };
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    let current = sha256_hex(&bytes);
    let outcome = if current == after {
        "matches_after"
    } else if current == before {
        "matches_before"
    } else {
        "unknown"
    };
    Ok(ReconcileResult {
        proposal_id,
        path: display_path(&path),
        current_sha256: current,
        before_sha256: before,
        after_sha256: after,
        outcome: outcome.to_string(),
    })
}

#[tauri::command]
pub fn broker_status(state: tauri::State<Mutex<BrokerState>>) -> BrokerStatus {
    status_of(&state.lock().unwrap())
}

fn status_of(s: &BrokerState) -> BrokerStatus {
    let remembered = project_access::load().unwrap_or_default();
    let grant = s.grant.as_ref().filter(|grant| grant.remembered.as_ref().is_none_or(|identity|
        remembered.get(&display_path(&grant.root)) == Some(identity)));
    BrokerStatus {
        granted: grant.is_some(),
        root: grant.map(|grant| display_path(&grant.root)),
        read: grant.is_some_and(|grant| grant.read),
        write: grant.is_some_and(|grant| grant.write),
        terminal_available: grant.is_some_and(|grant| grant.read && crate::scope::workspace_root_supported(&grant.root)),
        pending_proposals: if grant.is_some() {
            s.proposals.values().filter(|p| p.status == ProposalStatus::Pending).count()
        } else { 0 },
        session_id: s.session_id.clone(),
        allowed_roots: crate::scope::display_roots(),
        remembered_roots: remembered.into_keys().filter(|root| !s.suppressed_projects.contains(root)).collect(),
    }
}

#[tauri::command]
pub fn broker_grant_root(
    state: tauri::State<Mutex<BrokerState>>,
    root: String,
    read: bool,
    write: bool,
) -> Result<BrokerStatus, String> {
    let _turn = crate::chats::ChatsStore::lock_turn()?;
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
    // The requested root must be inside the single dynamic scope registry at
    // the requested operation class. An agent cannot grant itself a root the
    // owner never authorized.
    let allowed = crate::scope::is_authorized_root(&canon, write);
    if !allowed {
        return Err("root is not in the authorized Computer Access scope".to_string());
    }
    drop(s);
    let directory = hold_project_directory(&canon)?;
    activate_root(&mut state.lock().unwrap(), Grant { root: canon, _directory: Some(directory), remembered: None, read, write })
}

fn project_key(root: &Path) -> String {
    display_path(root)
}

fn can_switch(s: &BrokerState) -> Result<(), String> {
    if s.proposals.values().any(|proposal| proposal.status == ProposalStatus::Applying) {
        return Err("Wait for the approved file write before switching projects.".into());
    }
    Ok(())
}

fn activate_root(s: &mut BrokerState, grant: Grant) -> Result<BrokerStatus, String> {
    can_switch(s)?;
    let key = project_key(&grant.root);
    s.session_projects.remove(&key);
    if let Some(previous) = s.grant.take() {
        let previous_key = project_key(&previous.root);
        if previous_key != key {
            s.session_projects.insert(previous_key, previous);
        }
    }
    cancel_queued_writes(s);
    s.revoked = false;
    s.grant = Some(grant);
    Ok(status_of(s))
}

fn cancel_queued_writes(state: &mut BrokerState) {
    for proposal in state.proposals.values_mut() {
        if matches!(proposal.status, ProposalStatus::Pending | ProposalStatus::Approved) {
            proposal.status = ProposalStatus::Cancelled;
        }
    }
}

/// The active session grant's root and write flag, for scope-revocation checks.
pub(crate) fn active_grant_root(state: &Mutex<BrokerState>) -> Option<(PathBuf, bool)> {
    let s = state.lock().unwrap();
    s.grant.as_ref().map(|g| (g.root.clone(), g.write))
}

pub(crate) fn suspend_session_project(state: tauri::State<Mutex<BrokerState>>) -> Result<(), String> {
    let mut state = state.lock().unwrap();
    can_switch(&state)?;
    if let Some(active) = state.grant.take() {
        state.session_projects.insert(project_key(&active.root), active);
        cancel_queued_writes(&mut state);
    }
    Ok(())
}

// Called under the active Ask turn lock, including when the saved chat opens.
pub(crate) fn select_session_project(
    app: &tauri::AppHandle, state: tauri::State<Mutex<BrokerState>>, root: &str, require_write: bool,
) -> Result<bool, String> {
    let mut s = state.lock().unwrap();
    let key = root;
    let remembered = project_access::load().unwrap_or_default();
    let usable = |grant: &Grant| {
        let (scope_read, scope_write) = crate::scope::effective_access(&grant.root);
        grant.read && scope_read && (!require_write || (grant.write && scope_write))
            && grant.remembered.as_ref().is_none_or(|identity|
                remembered.get(&display_path(&grant.root)) == Some(identity)
                    && hold_project_directory(&grant.root).and_then(|file| FolderIdentity::read(&file)).as_ref() == Ok(identity))
    };
    if s.grant.as_ref().is_some_and(|active| project_key(&active.root) == key && usable(active)) {
        return Ok(true);
    }
    if s.session_projects.get(key).is_some_and(usable) {
        can_switch(&s)?;
        let cached = s.session_projects.remove(key).expect("checked session project");
        activate_root(&mut s, cached)?;
        return Ok(true);
    }

    if s.suppressed_projects.contains(key) {
        return Ok(false);
    }
    let Some(identity) = remembered.get(root) else { return Ok(false) };
    let Ok(canonical) = fs::canonicalize(root) else { return Ok(false) };
    if display_path(&canonical) != root {
        return Ok(false);
    }
    let Ok(directory) = hold_project_directory(&canonical) else { return Ok(false) };
    let Ok(current) = FolderIdentity::read(&directory) else { return Ok(false) };
    if &current != identity {
        project_access::forget_replaced(root, identity)?;
        return Ok(false);
    }

    let (scope_read, scope_write) = crate::scope::effective_access(&canonical);
    if !scope_read {
        // The owner removed this root from Computer Access: opening the chat
        // must not mint authority the registry no longer grants.
        return Ok(false);
    }

    activate_root(&mut s, Grant {
        root: canonical.clone(), _directory: Some(directory), remembered: Some(identity.clone()),
        read: scope_read, write: scope_write,
    })?;
    drop(s);
    crate::project::register_open_folder(app, &canonical, root);
    Ok(true)
}

pub(crate) fn activate_project_folder(
    state: tauri::State<Mutex<BrokerState>>, root: PathBuf, directory: fs::File, remember: bool,
) -> Result<BrokerStatus, String> {
    let mut s = state.lock().unwrap();
    can_switch(&s)?;
    // Computer Access is the only authority that can open a project session.
    let (read, write) = crate::scope::effective_access(&root);
    if !read {
        return Err("This folder is not in PAIGE's Computer Access scope. Add it under Settings > Computer Access first.".into());
    }
    let remembered = if remember {
        let identity = FolderIdentity::read(&directory)?;
        project_access::update(&display_path(&root), Some(identity.clone()))?;
        s.suppressed_projects.remove(&project_key(&root));
        Some(identity)
    } else {
        None
    };
    activate_root(&mut s, Grant { root, _directory: Some(directory), remembered, read, write })
}

#[tauri::command]
pub fn broker_revoke(state: tauri::State<Mutex<BrokerState>>, root: Option<String>) -> Result<BrokerStatus, String> {
    let mut s = state.lock().unwrap();
    let root = root.or_else(|| s.grant.as_ref().map(|grant| display_path(&grant.root)));
    let mut durable = Ok(());
    if let Some(root) = &root {
        let key = root.clone();
        durable = project_access::update(root, None)
            .map_err(|error| format!("Session access was revoked. The saved approval could not be removed. {error}"));
        if durable.is_err() {
            s.suppressed_projects.insert(key.clone());
        }
        s.session_projects.remove(&key);
        if s.grant.as_ref().is_none_or(|active| project_key(&active.root) != key) {
            durable?;
            return Ok(status_of(&s));
        }
    }
    s.grant = None;
    s.revoked = true;
    for p in s.proposals.values_mut() {
        if p.status == ProposalStatus::Pending || p.status == ProposalStatus::Approved || p.status == ProposalStatus::Applying {
            p.status = ProposalStatus::Cancelled;
        }
    }
    durable?;
    Ok(status_of(&s))
}

// Check the current grant without collecting or disclosing filenames.
pub(crate) fn verify_workspace_read(state: tauri::State<Mutex<BrokerState>>, expected_root: Option<&str>) -> Result<Option<(String, bool)>, String> {
    let locked = state.lock().unwrap();
    let Some(grant) = locked.grant.as_ref().filter(|grant|
        expected_root.is_none_or(|root| display_path(&grant.root) == root)) else { return Ok(None); };
    let root = require_grant(&locked, false)?;
    let _directory = hold_project_directory(&root)?;
    let mut entries = fs::read_dir(&root).map_err(|error| format!("Could not open the active folder: {error}"))?;
    entries.next().transpose().map_err(|error| format!("Could not read the active folder: {error}"))?;
    Ok(Some((display_path(&root), grant.write)))
}

#[tauri::command]
pub fn broker_list_files(state: tauri::State<Mutex<BrokerState>>, expected_root: Option<String>) -> Result<Vec<FileEntry>, String> {
    let locked = state.lock().unwrap();
    let root = require_grant(&locked, false)?;
    if expected_root.is_some_and(|expected| expected != display_path(&root)) {
        return Err("Authorize this chat’s project before browsing its files.".into());
    }
    Ok(list_files_at(&root))
}

pub(crate) struct ProjectOverview {
    pub directories: Vec<String>,
    pub files: Vec<String>,
    pub truncated: bool,
    pub package: Option<Result<FileReadResult, String>>,
}

pub(crate) fn inspect_project_root(
    state: tauri::State<Mutex<BrokerState>>, expected_root: &str,
) -> Result<ProjectOverview, String> {
    const MAX_ROOT_ENTRIES: usize = 256;
    let locked = state.lock().unwrap();
    let root = require_grant(&locked, false)?;
    if display_path(&root) != expected_root {
        return Err("The active grant differs from the chat project.".into());
    }
    let _directory = hold_project_directory(&root)?;
    let mut overview = ProjectOverview {
        directories: Vec::new(), files: Vec::new(), truncated: false, package: None,
    };
    let entries = fs::read_dir(&root).map_err(|error| format!("Could not open the pinned project: {error}"))?;
    for (index, entry) in entries.enumerate() {
        if index == MAX_ROOT_ENTRIES {
            overview.truncated = true;
            break;
        }
        let entry = entry.map_err(|error| format!("Could not read the pinned project: {error}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || is_reparse_point(&entry.path()) {
            continue;
        }
        let kind = entry.file_type().map_err(|error| format!("Could not inspect a project entry: {error}"))?;
        if kind.is_dir() {
            overview.directories.push(name);
        } else if kind.is_file() {
            overview.files.push(name);
        }
    }
    overview.directories.sort_unstable();
    overview.files.sort_unstable();
    if overview.files.iter().any(|file| file.eq_ignore_ascii_case("package.json")) {
        overview.package = Some(read_project_manifest(&root));
    }
    Ok(overview)
}

fn read_project_manifest(root: &Path) -> Result<FileReadResult, String> {
    // The root is held by inspect_project_root. Open the direct child without
    // following a reparse point and then verify and read the same handle.
    let path = root.join("package.json");
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(OPEN_REPARSE_POINT);
    }
    let file = options.open(&path).map_err(|error| error.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE_POINT: u32 = 0x400;
        if file.metadata().map_err(|error| error.to_string())?.file_attributes() & REPARSE_POINT != 0 {
            return Err("Project manifest is redirected.".into());
        }
    }
    read_open_text_file(file, &path)
}

fn list_files_at(root: &Path) -> Vec<FileEntry> {
    let mut out = Vec::new();
    list_files_recursive(root, &mut out, root, 0);
    out
}

fn workflow_root(state: &BrokerState, expected_root: &str, write: bool) -> Result<(PathBuf, fs::File), String> {
    let root = require_grant(state, write)?;
    if display_path(&root) != expected_root {
        return Err("The workflow workspace grant changed.".into());
    }
    let handle = hold_project_directory(&root)?;
    Ok((root, handle))
}

pub(crate) fn hold_project_directory(root: &Path) -> Result<fs::File, String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ_WRITE: u32 = 0x3; // Omit share-delete while the directory is in use.
        const OPEN_DIRECTORY_OR_REPARSE_POINT: u32 = 0x0220_0000;
        options.share_mode(FILE_SHARE_READ_WRITE).custom_flags(OPEN_DIRECTORY_OR_REPARSE_POINT);
    }
    let handle = options.open(root).map_err(|_| "The project folder could not be opened.")?;
    if is_reparse_point(root) || fs::canonicalize(root).ok().as_deref() != Some(root)
        || !handle.metadata().is_ok_and(|metadata| metadata.is_dir()) {
        return Err("The project folder was moved or redirected.".into());
    }
    Ok(handle)
}

pub(crate) fn lock_workflow_root(
    state: tauri::State<Mutex<BrokerState>>, root: &str, write: bool,
) -> Result<fs::File, String> {
    workflow_root(&state.lock().unwrap(), root, write).map(|(_, handle)| handle)
}

pub(crate) fn list_workflow_files(state: tauri::State<Mutex<BrokerState>>, root: &str) -> Result<Vec<FileEntry>, String> {
    let locked = state.lock().unwrap();
    let (root, _handle) = workflow_root(&locked, root, true)?;
    Ok(list_files_at(&root))
}

pub(crate) fn read_workflow_file(
    state: tauri::State<Mutex<BrokerState>>, root: &str, path: &str, write: bool,
) -> Result<FileReadResult, String> {
    let locked = state.lock().unwrap();
    let (root, _handle) = workflow_root(&locked, root, write)?;
    let canon = validate_path(&root, path)?;
    if !canon.starts_with(&root) {
        return Err("The workflow file resolved outside its saved workspace.".into());
    }
    read_canonical_file(&canon)
}

pub(crate) fn workflow_file_checksum(
    state: tauri::State<Mutex<BrokerState>>, root: &str, path: &str, write: bool,
) -> Result<(String, String), String> {
    let locked = state.lock().unwrap();
    let (root, _handle) = workflow_root(&locked, root, write)?;
    let canon = validate_path(&root, path)?;
    Ok((display_path(&canon), target_checksum(&canon)?))
}

pub(crate) fn workflow_read_verified_bytes(
    state: tauri::State<Mutex<BrokerState>>, root: &str, path: &str,
    expected_path: &str, expected_hash: &str,
) -> Result<Vec<u8>, String> {
    let locked = state.lock().unwrap();
    let (root, _handle) = workflow_root(&locked, root, false)?;
    let target = validate_path(&root, path)?;
    if display_path(&target) != expected_path {
        return Err("The report path differs from its verified receipt.".into());
    }
    let bytes = read_open_bytes(fs::File::open(target).map_err(|error| error.to_string())?)?;
    if sha256_hex(&bytes) != expected_hash {
        return Err("The report changed after its verified preview.".into());
    }
    Ok(bytes)
}

#[tauri::command]
pub fn broker_read_file(
    state: tauri::State<Mutex<BrokerState>>,
    path: String,
    expected_root: Option<String>,
) -> Result<FileReadResult, String> {
    let locked = state.lock().unwrap();
    let root = require_grant(&locked, false)?;
    if expected_root.is_some_and(|expected| expected != display_path(&root)) {
        return Err("The active grant differs from the chat project.".into());
    }
    read_file_at(&root, &path)
}

fn read_file_at(root: &Path, path: &str) -> Result<FileReadResult, String> {
    let canon = validate_path(root, path)?;
    read_canonical_file(&canon)
}

fn read_canonical_file(canon: &Path) -> Result<FileReadResult, String> {
    let file = fs::File::open(canon).map_err(|error| error.to_string())?;
    decode_text_file(read_sized_bytes(file, MAX_PROJECT_TEXT_BYTES)?, canon)
}

pub(crate) fn read_workflow_image(
    state: tauri::State<Mutex<BrokerState>>, root: &str, path: &str,
) -> Result<Vec<u8>, String> {
    let locked = state.lock().unwrap();
    let (root, _handle) = workflow_root(&locked, root, false)?;
    let target = validate_path(&root, path)?;
    let mut directories = Vec::new();
    let mut current = root;
    let mut components = path.split('/').peekable();
    while let Some(component) = components.next() {
        current.push(component);
        if components.peek().is_some() {
            directories.push(hold_project_directory(&current)?);
        }
    }
    if current != target { return Err("The screenshot path changed during validation.".into()); }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.share_mode(1).custom_flags(OPEN_REPARSE_POINT);
    }
    let file = options.open(&target).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("The screenshot is redirected.".into());
        }
    }
    if !metadata.is_file() || is_reparse_point(&target) {
        return Err("Choose a regular PNG file inside the pinned worktree.".into());
    }
    read_sized_bytes(file, crate::visual_qa::MAX_PNG_BYTES)
}

fn read_open_bytes(file: fs::File) -> Result<Vec<u8>, String> {
    read_sized_bytes(file, MAX_FILE_BYTES)
}

fn read_sized_bytes(file: fs::File, limit: usize) -> Result<Vec<u8>, String> {
    let meta = file.metadata().map_err(|error| error.to_string())?;
    if !meta.is_file() {
        return Err("target is not a regular file".into());
    }
    if meta.len() > limit as u64 {
        return Err("file exceeds the read size limit".to_string());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
    if bytes.len() > limit {
        return Err("file exceeds the read size limit".to_string());
    }
    Ok(bytes)
}

fn read_open_text_file(file: fs::File, path: &Path) -> Result<FileReadResult, String> {
    decode_text_file(read_open_bytes(file)?, path)
}

fn decode_text_file(bytes: Vec<u8>, path: &Path) -> Result<FileReadResult, String> {
    let size = bytes.len() as u64;
    let sha = sha256_hex(&bytes);
    let content = String::from_utf8(bytes).map_err(|_| "file is not valid UTF-8 text".to_string())?;
    if content.contains('\0') {
        return Err("binary content is not allowed".to_string());
    }
    Ok(FileReadResult {
        path: display_path(path),
        content,
        sha256: sha,
        size,
    })
}

pub(crate) fn write_identity(
    state: tauri::State<Mutex<BrokerState>>,
    path: &str,
    content: &[u8],
) -> Result<crate::continuation::FileEdit, String> {
    let root = require_grant(&state.lock().unwrap(), true)?;
    let target = validate_path(&root, path)?;
    Ok(crate::continuation::FileEdit {
        path: display_path(&target),
        sha256: sha256_hex(content),
    })
}

pub(crate) fn scoped_command_cwd(
    state: tauri::State<Mutex<BrokerState>>,
    requested: Option<&str>,
    saved_root: Option<&str>,
) -> Result<Option<String>, String> {
    let locked = state.lock().unwrap();
    if locked.grant.is_none() && saved_root.is_none() {
        return Ok(requested.map(str::to_owned));
    }
    let root = require_grant(&locked, false)?;
    if saved_root.is_some_and(|path| !display_path(&root).eq_ignore_ascii_case(path)) {
        return Err("The authorized workspace changed before command dispatch.".into());
    }
    let cwd = match requested {
        Some(path) => fs::canonicalize(path).map_err(|error| format!("Could not open command folder: {error}"))?,
        None => root.clone(),
    };
    if !cwd.is_dir() || !cwd.starts_with(&root) {
        return Err("The command must run inside the authorized workspace.".into());
    }
    Ok(Some(display_path(&cwd)))
}

#[tauri::command]
pub fn broker_propose_write(
    state: tauri::State<Mutex<BrokerState>>,
    path: String,
    new_content: String,
    expected_sha256: String,
    expected_root: Option<String>,
) -> Result<ProposalInfo, String> {
    validate_text(&new_content)?;
    propose_bytes(state, path, new_content.into_bytes(), expected_sha256, expected_root, false)
}

pub(crate) fn propose_bytes(
    state: tauri::State<Mutex<BrokerState>>,
    path: String,
    new_content: Vec<u8>,
    expected_sha256: String,
    expected_root: Option<String>,
    targeted_edit: bool,
) -> Result<ProposalInfo, String> {
    let limit = if targeted_edit { MAX_PROJECT_TEXT_BYTES } else { MAX_FILE_BYTES };
    if new_content.len() > limit {
        return Err(format!("content exceeds the {limit}-byte limit"));
    }
    let root = require_grant(&state.lock().unwrap(), true)?;
    if expected_root.is_some_and(|expected| expected != display_path(&root)) {
        return Err("The active grant differs from the chat project.".into());
    }
    let canon = validate_path(&root, &path)?;

    let current_sha = checksum_with_limit(&canon, limit)?;
    if current_sha != expected_sha256 {
        return Err(format!(
            "expected checksum {expected_sha256} does not match current file {current_sha}; file changed since proposal"
        ));
    }

    let mut s = state.lock().unwrap();
    // Generate an approval token the owner must echo back.
    // Cryptographically secure, single-use authorization identifier bound to
    // this proposal. Generated from OS-backed randomness (BCryptGenRandom).
    let mut raw = [0u8; 32];
    os_random_bytes(&mut raw).map_err(|e| format!("secure RNG failed: {e}"))?;
    let token = hex::encode(raw);
    let id = format!("prop-{}", s.next_id);
    s.next_id += 1;

    let rel = display_path(canon.strip_prefix(&root).map_err(|_| "The write target is outside the grant.")?);
    let proposed_sha256 = sha256_hex(&new_content);
    let new_size = new_content.len();
    let proposal = Proposal {
        id: id.clone(),
        path: canon.clone(),
        rel_path: rel,
        new_content,
        expected_sha256: expected_sha256.clone(),
        before_sha256: current_sha.clone(),
        proposed_sha256,
        status: ProposalStatus::Pending,
        created_at: now_secs(),
        approved_at: None,
        approval_token: token.clone(),
        force_readback_failure: false,
    };
    s.proposals.insert(id.clone(), proposal);

    Ok(ProposalInfo {
        proposal_id: id,
        path: display_path(&canon),
        before_sha256: current_sha.clone(),
        approval_token: token,
        expires_in_seconds: APPROVAL_TTL_SECS,
        new_size,
    })
}

fn target_checksum(path: &Path) -> Result<String, String> {
    checksum_with_limit(path, MAX_PROJECT_TEXT_BYTES)
}

fn checksum_with_limit(path: &Path, limit: usize) -> Result<String, String> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(error.to_string()),
    };
    let bytes = read_sized_bytes(file, limit)?;
    Ok(sha256_hex(&bytes))
}

#[tauri::command]
pub fn broker_approve(
    state: tauri::State<Mutex<BrokerState>>,
    proposal_id: String,
    approval_token: String,
) -> Result<String, String> {
    let mut s = state.lock().unwrap();
    let session_id = s.session_id.clone();
    let p = s.proposals.get_mut(&proposal_id).ok_or("unknown proposal")?;
    if p.status != ProposalStatus::Pending {
        return Err(format!("proposal is not pending (status: {:?})", p.status));
    }
    if is_expired(p.created_at, None, now_secs()) {
        p.status = ProposalStatus::Expired;
        return Err("approval expired".to_string());
    }
    if p.approval_token != approval_token {
        return Err("approval token does not match".to_string());
    }
    // Owner-mediated native confirmation boundary. The WebView cannot
    // programmatically click this dialog; only the owner can approve.
    // A test-only escape hatch (PAIGE_BROKER_TEST_MODE=1) skips the dialog so
    // automated tests can exercise the full flow. It is never set in a normal
    // production launch; the production binary requires the native dialog.
    if !uses_test_confirmation() {
        let proposed_hash = sha256_hex(&p.new_content);
        let msg = format!(
            "PAIGE Desktop - approve file write?\\n\\nTarget: {}\\nSession: {}\\n\\nSHA-256 before: {}\\nSHA-256 after : {}\\nBytes: {}\\n\\nApprove this write?",
            display_path(&p.path),
            session_id,
            p.before_sha256,
            proposed_hash,
            p.new_content.len()
        );
        if !native_confirm(&msg, "PAIGE Desktop - Owner Approval") {
            return Err("owner declined the native confirmation dialog".to_string());
        }
    }
    p.status = ProposalStatus::Approved;
    p.approved_at = Some(now_secs());
    Ok("approved".to_string())
}

pub(crate) fn uses_test_confirmation() -> bool {
    #[cfg(feature = "insecure-test-bypass")]
    { std::env::var("PAIGE_BROKER_TEST_MODE").ok().as_deref() == Some("1") }
    #[cfg(not(feature = "insecure-test-bypass"))]
    { false }
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
        if is_expired(p.created_at, p.approved_at, now_secs()) {
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

    // An empty checksum means the target must still be absent.
    let before_sha = target_checksum(&canon)?;
    if before_sha != expected_sha {
        return Err(format!(
            "file changed since approval (expected {expected_sha}, found {before_sha}); apply aborted"
        ));
    }

    // Bounded size check on the new content.
    if new_content.len() > MAX_PROJECT_TEXT_BYTES {
        return Err("new content exceeds the size limit".to_string());
    }

    // Mark Applying under the lock right before the write (concurrency guard).
    {
        let mut s = state.lock().unwrap();
        if s.revoked {
            return Err("authorization revoked before write".to_string());
        }
        if let Some(p) = s.proposals.get_mut(&proposal_id) {
            if p.status != ProposalStatus::Approved {
                return Err("proposal is no longer approved".to_string());
            }
            p.status = ProposalStatus::Applying;
        } else {
            return Err("unknown proposal".to_string());
        }
    }

    // Re-check revoked and scope immediately before touching the filesystem.
    if state.lock().unwrap().revoked || !crate::scope::is_authorized_root(&root, true) {
        let mut s = state.lock().unwrap();
        if let Some(p) = s.proposals.get_mut(&proposal_id) {
            p.status = ProposalStatus::Cancelled;
        }
        return Err("authorization revoked before write".to_string());
    }

    // Atomic replacement: write to a temp file in the same directory, fsync,
    // then rename. Rename on the same volume is atomic on Windows.
    let dir = canon.parent().ok_or("target has no parent directory")?;
    let mut random = [0u8; 16];
    os_random_bytes(&mut random).map_err(|e| format!("secure RNG failed: {e}"))?;
    let tmp = dir.join(format!(".paige-broker-tmp-{}", hex::encode(random)));
    if let Err(e) = write_temp_file(&tmp, &new_content) {
        let mut s = state.lock().unwrap();
        if let Some(p) = s.proposals.get_mut(&proposal_id) { p.status = ProposalStatus::Failed; }
        return Err(format!("temp write failed: {e}"));
    }
    // Hard-link creation is atomic and fails if anything created the new
    // target after the checksum check. Existing targets use atomic replace.
    let publish = (|| -> Result<(), String> {
        let s = state.lock().unwrap();
        // Serialize final publication against durable revocation in other app instances.
        let _access_lock = s.grant.as_ref().filter(|grant| grant.remembered.is_some())
            .map(|_| project_access::lock_for_publication()).transpose()?;
        if require_grant(&s, true)? != root
            || s.proposals.get(&proposal_id).is_none_or(|p| p.status != ProposalStatus::Applying) {
            return Err("The grant changed before publication.".into());
        }
        if validate_path(&root, &rel_path)? != canon || target_checksum(&canon)? != expected_sha {
            return Err("The target changed before publication.".into());
        }
        if expected_sha.is_empty() { fs::hard_link(&tmp, &canon) } else { fs::rename(&tmp, &canon) }
            .map_err(|e| e.to_string())
    })();
    if expected_sha.is_empty() || publish.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    if let Err(e) = publish {
        let mut s = state.lock().unwrap();
        if let Some(p) = s.proposals.get_mut(&proposal_id).filter(|p| p.status == ProposalStatus::Applying) {
            p.status = ProposalStatus::Failed;
        }
        return Err(format!("atomic replace failed: {e}"));
    }
    // Best-effort directory fsync (may fail on some filesystems; documented).
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }

    // Readback verification.
    let after_sha = target_checksum(&canon).unwrap_or_default();
    let expected_after = sha256_hex(&new_content);
    let force_fail = {
        let s = state.lock().unwrap();
        s.proposals.get(&proposal_id).map(|p| p.force_readback_failure).unwrap_or(false)
    };
    let verified = !force_fail && after_sha == expected_after;

    // Update proposal status.
    {
        let mut s = state.lock().unwrap();
        if let Some(p) = s.proposals.get_mut(&proposal_id) {
            p.status = if verified { ProposalStatus::Applied } else { ProposalStatus::Failed };
            p.force_readback_failure = false;
        }
    }

    let outcome = if verified { "committed_verified" } else { "committed_unverified" };

    // Record the operation (session-scoped, minimal local record).
    {
        let mut s = state.lock().unwrap();
        let sid = s.session_id.clone();
        s.operations.push(OperationRecord {
            timestamp: now_secs(),
            session_id: sid,
            proposal_id: proposal_id.clone(),
            path: display_path(&canon),
            before_sha256: before_sha.clone(),
            after_sha256: after_sha.clone(),
            outcome: outcome.to_string(),
        });
    }

    // A committed-but-unverified write returns a structured receipt (not an
    // opaque error) so callers can reconcile instead of blindly retrying.
    // Pre-commit failures still return Err above.
    Ok(WriteReceipt {
        path: display_path(&canon),
        before_sha256: before_sha,
        after_sha256: after_sha,
        bytes_written: new_content.len(),
        readback_verified: verified,
        outcome: outcome.to_string(),
    })
}

pub(crate) fn write_temp_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    let result = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
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
    fn larger_project_text_uses_the_same_hash_for_read_approval_and_readback() {
        let root = tmp_root();
        let path = root.join(format!("large-source-{}.tsx", std::process::id()));
        let before = "const ready = '📎';\r\n".repeat(4_000);
        fs::write(&path, &before).unwrap();
        let read = read_canonical_file(&path).unwrap();
        assert_eq!(read.content, before);
        assert_eq!(target_checksum(&path).unwrap(), read.sha256);
        assert!(checksum_with_limit(&path, MAX_FILE_BYTES).is_err());
        let after = before.replacen("ready", "reviewed", 1);
        assert!(validate_targeted_text(&after).is_ok());
        assert!(validate_text(&after).is_err());
        fs::write(&path, &after).unwrap();
        assert_eq!(target_checksum(&path).unwrap(), sha256_hex(after.as_bytes()));
        assert!(validate_targeted_text(&"x".repeat(MAX_PROJECT_TEXT_BYTES + 1)).is_err());
        fs::remove_file(path).unwrap();
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
    fn allows_missing_file_for_creation() {
        // 1F: validate_path now permits non-existent files (new file creation).
        // The path must still be inside the root and pass all safety checks.
        let r = tmp_root();
        assert!(validate_path(&r, "does-not-exist.txt").is_ok());
        // But traversal is still rejected.
        assert!(validate_path(&r, "../escape.txt").is_err());
    }

    #[test]
    fn rejects_trailing_spaces_and_dots() {
        let r = tmp_root();
        let _ = fs::write(r.join("trail.txt"), "x");
        assert!(validate_path(&r, "trail.txt ").is_err());
        assert!(validate_path(&r, "trail.txt.").is_err());
    }

    #[test]
    fn rejects_reserved_device_names() {
        let r = tmp_root();
        let _ = fs::write(r.join("CON"), "x");
        let _ = fs::write(r.join("nul.txt"), "x");
        assert!(validate_path(&r, "CON").is_err());
        assert!(validate_path(&r, "nul.txt").is_err());
    }

    #[test]
    fn rejects_control_chars() {
        let r = tmp_root();
        let bad = format!("hel{}lo.txt", '\u{0001}');
        assert!(validate_path(&r, &bad).is_err());
    }

    #[test]
    fn rejects_8_3_alias() {
        let r = tmp_root();
        let _ = fs::write(r.join("PROGRA~1.txt"), "x");
        assert!(validate_path(&r, "PROGRA~1.txt").is_err());
    }

    #[test]
    fn expiry_uses_approval_time() {
        // Approved late in the window gets the full TTL from approval.
        assert!(!is_expired(0, Some(100), 100 + APPROVAL_TTL_SECS - 1));
        assert!(is_expired(0, Some(100), 100 + APPROVAL_TTL_SECS + 1));
        // No approval time falls back to creation time.
        assert!(!is_expired(100, None, 100 + APPROVAL_TTL_SECS - 1));
        assert!(is_expired(100, None, 100 + APPROVAL_TTL_SECS + 1));
    }

    #[test]
    fn approval_token_is_long() {
        // 32 random bytes -> 64 hex chars
        let mut buf = [0u8; 32];
        os_random_bytes(&mut buf).unwrap();
        assert_eq!(hex::encode(buf).len(), 64);
    }
}
