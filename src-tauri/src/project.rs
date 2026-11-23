// PAIGE Desktop - Native project authority and Git operations (Increment 1F).
//
// This module provides owner-authorized project root grants and native Git
// operations. It enforces permission at the Rust/OS level, not just UI.
//
// Key requirements:
// - Owner-authorized selection of real source repositories (beyond lab).
// - Per-project grants with immediate revoke.
// - Git repository discovery, branch/status/history/diff, worktree management.
// - Canonical-path resolution with Windows drive/reparse/symlink/junction checks.
// - Worktree boundaries and path traversal prevention.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use std::time::{SystemTime, UNIX_EPOCH};

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

// ---------------------------------------------------------------------------
// Project grants
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectGrant {
    pub id: String,
    pub name: String,
    pub root: String,
    pub granted_at: u64,
    pub revoked: bool,
    #[serde(skip)]
    workflow_only: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectStatus {
    pub id: String,
    pub name: String,
    pub root: String,
    pub is_git_repo: bool,
    pub branch: String,
    pub head: String,
    pub dirty_files: usize,
    pub ahead: u64,
    pub behind: u64,
    pub worktrees: Vec<GitWorktreeInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitWorktreeInfo {
    pub path: String,
    pub branch: String,
    pub head: String,
    pub is_current: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitFileStatus {
    pub path: String,
    pub status: String,
    pub staged: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitLogEntry {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitDiff {
    pub path: String,
    pub additions: usize,
    pub deletions: usize,
    pub patch: String,
}

// ---------------------------------------------------------------------------
// ProjectState
// ---------------------------------------------------------------------------

const PROJECTS_STORE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct ProjectsFile {
    version: u32,
    next_id: u64,
    grants: Vec<ProjectGrant>,
}

fn projects_store_path() -> Result<PathBuf, String> {
    Ok(crate::secrets::config_dir()?.join("projects.json"))
}

fn save_state(state: &ProjectState, path: &Path) -> Result<(), String> {
    let file = ProjectsFile {
        version: PROJECTS_STORE_VERSION,
        next_id: state.next_id,
        grants: state.grants.values().cloned().collect(),
    };
    let json = serde_json::to_vec_pretty(&file)
        .map_err(|e| format!("Could not encode the project list: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    {
        use std::io::Write;
        let mut out = std::fs::File::create(&tmp)
            .map_err(|e| format!("Could not open the project store for writing: {e}"))?;
        out.write_all(&json)
            .map_err(|e| format!("Could not write the project store: {e}"))?;
        out.sync_all().ok();
    }
    std::fs::rename(&tmp, path)
        .map_err(|e| format!("Could not replace the project store: {e}"))
}

fn load_state(path: &Path) -> Option<ProjectState> {
    let bytes = std::fs::read(path).ok()?;
    let file: ProjectsFile = serde_json::from_slice(&bytes).ok()?;
    if file.version != PROJECTS_STORE_VERSION {
        return None;
    }
    let mut grants = HashMap::new();
    for grant in file.grants {
        grants.insert(grant.id.clone(), grant);
    }
    Some(ProjectState { grants, next_id: file.next_id })
}

pub struct ProjectState {
    grants: HashMap<String, ProjectGrant>,
    next_id: u64,
}

impl ProjectState {
    pub fn new() -> Self {
        projects_store_path()
            .ok()
            .and_then(|path| load_state(&path))
            .unwrap_or(Self { grants: HashMap::new(), next_id: 1 })
    }

    fn persist(&self) -> Result<(), String> {
        save_state(self, &projects_store_path()?)
    }

    /// Renames a project's display name. The physical folder is never touched.
    fn rename(&mut self, id: &str, name: &str) -> Result<ProjectGrant, String> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("Enter a project name.".into());
        }
        if name.len() > 80 {
            return Err("Project names are limited to 80 characters.".into());
        }
        let grant = self.grants.get_mut(id).ok_or("unknown project grant")?;
        grant.name = name;
        Ok(grant.clone())
    }

    /// Removes the project entry from PAIGE's list only. The physical folder,
    /// broker authorization state and chat history are intentionally untouched.
    fn remove(&mut self, id: &str) -> Result<(), String> {
        if self.grants.remove(id).is_none() {
            return Err("unknown project grant".to_string());
        }
        Ok(())
    }

    pub fn is_authorized(&self, path: &Path) -> bool {
        let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let canon_display = display_path(&canon);
        self.grants
            .values()
            .filter(|g| !g.revoked && !g.workflow_only)
            .any(|g| {
                let root = display_path(&PathBuf::from(&g.root));
                Path::new(&canon_display).starts_with(Path::new(&root))
            })
    }

    #[allow(dead_code)]
    pub fn authorized_roots(&self) -> Vec<PathBuf> {
        self.grants
            .values()
            .filter(|g| !g.revoked && !g.workflow_only)
            .map(|g| PathBuf::from(&g.root))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Git operations (require authorized project root)
// ---------------------------------------------------------------------------

fn run_git(cwd: &Path, args: &[&str]) -> Result<String, String> {
    git_output(&mut Command::new("git"), cwd, args)
}

fn run_workflow_git(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("git");
    for name in [
        "GIT_DIR", "GIT_COMMON_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_NAMESPACE", "GIT_CONFIG", "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS", "GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM", "GIT_EXTERNAL_DIFF",
        "GIT_CEILING_DIRECTORIES", "GIT_ATTR_SOURCE", "GIT_PREFIX",
    ] {
        command.env_remove(name);
    }
    command.args(["--no-optional-locks", "-c", "core.fsmonitor=false"]);
    git_output(&mut command, cwd, args)
}

fn git_output(command: &mut Command, cwd: &Path, args: &[&str]) -> Result<String, String> {
    let output = command.args(args).current_dir(cwd).output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {}: {}", args.first().unwrap_or(&""), stderr.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn validate_git_root(state: &ProjectState, root: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(root);
    let canon = std::fs::canonicalize(&p)
        .map_err(|e| format!("path is inaccessible: {e}"))?;
    if !state.is_authorized(&canon) {
        return Err("project root is not authorized".to_string());
    }
    if !canon.is_dir() {
        return Err("path is not a directory".to_string());
    }
    Ok(canon)
}

pub(crate) fn verify_pinned_git_root(root: &str) -> Result<(), String> {
    let expected = std::fs::canonicalize(root).map_err(|error| error.to_string())?;
    let top = run_git(&expected, &["rev-parse", "--show-toplevel"])?;
    let actual = std::fs::canonicalize(top.trim()).map_err(|error| error.to_string())?;
    if actual != expected {
        return Err("Pin the Git worktree itself before running Git tools in this chat.".into());
    }
    Ok(())
}

pub fn git_status(state: &ProjectState, root: &str) -> Result<Vec<GitFileStatus>, String> {
    let cwd = validate_git_root(state, root)?;
    let out = run_git(&cwd, &["status", "--porcelain"])?;
    let mut files = Vec::new();
    for line in out.lines() {
        if line.len() < 4 {
            continue;
        }
        let status = &line[..2];
        let path = line[3..].to_string();
        let staged = !status.starts_with(' ') && !status.starts_with('?') && !status.starts_with('!');
        files.push(GitFileStatus {
            path,
            status: status.trim().to_string(),
            staged,
        });
    }
    Ok(files)
}

pub fn git_branch_info(state: &ProjectState, root: &str) -> Result<(String, String, u64, u64), String> {
    let cwd = validate_git_root(state, root)?;
    let branch = run_git(&cwd, &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string();
    let head = run_git(&cwd, &["rev-parse", "HEAD"])?.trim().to_string();
    let (ahead, behind) = match run_git(&cwd, &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"]) {
        Ok(out) => {
            let parts: Vec<&str> = out.trim().split('\t').collect();
            if parts.len() == 2 {
                (parts[0].parse().unwrap_or(0), parts[1].parse().unwrap_or(0))
            } else {
                (0, 0)
            }
        }
        Err(_) => (0, 0), // no upstream
    };
    Ok((branch, head, ahead, behind))
}

pub fn git_log(state: &ProjectState, root: &str, limit: usize) -> Result<Vec<GitLogEntry>, String> {
    let cwd = validate_git_root(state, root)?;
    let format = "%H|%an|%ai|%s";
    let out = run_git(&cwd, &["log", &format!("--pretty=format:{format}"), &format!("-{limit}")])?;
    let mut entries = Vec::new();
    for line in out.lines() {
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        if parts.len() == 4 {
            entries.push(GitLogEntry {
                hash: parts[0].to_string(),
                author: parts[1].to_string(),
                date: parts[2].to_string(),
                message: parts[3].to_string(),
            });
        }
    }
    Ok(entries)
}

pub fn git_diff(state: &ProjectState, root: &str, staged: bool) -> Result<Vec<GitDiff>, String> {
    git_diff_with_policy(state, root, staged, false)
}

pub(crate) fn workflow_diff(state: &ProjectState, root: &str) -> Result<Vec<GitDiff>, String> {
    git_diff_with_policy(state, root, false, true)
}

fn git_diff_with_policy(state: &ProjectState, root: &str, staged: bool, workflow: bool) -> Result<Vec<GitDiff>, String> {
    let cwd = if workflow { validate_workflow_git_root(state, root)? } else { validate_git_root(state, root)? };
    let mut args = vec!["diff"];
    if workflow {
        args.extend(["--no-ext-diff", "--no-textconv"]);
    }
    if staged {
        args.push("--cached");
    }
    let out = if workflow { run_workflow_git(&cwd, &args)? } else { run_git(&cwd, &args)? };
    let mut diffs = Vec::new();
    let mut current_path = String::new();
    let mut current_patch = String::new();
    let mut additions = 0usize;
    let mut deletions = 0usize;

    for line in out.lines() {
        if line.starts_with("diff --git") {
            if !current_path.is_empty() {
                diffs.push(GitDiff {
                    path: current_path.clone(),
                    additions,
                    deletions,
                    patch: current_patch.clone(),
                });
            }
            // Parse: diff --git a/path b/path
            if let Some(pos) = line.rfind(" b/") {
                current_path = line[pos + 3..].to_string();
            }
            current_patch = line.to_string();
            additions = 0;
            deletions = 0;
        } else {
            current_patch.push('\n');
            current_patch.push_str(line);
            if line.starts_with('+') && !line.starts_with("+++") {
                additions += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                deletions += 1;
            }
        }
    }
    if !current_path.is_empty() {
        diffs.push(GitDiff {
            path: current_path,
            additions,
            deletions,
            patch: current_patch,
        });
    }
    Ok(diffs)
}

pub fn git_branches(state: &ProjectState, root: &str) -> Result<Vec<String>, String> {
    let cwd = validate_git_root(state, root)?;
    let out = run_git(&cwd, &["branch", "--list"])?;
    Ok(out
        .lines()
        .map(|l| l.trim_start_matches('*').trim().to_string())
        .collect())
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub(crate) struct WorktreeIdentity {
    git_dir: PathBuf,
    common_dir: PathBuf,
    head: String,
}

fn validate_workflow_git_root(state: &ProjectState, root: &str) -> Result<PathBuf, String> {
    let canon = std::fs::canonicalize(root).map_err(|error| format!("Path is inaccessible: {error}"))?;
    let display = display_path(&canon);
    let selected = state.grants.values().any(|grant| !grant.revoked && grant.root.eq_ignore_ascii_case(&display));
    if !selected && !state.is_authorized(&canon) {
        return Err("Project root is not authorized.".into());
    }
    if !canon.is_dir() {
        return Err("Project root is not a directory.".into());
    }
    Ok(canon)
}

pub(crate) fn linked_worktree_identity(state: &ProjectState, root: &str) -> Result<WorktreeIdentity, String> {
    let cwd = validate_workflow_git_root(state, root)
        .map_err(|error| format!("Authorize the worktree in Projects before continuing Engineering. {error}"))?;
    let identity = run_workflow_git(&cwd, &[
        "rev-parse", "--path-format=absolute", "--show-toplevel", "--absolute-git-dir", "--git-common-dir", "HEAD",
    ])?;
    let fields: Vec<_> = identity.lines().collect();
    let [top, git_dir, common_dir, head] = fields.as_slice() else {
        return Err("Could not identify the selected Git worktree.".into());
    };
    let canonical = |path: &str| std::fs::canonicalize(path).map_err(|error| error.to_string());
    let git_dir = canonical(git_dir)?;
    let common_dir = canonical(common_dir)?;
    if canonical(top)? != cwd || git_dir == common_dir {
        return Err("Choose an existing linked Git worktree in this chat before using Engineering.".into());
    }
    Ok(WorktreeIdentity { git_dir, common_dir, head: (*head).into() })
}

pub fn git_worktree_list(state: &ProjectState, root: &str) -> Result<Vec<GitWorktreeInfo>, String> {
    let cwd = validate_git_root(state, root)?;
    let out = run_git(&cwd, &["worktree", "list", "--porcelain"])?;
    let mut worktrees = Vec::new();
    let mut current_path = String::new();
    let mut current_branch = String::new();
    let mut current_head = String::new();

    for line in out.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            if !current_path.is_empty() {
                worktrees.push(GitWorktreeInfo {
                    path: current_path.clone(),
                    branch: current_branch.clone(),
                    head: current_head.clone(),
                    is_current: current_path == display_path(&cwd),
                });
            }
            current_path = rest.to_string();
            current_branch = String::new();
            current_head = String::new();
        } else if let Some(rest) = line.strip_prefix("branch ") {
            current_branch = rest.trim_start_matches("refs/heads/").to_string();
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            current_head = rest.to_string();
        }
    }
    if !current_path.is_empty() {
        worktrees.push(GitWorktreeInfo {
            path: current_path.clone(),
            branch: current_branch.clone(),
            head: current_head.clone(),
            is_current: current_path == display_path(&cwd),
        });
    }
    Ok(worktrees)
}

pub fn git_create_worktree(
    state: &ProjectState,
    root: &str,
    worktree_path: &str,
    branch: &str,
    create_branch: bool,
) -> Result<String, String> {
    let cwd = validate_git_root(state, root)?;
    let wt_path = PathBuf::from(worktree_path);
    if !wt_path.is_absolute() || wt_path.components().any(|part| part == std::path::Component::ParentDir) {
        return Err("worktree path must be absolute and inside the authorized project root".into());
    }
    // Validate the worktree path is inside the authorized root.
    if wt_path.exists() {
        let wt_canon = std::fs::canonicalize(&wt_path).map_err(|e| format!("worktree path inaccessible: {e}"))?;
        let root_display = display_path(&cwd);
        let wt_display = display_path(&wt_canon);
        if !Path::new(&wt_display).starts_with(Path::new(&root_display)) {
            return Err("worktree path must be inside the authorized project root".to_string());
        }
    } else {
        let parent = wt_path.parent().ok_or("invalid worktree path")?;
        let ancestor = parent.ancestors().find(|p| p.is_dir()).ok_or("invalid worktree parent")?;
        let parent_canon = std::fs::canonicalize(ancestor)
            .map_err(|e| format!("worktree parent inaccessible: {e}"))?;
        let root_display = display_path(&cwd);
        let parent_display = display_path(&parent_canon);
        if !Path::new(&parent_display).starts_with(Path::new(&root_display)) {
            return Err("worktree path must be inside the authorized project root".to_string());
        }
        std::fs::create_dir_all(parent).map_err(|e| format!("cannot create worktree parent: {e}"))?;
    }

    let mut args = vec!["worktree", "add"];
    if create_branch {
        args.extend(["-b", branch]);
    }
    args.extend(["--", worktree_path]);
    if !create_branch {
        args.push(branch);
    }
    let out = run_git(&cwd, &args)?;
    Ok(out.trim().to_string())
}

pub fn git_remove_worktree(
    state: &ProjectState,
    root: &str,
    worktree_path: &str,
) -> Result<String, String> {
    let cwd = validate_git_root(state, root)?;
    let out = run_git(&cwd, &["worktree", "remove", worktree_path])?;
    Ok(out.trim().to_string())
}

pub fn git_create_branch(
    state: &ProjectState,
    root: &str,
    branch: &str,
    from: Option<&str>,
) -> Result<String, String> {
    let cwd = validate_git_root(state, root)?;
    let mut args = vec!["checkout", "-b", branch];
    if let Some(f) = from {
        args.push(f);
    }
    let out = run_git(&cwd, &args)?;
    Ok(out.trim().to_string())
}

pub fn git_checkout(
    state: &ProjectState,
    root: &str,
    branch: &str,
) -> Result<String, String> {
    let cwd = validate_git_root(state, root)?;
    let out = run_git(&cwd, &["checkout", branch])?;
    Ok(out.trim().to_string())
}

pub fn git_add(
    state: &ProjectState,
    root: &str,
    paths: &[String],
) -> Result<String, String> {
    let cwd = validate_git_root(state, root)?;
    let mut args = vec!["add"];
    for p in paths {
        args.push(p);
    }
    let out = run_git(&cwd, &args)?;
    Ok(out.trim().to_string())
}

pub fn git_commit(
    state: &ProjectState,
    root: &str,
    message: &str,
) -> Result<String, String> {
    let cwd = validate_git_root(state, root)?;
    let out = run_git(&cwd, &["commit", "-m", message])?;
    Ok(out.trim().to_string())
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[derive(Clone, Deserialize)]
pub struct ProjectGrantRequest {
    pub name: String,
    pub root: String,
}

#[tauri::command]
pub fn project_grant(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    request: ProjectGrantRequest,
) -> Result<ProjectGrant, String> {
    let root = PathBuf::from(&request.root);
    let canon = std::fs::canonicalize(&root)
        .map_err(|e| format!("path is inaccessible: {e}"))?;
    if !canon.is_dir() {
        return Err("path is not a directory".to_string());
    }
    // Check if it's a git repository.
    let is_git = canon.join(".git").exists() || run_git(&canon, &["rev-parse", "--git-dir"]).is_ok();
    if !is_git {
        return Err("path is not a Git repository".to_string());
    }

    let (grant, saved) = {
        let mut s = state.lock().unwrap();
        let grant = register_project(&mut s, request.name, display_path(&canon), false);
        let saved = s.persist();
        (grant, saved)
    };
    saved?;
    Ok(grant)
}

fn register_project(state: &mut ProjectState, name: String, root: String, workflow_only: bool) -> ProjectGrant {
    if let Some(grant) = state.grants.values_mut().find(|grant| grant.root.eq_ignore_ascii_case(&root)) {
        grant.name = name;
        grant.granted_at = now_secs();
        if grant.revoked {
            grant.workflow_only = workflow_only;
        } else {
            grant.workflow_only &= workflow_only;
        }
        grant.revoked = false;
        return grant.clone();
    }
    let id = format!("proj-{}", state.next_id);
    state.next_id += 1;
    let grant = ProjectGrant {
        id: id.clone(),
        name,
        root,
        granted_at: now_secs(),
        revoked: false,
        workflow_only,
    };
    state.grants.insert(id, grant.clone());
    grant
}

#[tauri::command]
pub async fn project_pick_folder(window: tauri::WebviewWindow) -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        let owner = window.hwnd().map_err(|error| error.to_string())?.0 as usize;
        tauri::async_runtime::spawn_blocking(move || pick_project_folder(owner)).await
            .map_err(|_| "The folder picker was interrupted.".to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        Err("The native project folder picker is available on Windows.".into())
    }
}

#[cfg(windows)]
fn pick_project_folder(owner: usize) -> Result<Option<String>, String> {
    use windows::Win32::{
        Foundation::{ERROR_CANCELLED, HWND},
        System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize},
        UI::Shell::{FOS_DONTADDTORECENT, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH},
    };
    use windows::core::{HRESULT, w};

    static ACTIVE_PICKER: Mutex<()> = Mutex::new(());
    let _picker = ACTIVE_PICKER.try_lock().map_err(|_| "A project folder picker is already open.")?;

    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            // This guard is constructed after successful initialization on this thread.
            unsafe { CoUninitialize() };
        }
    }

    // The apartment and all dialog interfaces stay on the blocking worker thread.
    let choose = || -> windows::core::Result<Option<String>> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let _apartment = Apartment;
            let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            dialog.SetOptions(dialog.GetOptions()? | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_DONTADDTORECENT)?;
            dialog.SetTitle(w!("Choose a PAIGE project folder"))?;
            dialog.SetOkButtonLabel(w!("Select folder"))?;
            match dialog.Show(Some(HWND(owner as *mut _))) {
                Err(error) if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => return Ok(None),
                result => result?,
            }
            let item = dialog.GetResult()?;
            let path = item.GetDisplayName(SIGDN_FILESYSPATH)?;
            let result = path.to_string();
            CoTaskMemFree(Some(path.0.cast()));
            Ok(Some(result?))
        }
    };
    choose().map_err(|error| format!("Could not browse for a project folder. {error}"))
}

fn selected_project_path(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err("Enter the full path to a local project folder.".into());
    }
    #[cfg(windows)]
    if !matches!(path.components().next(), Some(std::path::Component::Prefix(prefix)) if
        matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_))) {
        return Err("Choose a local project folder.".into());
    }
    let canonical = std::fs::canonicalize(path).map_err(|error| format!("Cannot open that folder. {error}"))?;
    let display = display_path(&canonical);
    if display.starts_with("UNC\\") || display.starts_with("\\\\") {
        return Err("Choose a local project folder.".into());
    }
    if canonical.parent().is_none() || std::env::var_os("USERPROFILE").and_then(|profile|
        std::fs::canonicalize(profile).ok()).is_some_and(|profile| profile.starts_with(&canonical)) {
        return Err("Choose a specific project folder inside the drive or user profile.".into());
    }
    if display.encode_utf16().count() > 650 || display.chars().any(|ch|
        ch.is_control() || matches!(ch, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')) {
        return Err("This folder path cannot be displayed safely in the approval dialog.".into());
    }
    if !canonical.is_dir() {
        return Err("Select a folder rather than a file.".into());
    }
    Ok(canonical)
}

fn confirm_project_access(owner: usize, path: &str) -> Result<bool, String> {
    use windows::{
        Win32::{
            Foundation::HWND,
            UI::Controls::{
                TASKDIALOG_BUTTON, TASKDIALOGCONFIG, TDCBF_CANCEL_BUTTON, TDF_ALLOW_DIALOG_CANCELLATION,
                TDF_POSITION_RELATIVE_TO_WINDOW, TaskDialogIndirect,
            },
        },
        core::{BOOL, PCWSTR, w},
    };

    const ALLOW: i32 = 100;
    const CANCEL: i32 = 2;
    let content: Vec<u16> = format!(
        "{path}\n\nPAIGE can read this folder and propose edits. Each file edit still requires separate approval.\n\n\
        Remembered access is available when PAIGE reopens this folder. You can revoke it in Projects."
    ).encode_utf16().chain(std::iter::once(0)).collect();
    let allow = TASKDIALOG_BUTTON { nButtonID: ALLOW, pszButtonText: w!("Allow access") };
    let dialog = TASKDIALOGCONFIG {
        cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
        hwndParent: HWND(owner as *mut _),
        dwFlags: TDF_ALLOW_DIALOG_CANCELLATION | TDF_POSITION_RELATIVE_TO_WINDOW,
        dwCommonButtons: TDCBF_CANCEL_BUTTON,
        pszWindowTitle: w!("PAIGE Desktop - Authorize project folder"),
        pszMainInstruction: w!("Allow access to this project?"),
        pszContent: PCWSTR(content.as_ptr()),
        cButtons: 1,
        pButtons: &allow,
        nDefaultButton: CANCEL,
        pszVerificationText: w!("Remember this project"),
        ..Default::default()
    };

    let mut button = 0;
    let mut checked = BOOL(0);
    // Inputs and output pointers live until the modal dialog returns.
    unsafe { TaskDialogIndirect(&dialog, Some(&mut button), None, Some(&mut checked)) }
        .map_err(|error| format!("Could not open the native project approval dialog: {error}"))?;
    if button != ALLOW {
        return Err("Project folder authorization was declined.".into());
    }
    Ok(checked.as_bool())
}

pub(crate) fn register_open_folder(app: &AppHandle, canonical: &Path, path: &str) {
    let is_git_root = run_workflow_git(canonical, &["rev-parse", "--show-toplevel"]).ok()
        .and_then(|top| std::fs::canonicalize(top.trim()).ok()).as_deref() == Some(canonical);
    if is_git_root {
        let name = canonical.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let project = app.state::<Mutex<ProjectState>>();
        register_project(&mut project.lock().unwrap(), name, path.into(), true);
    }
}

#[tauri::command]
pub async fn project_open_folder(
    app: AppHandle, window: tauri::WebviewWindow, root: String, remember_access: Option<bool>,
) -> Result<crate::broker::BrokerStatus, String> {
    let owner = window.hwnd().map_err(|error| error.to_string())?.0 as usize;
    tauri::async_runtime::spawn_blocking(move || {
        let _turn = crate::chats::ChatsStore::lock_turn()?;
        let canonical = selected_project_path(&root)?;
        let folder = crate::broker::hold_project_directory(&canonical)?;
        let path = display_path(&canonical);
        let broker = app.state::<Mutex<crate::broker::BrokerState>>();
        let reuse = crate::broker::select_session_project(&app, broker.clone(), &path, true)?;
        if reuse {
            let status = crate::broker::broker_status(broker.clone());
            if !remember_access.unwrap_or(false) || status.remembered_roots.contains(&path) {
                register_open_folder(&app, &canonical, &path);
                return Ok(status);
            }
        }

        // Only the native checkbox can turn today's access into remembered access.
        let remember = if crate::broker::uses_test_confirmation() {
            // Exercise restart recovery against an isolated test profile only.
            remember_access == Some(true) && crate::project_access::can_remember_test_project(&path)
        } else { confirm_project_access(owner, &path)? };
        let status = crate::broker::activate_project_folder(broker, canonical.clone(), folder, remember)?;
        register_open_folder(&app, &canonical, &path);
        Ok(status)
    }).await.map_err(|_| "Opening the project was interrupted.".to_string())?
}

#[tauri::command]
pub fn project_revoke(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    id: String,
) -> Result<bool, String> {
    let mut s = state.lock().unwrap();
    if let Some(g) = s.grants.get_mut(&id) {
        g.revoked = true;
        let revoked = s.persist();
        revoked?;
        Ok(true)
    } else {
        Err("unknown project grant".to_string())
    }
}

#[tauri::command]
pub fn project_rename(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    id: String,
    name: String,
) -> Result<ProjectGrant, String> {
    let mut s = state.lock().unwrap();
    let grant = s.rename(&id, &name)?;
    s.persist()?;
    Ok(grant)
}

#[tauri::command]
pub fn project_remove(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    id: String,
) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    // Removing a project from PAIGE only drops the list entry: the Windows
    // folder, broker grants and saved conversations are deliberately untouched.
    s.remove(&id)?;
    s.persist()
}

#[tauri::command]
pub fn project_list(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
) -> Vec<ProjectGrant> {
    let s = state.lock().unwrap();
    s.grants.values().cloned().collect()
}

#[tauri::command]
pub fn project_status(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    id: String,
) -> Result<ProjectStatus, String> {
    let s = state.lock().unwrap();
    let grant = s.grants.get(&id).ok_or("unknown project grant")?;
    if grant.revoked {
        return Err("project grant is revoked".to_string());
    }
    let root = grant.root.clone();

    let is_git = run_git(Path::new(&root), &["rev-parse", "--git-dir"]).is_ok();
    let (branch, head, ahead, behind) = if is_git {
        git_branch_info(&s, &root)?
    } else {
        (String::new(), String::new(), 0, 0)
    };
    let dirty_files = if is_git {
        git_status(&s, &root)?.len()
    } else {
        0
    };
    let worktrees = if is_git {
        git_worktree_list(&s, &root).unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(ProjectStatus {
        id: grant.id.clone(),
        name: grant.name.clone(),
        root: grant.root.clone(),
        is_git_repo: is_git,
        branch,
        head,
        dirty_files,
        ahead,
        behind,
        worktrees,
    })
}

#[tauri::command]
pub fn git_files(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    root: String,
) -> Result<Vec<GitFileStatus>, String> {
    let s = state.lock().unwrap();
    git_status(&s, &root)
}

#[tauri::command]
pub fn git_log_cmd(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    root: String,
    limit: usize,
) -> Result<Vec<GitLogEntry>, String> {
    let s = state.lock().unwrap();
    git_log(&s, &root, limit)
}

#[tauri::command]
pub fn git_diff_cmd(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    root: String,
    staged: bool,
) -> Result<Vec<GitDiff>, String> {
    let s = state.lock().unwrap();
    git_diff(&s, &root, staged)
}

#[tauri::command]
pub fn git_branches_cmd(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    root: String,
) -> Result<Vec<String>, String> {
    let s = state.lock().unwrap();
    git_branches(&s, &root)
}

#[tauri::command]
pub fn git_worktrees_cmd(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    root: String,
) -> Result<Vec<GitWorktreeInfo>, String> {
    let s = state.lock().unwrap();
    git_worktree_list(&s, &root)
}

#[tauri::command]
pub fn git_create_worktree_cmd(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    root: String,
    worktree_path: String,
    branch: String,
    create_branch: bool,
) -> Result<String, String> {
    let s = state.lock().unwrap();
    git_create_worktree(&s, &root, &worktree_path, &branch, create_branch)
}

#[tauri::command]
pub fn git_remove_worktree_cmd(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    root: String,
    worktree_path: String,
) -> Result<String, String> {
    let s = state.lock().unwrap();
    git_remove_worktree(&s, &root, &worktree_path)
}

#[tauri::command]
pub fn git_create_branch_cmd(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    root: String,
    branch: String,
    from: Option<String>,
) -> Result<String, String> {
    let s = state.lock().unwrap();
    git_create_branch(&s, &root, &branch, from.as_deref())
}

#[tauri::command]
pub fn git_checkout_cmd(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    root: String,
    branch: String,
) -> Result<String, String> {
    let s = state.lock().unwrap();
    git_checkout(&s, &root, &branch)
}

#[tauri::command]
pub fn git_add_cmd(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    root: String,
    paths: Vec<String>,
) -> Result<String, String> {
    let s = state.lock().unwrap();
    git_add(&s, &root, &paths)
}

#[tauri::command]
pub fn git_commit_cmd(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    root: String,
    message: String,
) -> Result<String, String> {
    let s = state.lock().unwrap();
    git_commit(&s, &root, &message)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_state_is_authorized_rejects_unauthorized() {
        let state = ProjectState::new();
        let tmp = std::env::temp_dir().join("paige-project-test-unauthorized");
        assert!(!state.is_authorized(&tmp));
    }

    #[test]
    fn project_state_authorized_roots_empty() {
        let state = ProjectState::new();
        assert!(state.authorized_roots().is_empty());
    }

    #[test]
    fn rejects_a_sibling_whose_name_starts_with_an_authorized_root() {
        let mut state = ProjectState::new();
        let base = std::env::temp_dir();
        state.grants.insert("fixture".into(), ProjectGrant {
            id: "fixture".into(), name: "Fixture".into(),
            root: display_path(&base.join("paige-boundary-project")), granted_at: 0, revoked: false,
            workflow_only: false,
        });
        assert!(!state.is_authorized(&base.join("paige-boundary-project-other")));
    }

    #[test]
    fn validate_git_root_rejects_unauthorized() {
        let state = ProjectState::new();
        let tmp = std::env::temp_dir();
        let tmp_str = tmp.to_string_lossy().to_string();
        assert!(validate_git_root(&state, &tmp_str).is_err());
    }

    #[test]
    fn run_git_returns_error_for_bad_command() {
        let tmp = std::env::temp_dir();
        assert!(run_git(&tmp, &["--invalid-flag"]).is_err());
    }

    #[test]
    fn project_store_round_trips_grants() {
        let path = std::env::temp_dir().join("paige-project-store-roundtrip.json");
        let _ = std::fs::remove_file(&path);
        let mut state = ProjectState { grants: HashMap::new(), next_id: 7 };
        state.grants.insert("proj-7".into(), ProjectGrant {
            id: "proj-7".into(), name: "My Project".into(),
            root: "C:\\repo".into(), granted_at: 42, revoked: false, workflow_only: false,
        });
        save_state(&state, &path).unwrap();
        let loaded = load_state(&path).expect("store should load");
        assert_eq!(loaded.next_id, 7);
        assert_eq!(loaded.grants["proj-7"].name, "My Project");
        assert_eq!(loaded.grants["proj-7"].root, "C:\\repo");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn project_store_rejects_unsupported_versions_without_deleting_them() {
        let path = std::env::temp_dir().join("paige-project-store-version.json");
        std::fs::write(&path, br#"{"version":99,"next_id":2,"grants":[]}"#).unwrap();
        assert!(load_state(&path).is_none());
        assert!(path.exists(), "an unsupported store must not be deleted");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn project_rename_only_changes_the_display_name() {
        let mut state = ProjectState { grants: HashMap::new(), next_id: 1 };
        state.grants.insert("proj-1".into(), ProjectGrant {
            id: "proj-1".into(), name: "Old Name".into(),
            root: "C:\\repo".into(), granted_at: 1, revoked: false, workflow_only: false,
        });
        let grant = state.rename("proj-1", "  New Name  ").unwrap();
        assert_eq!(grant.name, "New Name");
        assert_eq!(state.grants["proj-1"].root, "C:\\repo", "rename must not move the folder");
        assert_eq!(state.grants["proj-1"].granted_at, 1, "rename must not re-grant authority");
        assert!(state.rename("proj-1", "   ").is_err(), "blank names are rejected");
    }

    #[test]
    fn project_remove_deletes_only_the_list_entry() {
        let mut state = ProjectState { grants: HashMap::new(), next_id: 1 };
        state.grants.insert("proj-1".into(), ProjectGrant {
            id: "proj-1".into(), name: "Keep On Disk".into(),
            root: "C:\\repo".into(), granted_at: 1, revoked: false, workflow_only: false,
        });
        state.remove("proj-1").unwrap();
        assert!(!state.grants.contains_key("proj-1"), "the PAIGE list entry is removed");
        assert!(state.remove("proj-1").is_err(), "removing twice is an error");
    }
}
