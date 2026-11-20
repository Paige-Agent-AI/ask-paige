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

pub struct ProjectState {
    grants: HashMap<String, ProjectGrant>,
    next_id: u64,
}

impl ProjectState {
    pub fn new() -> Self {
        Self {
            grants: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn is_authorized(&self, path: &Path) -> bool {
        let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.grants
            .values()
            .filter(|g| !g.revoked)
            .any(|g| {
                let root = PathBuf::from(&g.root);
                canon == root || canon.starts_with(&root)
            })
    }

    #[allow(dead_code)]
    pub fn authorized_roots(&self) -> Vec<PathBuf> {
        self.grants
            .values()
            .filter(|g| !g.revoked)
            .map(|g| PathBuf::from(&g.root))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Git operations (require authorized project root)
// ---------------------------------------------------------------------------

fn run_git(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
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
    let cwd = validate_git_root(state, root)?;
    let mut args = vec!["diff"];
    if staged {
        args.push("--cached");
    }
    let out = run_git(&cwd, &args)?;
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
    // Validate the worktree path is inside the authorized root.
    if wt_path.exists() {
        let wt_canon = std::fs::canonicalize(&wt_path).map_err(|e| format!("worktree path inaccessible: {e}"))?;
        let root_canon = cwd.clone();
        if !wt_canon.starts_with(&root_canon) && wt_canon != root_canon {
            return Err("worktree path must be inside the authorized project root".to_string());
        }
    } else {
        // For new worktrees, validate the parent directory.
        let parent = wt_path.parent().ok_or("invalid worktree path")?;
        let parent_canon = std::fs::canonicalize(parent)
            .map_err(|e| format!("worktree parent inaccessible: {e}"))?;
        let root_canon = cwd.clone();
        if !parent_canon.starts_with(&root_canon) && parent_canon != root_canon {
            return Err("worktree path must be inside the authorized project root".to_string());
        }
    }

    let mut args = vec!["worktree", "add"];
    if create_branch {
        args.push("-b");
    }
    args.push(branch);
    args.push(&worktree_path);
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

    let mut s = state.lock().unwrap();
    let id = format!("proj-{}", s.next_id);
    s.next_id += 1;
    let grant = ProjectGrant {
        id: id.clone(),
        name: request.name,
        root: display_path(&canon),
        granted_at: now_secs(),
        revoked: false,
    };
    s.grants.insert(id, grant.clone());
    Ok(grant)
}

#[tauri::command]
pub fn project_revoke(
    state: tauri::State<std::sync::Mutex<ProjectState>>,
    id: String,
) -> Result<bool, String> {
    let mut s = state.lock().unwrap();
    if let Some(g) = s.grants.get_mut(&id) {
        g.revoked = true;
        Ok(true)
    } else {
        Err("unknown project grant".to_string())
    }
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
}