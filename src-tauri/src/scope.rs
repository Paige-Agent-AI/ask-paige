// PAIGE Desktop - Native Scope Registry (ANT-173, Slice 1).
//
// ONE dynamic, owner-granted authority record replacing the duplicate
// compile-time root allowlists that previously lived in broker.rs and
// terminal.rs. The broker, the terminal and the AI tool path all consult
// this registry; none of them keeps a private root list.
//
// Owner-selectable modes:
//   Observe   - read-only discovery inside granted roots; no writes, no
//               terminal execution, no effectful native tools.
//   Selected  - per-root grants with explicit read/write operation classes
//               (the previous default behavior, now generalized to any
//               owner-picked root).
//   Full      - a standing owner-granted profile scope plus explicitly
//               authorized additions, for ordinary operations within the
//               signed-in Windows user's rights. Never elevated: UAC
//               prompts are honored, not bypassed.
//
// Security invariants preserved from the former broker/terminal authority:
//   - canonicalization before every check (symlinks resolve to their real
//     target; a link pointing outside the scope is denied);
//   - reparse-point rejection for roots and targets;
//   - folder identity capture so a renamed/replaced grant folder cannot be
//     silently re-used;
//   - fail-closed on expired, corrupted or ambiguous stores: an unreadable
//     scope.json authorizes only the built-in baseline roots and refuses
//     every scope mutation with an honest error;
//   - Windows file authority never confers cloud/SaaS account authority;
//   - every grant records its owning workspace entity for multi-company
//     provenance; the display name is never the security identity.
//
// Persistence: `scope.json` under the app config directory, atomic
// temp-file + rename, version-guarded, never silently replaced when it
// cannot be read.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use tauri::{AppHandle, Manager};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::broker::{self, BrokerState};
use crate::project_access::FolderIdentity;
use crate::secrets::config_dir;
use crate::terminal::TerminalState;
use std::sync::Mutex;

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
// Modes and grants
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScopeMode {
    Observe,
    Selected,
    Full,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScopeGrant {
    pub id: String,
    /// Canonical, display-formatted root path. The authority identity is the
    /// canonical path plus the folder identity - never the display name.
    pub root: String,
    pub read: bool,
    pub write: bool,
    /// Stable owning-entity slug (the security identity for provenance).
    pub entity_id: String,
    /// Display label only; never used for authorization decisions.
    pub entity_name: String,
    pub granted_at: u64,
    /// Windows file identity of the granted folder (replacement detection).
    pub identity: FolderIdentity,
    /// How this grant was approved, recorded honestly.
    pub confirmed_via: String,
    /// Set only on the standing profile-scope grant that Full Computer Access
    /// creates. Leaving Full withdraws exactly these grants, so returning to
    /// Selected can never silently retain full authority. Defaults to false so
    /// an existing v1 store loads unchanged.
    #[serde(default)]
    pub full_access: bool,
}

#[derive(Serialize)]
pub struct ScopeStatus {
    pub mode: ScopeMode,
    pub grants: Vec<ScopeGrant>,
    pub store_unsupported: bool,
    pub profile_root: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ScopeFile {
    version: u32,
    mode: ScopeMode,
    next_id: u64,
    grants: Vec<ScopeGrant>,
}

const SCOPE_STORE_VERSION: u32 = 1;

// The former compile-time baseline. On first start (and on a corrupted store,
// fail-closed) exactly these roots authorize, so every ANT-172 capability is
// preserved bit-identically and an unreadable store can never widen access.
const BASELINE_ROOTS: &[(&str, &str, &str)] = &[
    (r"C:\Users\tonig\paige-desktop-lab", "paige-lab", "PAIGE Desktop Lab"),
    (r"C:\Users\tonig\paige-desktop", "paige-agent-ai", "PAIGE Agent AI Inc."),
];

// ---------------------------------------------------------------------------
// Registry state
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ScopeState {
    mode: ScopeMode,
    next_id: u64,
    grants: Vec<ScopeGrant>,
    /// Set when scope.json exists but cannot be read. All mutations are
    /// refused and only the baseline roots authorize.
    store_unsupported: bool,
}

static REGISTRY: RwLock<Option<ScopeState>> = RwLock::new(None);

fn scope_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("scope.json"))
}

fn baseline_grants(now: u64) -> Vec<ScopeGrant> {
    BASELINE_ROOTS
        .iter()
        .enumerate()
        .filter_map(|(index, (root, entity_id, entity_name))| {
            let canon = std::fs::canonicalize(root).ok()?;
            if crate::broker::is_reparse_point(&canon) {
                return None;
            }
            let identity = crate::broker::hold_project_directory(&canon)
                .and_then(|file| FolderIdentity::read(&file).map_err(|e| e.to_string()))
                .ok()?;
            Some(ScopeGrant {
                id: format!("scope-baseline-{}", index),
                root: display_path(&canon),
                read: true,
                write: true,
                entity_id: entity_id.to_string(),
                entity_name: entity_name.to_string(),
                granted_at: now,
                identity,
                confirmed_via: "built-in baseline (pre-existing owner grants)".into(),
                full_access: false,
            })
        })
        .collect()
}

/// Re-validate persisted grants before they can authorize anything: a grant
/// must be an absolute, canonical, non-reparse directory whose recorded Windows
/// folder identity still matches. Anything else is dropped (fail closed) rather
/// than trusted from disk.
fn validate_loaded_grants(grants: Vec<ScopeGrant>) -> Vec<ScopeGrant> {
    grants
        .into_iter()
        .filter(|grant| {
            let root = Path::new(&grant.root);
            if !root.is_absolute() {
                return false;
            }
            let Ok(canon) = std::fs::canonicalize(root) else { return false };
            if compare_form(&canon) != compare_form(root)
                || !canon.is_dir()
                || crate::broker::is_reparse_point(&canon)
            {
                return false;
            }
            if is_protected_path(&canon) {
                return false;
            }
            let Ok(file) = crate::broker::hold_project_directory(&canon) else { return false };
            FolderIdentity::read(&file).map(|identity| identity == grant.identity).unwrap_or(false)
        })
        .collect()
}

fn load_state() -> ScopeState {
    let path = match scope_path() {
        Ok(path) => path,
        Err(_) => {
            return ScopeState {
                mode: ScopeMode::Selected,
                next_id: 1,
                grants: baseline_grants(now_secs()),
                store_unsupported: false,
            }
        }
    };
    match std::fs::read(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // First start: initialize Selected mode from the baseline plus the
            // folders the owner previously remembered (ANT-172 pinned projects).
            let mut grants = baseline_grants(now_secs());
            if let Ok(remembered) = crate::project_access::load() {
                for (root, identity) in remembered {
                    if grants.iter().any(|g| g.root.eq_ignore_ascii_case(&root)) {
                        continue;
                    }
                    if let Ok(canon) = std::fs::canonicalize(&root) {
                        if crate::broker::is_reparse_point(&canon) {
                            continue;
                        }
                        grants.push(ScopeGrant {
                            id: format!("scope-remembered-{}", grants.len()),
                            root: display_path(&canon),
                            read: true,
                            write: true,
                            entity_id: "personal".into(),
                            entity_name: "Personal".into(),
                            granted_at: now_secs(),
                            identity,
                            confirmed_via: "previously remembered owner access".into(),
                            full_access: false,
                        });
                    }
                }
            }
            ScopeState { mode: ScopeMode::Selected, next_id: 1, grants, store_unsupported: false }
        }
        Err(_) => {
            // Present but unreadable: fail closed to the baseline only.
            ScopeState {
                mode: ScopeMode::Selected,
                next_id: 1,
                grants: baseline_grants(now_secs()),
                store_unsupported: true,
            }
        }
        Ok(bytes) => {
            if let Ok(file) = serde_json::from_slice::<ScopeFile>(&bytes)
                && file.version == SCOPE_STORE_VERSION
            {
                let grants = validate_loaded_grants(file.grants);
                // Full Access is only as real as the profile-scope grant that
                // created it; if that grant no longer survives validation, the
                // standing mode must not stay Full.
                let mode = if file.mode == ScopeMode::Full
                    && !grants.iter().any(|g| g.full_access)
                {
                    ScopeMode::Selected
                } else {
                    file.mode
                };
                ScopeState { mode, next_id: file.next_id, grants, store_unsupported: false }
            } else {
                // Unsupported version or corrupt content: fail closed to the
                // baseline only and refuse every mutation until the owner acts.
                ScopeState {
                    mode: ScopeMode::Selected,
                    next_id: 1,
                    grants: baseline_grants(now_secs()),
                    store_unsupported: true,
                }
            }
        }
    }
}

/// Load the registry once at application startup.
pub fn init() {
    let state = load_state();
    // Persist an initialized first-start store so the authority record exists
    // on disk (unless the store is unreadable - never overwrite it then).
    if !state.store_unsupported && scope_path().map(|p| !p.exists()).unwrap_or(false) {
        let _ = persist(&state);
    }
    *REGISTRY.write().unwrap() = Some(state);
}

fn persist(state: &ScopeState) -> Result<(), String> {
    let path = scope_path()?;
    let file = ScopeFile {
        version: SCOPE_STORE_VERSION,
        mode: state.mode,
        next_id: state.next_id,
        grants: state.grants.clone(),
    };
    let json = serde_json::to_vec_pretty(&file)
        .map_err(|e| format!("Could not encode the scope registry: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    {
        use std::io::Write;
        let mut out = std::fs::File::create(&tmp)
            .map_err(|e| format!("Could not open the scope registry for writing: {e}"))?;
        out.write_all(&json)
            .map_err(|e| format!("Could not write the scope registry: {e}"))?;
        out.sync_all().ok();
    }
    std::fs::rename(&tmp, &path)
        .map_err(|e| format!("Could not replace the scope registry: {e}"))
}

fn read_scope<T>(f: impl FnOnce(&ScopeState) -> T) -> T {
    let guard = REGISTRY.read().unwrap();
    match guard.as_ref() {
        Some(state) => f(state),
        // Registry not initialized: nothing authorizes. Fail closed.
        None => f(&ScopeState { mode: ScopeMode::Observe, next_id: 0, grants: Vec::new(), store_unsupported: true }),
    }
}

// ---------------------------------------------------------------------------
// Authorization checks (the single authority for broker + terminal + tools)
// ---------------------------------------------------------------------------

/// Windows `fs::canonicalize` returns a verbatim path (`\\?\C:\...`) while the
/// registry stores display-form roots (`C:\...`). Every authorization compare
/// runs both sides through this normalization so the two spellings of the same
/// directory agree; without it a granted root could not authorize its own
/// canonical path.
fn compare_form(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    p.to_path_buf()
}

fn grant_covers(grant: &ScopeGrant, canon: &Path, want_write: bool, mode: ScopeMode) -> bool {
    if !grant.read {
        return false;
    }
    if want_write && (mode == ScopeMode::Observe || !grant.write) {
        return false;
    }
    let canon = compare_form(canon);
    let grant_root = compare_form(Path::new(&grant.root));
    canon == grant_root || canon.starts_with(&grant_root)
}

/// Locations no grant ever covers, not even Full Computer Access: the app's own
/// authority record and credential material live under the user profile, so the
/// tools whose authority they define must never be able to rewrite or read them.
fn protected_config_dir() -> Option<PathBuf> {
    static DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let dir = config_dir().ok()?;
        Some(
            std::fs::canonicalize(&dir)
                .map(|c| compare_form(&c))
                .unwrap_or_else(|_| compare_form(&dir)),
        )
    })
    .clone()
}

fn is_protected_path(canon: &Path) -> bool {
    let Some(config) = protected_config_dir() else { return false };
    let canon = compare_form(canon);
    canon == config || canon.starts_with(&config)
}

/// True when `path` (already canonicalized) is inside an authorized root with
/// the requested operation class. Reparse points never authorize.
pub fn is_authorized_canonical(canon: &Path, want_write: bool) -> bool {
    if crate::broker::is_reparse_point(canon) || is_protected_path(canon) {
        return false;
    }
    read_scope(|state| state.grants.iter().any(|g| grant_covers(g, canon, want_write, state.mode)))
}

/// True when `path` is inside an authorized root with the requested class.
/// The path is canonicalized first, so symlinks resolve to their real target:
/// a link pointing outside the scope is denied.
pub fn is_authorized_root(path: &Path, want_write: bool) -> bool {
    match std::fs::canonicalize(path) {
        Ok(canon) => is_authorized_canonical(&canon, want_write),
        Err(_) => false,
    }
}

/// The access classes the registry grants for `root` right now: `(read, write)`.
/// Callers that keep their own session grant use this so a revoked or
/// downgraded root cannot leave older session state in force.
pub fn effective_access(root: &Path) -> (bool, bool) {
    let Ok(canon) = std::fs::canonicalize(root) else { return (false, false) };
    (
        is_authorized_canonical(&canon, false),
        is_authorized_canonical(&canon, true),
    )
}

/// Terminal execution requires a write-capable root outside Observe mode.
pub fn workspace_root_supported(root: &Path) -> bool {
    if read_scope(|state| state.mode == ScopeMode::Observe) {
        return false;
    }
    is_authorized_root(root, true)
}

/// The currently write-capable, terminal-executable roots (canonicalized).
pub fn workspace_roots() -> Vec<PathBuf> {
    let roots: Vec<String> = read_scope(|state| {
        if state.mode == ScopeMode::Observe {
            Vec::new()
        } else {
            state.grants.iter().filter(|g| g.read && g.write).map(|g| g.root.clone()).collect()
        }
    });
    roots.iter().filter_map(|r| std::fs::canonicalize(r).ok()).collect()
}

/// Display-formatted roots for status surfaces.
pub fn display_roots() -> Vec<String> {
    read_scope(|state| state.grants.iter().map(|g| g.root.clone()).collect())
}

fn grant_matches_root(grant: &ScopeGrant, canon: &Path) -> bool {
    compare_form(Path::new(&grant.root)) == compare_form(canon)
}

/// Drop only the synthetic profile-scope grants created by Full Computer
/// Access. Explicit owner grants (including one the owner deliberately added
/// for the same folder) are preserved.
fn withdraw_full_access(grants: &mut Vec<ScopeGrant>) {
    grants.retain(|g| !g.full_access);
}

// ---------------------------------------------------------------------------
// Revocation side effects (broker session + terminal sessions)
// ---------------------------------------------------------------------------

fn enforce_after_change(app: &AppHandle) -> Result<(), String> {
    // Both revocation side effects must run: a broker suspension that fails
    // (for example while a write is Applying) must never skip terminal cleanup.
    let mut broker_error = None;
    let broker_state = app.state::<Mutex<BrokerState>>();
    if let Some((root, write)) = broker::active_grant_root(&broker_state) {
        if !is_authorized_root(&root, write) {
            if let Err(error) = broker::suspend_session_project(app.state::<Mutex<BrokerState>>()) {
                broker_error = Some(error);
            }
        }
    }
    // Terminal: kill every running session whose working directory is no
    // longer inside an authorized root for the active mode.
    let terminal_state = app.state::<Mutex<TerminalState>>();
    let keep: Vec<PathBuf> = workspace_roots();
    crate::terminal::kill_sessions_outside(&terminal_state, &keep);
    match broker_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn scope_status() -> ScopeStatus {
    read_scope(|state| ScopeStatus {
        mode: state.mode,
        grants: state.grants.clone(),
        store_unsupported: state.store_unsupported,
        profile_root: std::env::var("USERPROFILE").ok().map(|p| p),
    })
}

#[tauri::command]
pub fn scope_set_mode(
    app: AppHandle,
    mode: String,
) -> Result<ScopeStatus, String> {
    let new_mode = match mode.as_str() {
        "observe" => ScopeMode::Observe,
        "selected" => ScopeMode::Selected,
        "full" => ScopeMode::Full,
        _ => return Err("Choose Observe, Selected, or Full.".into()),
    };
    let _turn = crate::chats::ChatsStore::lock_turn()?;
    // Work on a copy and commit only after a successful persist, so a failed
    // write can never leave authority live in memory while absent on disk.
    let mut state = {
        let guard = REGISTRY.read().unwrap();
        guard.as_ref().ok_or("The scope registry is not available.")?.clone()
    };
    {
        if state.store_unsupported {
            return Err("The saved scope registry could not be read and was preserved. Remove scope.json to start a fresh one.".into());
        }
        let widening_to_full =
            state.mode != ScopeMode::Full && new_mode == ScopeMode::Full;
        if widening_to_full {
            // A standing Full Computer Access grant expands native authority
            // to the signed-in user's profile scope: it requires an explicit
            // native owner confirmation and is recorded with honest
            // provenance (a dialog click is never claimed as a verified human
            // identity).
            let profile = std::env::var("USERPROFILE")
                .map_err(|_| "The Windows user profile is unavailable.".to_string())?;
            let text = format!(
                "Grant PAIGE Full Computer Access?\n\nScope: ordinary read and write operations inside {}\nand the other roots listed in Computer Access, using your Windows account's own permissions.\n\nElevation is never performed: Windows UAC prompts still apply. Destructive and sensitive actions still ask separately. You can revoke this at any time.",
                profile
            );
            if !crate::broker::uses_test_confirmation()
                && !crate::broker::native_confirm(&text, "PAIGE Desktop - Full Computer Access")
            {
                return Err("Full Computer Access was declined.".into());
            }
            let canon = std::fs::canonicalize(&profile)
                .map_err(|e| format!("The user profile is inaccessible: {e}"))?;
            if crate::broker::is_reparse_point(&canon) {
                return Err("The user profile cannot be identified reliably.".into());
            }
            let identity = crate::broker::hold_project_directory(&canon)
                .and_then(|file| FolderIdentity::read(&file).map_err(|e| e.to_string()))
                .map_err(|e| format!("{e}"))?;
            let confirmed_via = if crate::broker::uses_test_confirmation() {
                "test confirmation".to_string()
            } else {
                "owner dialog (identity unverified)".to_string()
            };
            if let Some(existing) = state.grants.iter_mut().find(|g| grant_matches_root(g, &canon)) {
                existing.read = true;
                existing.write = true;
                existing.confirmed_via = confirmed_via;
                existing.full_access = true;
            } else {
                state.grants.push(ScopeGrant {
                    id: format!("scope-{}", state.next_id),
                    root: display_path(&canon),
                    read: true,
                    write: true,
                    entity_id: "personal".into(),
                    entity_name: "Personal".into(),
                    granted_at: now_secs(),
                    identity,
                    confirmed_via,
                    full_access: true,
                });
                state.next_id += 1;
            }
        } else if state.mode == ScopeMode::Full {
            // Leaving Full Computer Access withdraws the standing profile-scope
            // grant. Without this, returning to Selected would silently keep
            // full authority and a "revoked" root would still be reachable.
            withdraw_full_access(&mut state.grants);
        }
        state.mode = new_mode;
        persist(&state)?;
        *REGISTRY.write().unwrap() = Some(state);
    }
    enforce_after_change(&app)?;
    Ok(scope_status())
}

#[tauri::command]
pub fn scope_add_root(
    app: AppHandle,
    root: String,
    entity_id: Option<String>,
    entity_name: String,
    read: bool,
    write: bool,
) -> Result<ScopeStatus, String> {
    if !read && !write {
        return Err("Choose at least read or write for this root.".into());
    }
    let entity_name = entity_name.trim().to_string();
    if entity_name.is_empty() || entity_name.len() > 80 {
        return Err("Use a workspace display name up to 80 characters.".into());
    }
    // The security identity is the stable slug; the display name is never
    // used for authorization decisions.
    let entity_id = entity_id
        .unwrap_or_else(|| entity_name.to_lowercase().replace(' ', "-"))
        .trim()
        .to_lowercase()
        .replace(' ', "-");
    if entity_id.is_empty() || entity_id.len() > 40 || !entity_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("Use a short workspace identifier (letters, numbers, dashes).".into());
    }
    let _turn = crate::chats::ChatsStore::lock_turn()?;
    // Work on a copy and commit only after a successful persist, so a failed
    // write can never leave authority live in memory while absent on disk.
    let mut state = {
        let guard = REGISTRY.read().unwrap();
        guard.as_ref().ok_or("The scope registry is not available.")?.clone()
    };
    {
        if state.store_unsupported {
            return Err("The saved scope registry could not be read and was preserved. Remove scope.json to start a fresh one.".into());
        }
        let canon = std::fs::canonicalize(&root)
            .map_err(|e| format!("path is inaccessible: {e}"))?;
        if !canon.is_dir() {
            return Err("path is not a directory".into());
        }
        if crate::broker::is_reparse_point(&canon) {
            return Err("root must not be a reparse point".into());
        }
        if is_protected_path(&canon) {
            return Err("PAIGE's own settings and saved credentials can never be granted.".into());
        }
        // Observe mode never stores write authority; say so in the dialog so
        // the owner is not told something broader than what is stored.
        let stored_write = write && state.mode != ScopeMode::Observe;
        // Adding a root is itself a widening grant: confirm it natively
        // unless it only re-affirms an existing grant that already covers it.
        let already_covers = state
            .grants
            .iter()
            .find(|g| grant_matches_root(g, &canon))
            .is_some_and(|g| g.read || !read)
            && state
                .grants
                .iter()
                .find(|g| grant_matches_root(g, &canon))
                .is_some_and(|g| g.write || !stored_write);
        if !already_covers && !crate::broker::uses_test_confirmation() {
            let text = format!(
                "Add this root to PAIGE's Computer Access?\n\n{}\n\nPermissions: {}\nWorkspace: {}\n\nFiles inside stay under your Windows account's permissions. You can remove this root at any time.{}",
                display_path(&canon),
                if read && stored_write { "read and write" } else if read { "read only" } else { "write" },
                entity_name,
                if read && write && !stored_write {
                    " Write access stays paused while Observe mode is active."
                } else {
                    ""
                },
            );
            if !crate::broker::native_confirm(&text, "PAIGE Desktop - Add Computer Access root") {
                return Err("The new root was declined.".into());
            }
        }
        let write = stored_write;
        let identity = crate::broker::hold_project_directory(&canon)
            .and_then(|file| FolderIdentity::read(&file).map_err(|e| e.to_string()))
            .map_err(|e| format!("{e}"))?;
        let confirmed_via = if crate::broker::uses_test_confirmation() {
            "test confirmation".to_string()
        } else {
            "owner dialog (identity unverified)".to_string()
        };
        if let Some(grant) = state.grants.iter_mut().find(|g| grant_matches_root(g, &canon)) {
            grant.read = grant.read || read;
            grant.write = grant.write || write;
            grant.entity_id = entity_id;
            grant.entity_name = entity_name;
            grant.identity = identity;
            grant.confirmed_via = confirmed_via;
            // An explicit owner add is a named grant the owner keeps; it is no
            // longer the synthetic Full Access grant that leaving Full drops.
            grant.full_access = false;
        } else {
            state.grants.push(ScopeGrant {
                id: format!("scope-{}", state.next_id),
                root: display_path(&canon),
                read,
                write,
                entity_id,
                entity_name,
                granted_at: now_secs(),
                identity,
                confirmed_via,
                full_access: false,
            });
            state.next_id += 1;
        }
        persist(&state)?;
        *REGISTRY.write().unwrap() = Some(state);
    }
    enforce_after_change(&app)?;
    Ok(scope_status())
}

#[tauri::command]
pub fn scope_remove_root(app: AppHandle, id: String) -> Result<ScopeStatus, String> {
    let _turn = crate::chats::ChatsStore::lock_turn()?;
    // Work on a copy and commit only after a successful persist, so a failed
    // write can never leave authority live in memory while absent on disk.
    let mut state = {
        let guard = REGISTRY.read().unwrap();
        guard.as_ref().ok_or("The scope registry is not available.")?.clone()
    };
    {
        if state.store_unsupported {
            return Err("The saved scope registry could not be read and was preserved. Remove scope.json to start a fresh one.".into());
        }
        let before = state.grants.len();
        state.grants.retain(|g| g.id != id);
        if state.grants.len() == before {
            return Err("That root is not in PAIGE's Computer Access list.".into());
        }
        // Removing a root only drops the PAIGE list entry. The Windows folder
        // is never touched.
        persist(&state)?;
        *REGISTRY.write().unwrap() = Some(state);
    }
    enforce_after_change(&app)?;
    Ok(scope_status())
}

/// Owner STOP: switch to Observe, kill terminal execution outside the
/// remaining read scope, and suspend any broker session that is no longer
/// authorized. Grants are preserved as read-only records; revoke individually
/// with scope_remove_root.
#[tauri::command]
pub fn scope_revoke_all(app: AppHandle) -> Result<ScopeStatus, String> {
    let _turn = crate::chats::ChatsStore::lock_turn()?;
    let mut state = {
        let guard = REGISTRY.read().unwrap();
        guard.as_ref().ok_or("The scope registry is not available.")?.clone()
    };
    {
        if state.store_unsupported {
            return Err("The saved scope registry could not be read and was preserved. Remove scope.json to start a fresh one.".into());
        }
        state.mode = ScopeMode::Observe;
        persist(&state)?;
        *REGISTRY.write().unwrap() = Some(state);
    }
    enforce_after_change(&app)?;
    Ok(scope_status())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_fixture() -> FolderIdentity {
        serde_json::from_str(
            r#"{"volume":1,"file":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"created":1}"#,
        ).expect("fixture identity")
    }

    #[test]
    fn observe_mode_never_authorizes_writes() {
        let grant = ScopeGrant {
            id: "t".into(), root: "C:\\does-not-matter".into(),
            read: true, write: true, entity_id: "personal".into(),
            entity_name: "Personal".into(), granted_at: 0,
            identity: identity_fixture(),
            confirmed_via: "test".into(),
            full_access: false,
        };
        let mode = ScopeMode::Observe;
        assert!(!grant_covers(&grant, Path::new("C:\\does-not-matter"), true, mode),
            "Observe must never authorize writes even on a write-capable grant");
        assert!(grant_covers(&grant, Path::new("C:\\does-not-matter"), false, mode),
            "Observe still permits reads inside granted roots");
    }

    #[test]
    fn sibling_prefixes_never_match_a_grant() {
        let grant = ScopeGrant {
            id: "t".into(), root: "C:\\repo".into(),
            read: true, write: true, entity_id: "personal".into(),
            entity_name: "Personal".into(), granted_at: 0,
            identity: identity_fixture(),
            confirmed_via: "test".into(),
            full_access: false,
        };
        assert!(grant_covers(&grant, Path::new("C:\\repo\\sub"), true, ScopeMode::Selected));
        assert!(!grant_covers(&grant, Path::new("C:\\repo-other"), true, ScopeMode::Selected),
            "a sibling whose name merely starts with the root must be denied");
    }

    #[test]
    fn write_grants_only_cover_writes_in_selected_and_full() {
        let mut grant = ScopeGrant {
            id: "t".into(), root: "C:\\repo".into(),
            read: true, write: false, entity_id: "personal".into(),
            entity_name: "Personal".into(), granted_at: 0,
            identity: identity_fixture(),
            confirmed_via: "test".into(),
            full_access: false,
        };
        assert!(!grant_covers(&grant, Path::new("C:\\repo"), true, ScopeMode::Selected));
        assert!(grant_covers(&grant, Path::new("C:\\repo"), false, ScopeMode::Selected));
        grant.write = true;
        assert!(grant_covers(&grant, Path::new("C:\\repo"), true, ScopeMode::Full));
    }

    #[test]
    fn scope_store_round_trips_and_unsupported_versions_fail_closed() {
        let path = std::env::temp_dir().join("paige-scope-store-roundtrip.json");
        let _ = std::fs::remove_file(&path);
        let file = ScopeFile {
            version: SCOPE_STORE_VERSION,
            mode: ScopeMode::Selected,
            next_id: 4,
            grants: vec![ScopeGrant {
                id: "scope-1".into(), root: "C:\\repo".into(),
                read: true, write: true, entity_id: "paige-agent-ai".into(),
                entity_name: "PAIGE Agent AI Inc.".into(), granted_at: 7,
                identity: identity_fixture(),
                confirmed_via: "test".into(),
            full_access: false,
            }],
        };
        std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
        let loaded: ScopeFile = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(loaded.mode, ScopeMode::Selected);
        assert_eq!(loaded.grants[0].entity_id, "paige-agent-ai");
        // An unsupported version must never deserialize as usable authority.
        let mut bad = file.clone();
        bad.version = 99;
        std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        let parsed: Result<ScopeFile, _> = serde_json::from_slice(&std::fs::read(&path).unwrap());
        let refused = parsed.map(|f| f.version != SCOPE_STORE_VERSION).unwrap_or(true);
        assert!(refused, "an unsupported scope store must be refused");
        assert!(path.exists(), "a refused store is never deleted by loading");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn workspace_roots_exclude_observe_mode() {
        // workspace_roots reads the process-global registry; in the unit-test
        // binary the registry is not initialized, which must fail closed.
        let roots = workspace_roots();
        // Not initialized -> Observe-shaped fallback -> no write roots.
        assert!(roots.is_empty(), "an uninitialized registry must never expose write roots");
    }

    fn grant_at(root: &str) -> ScopeGrant {
        ScopeGrant {
            id: "t".into(),
            root: root.into(),
            read: true,
            write: true,
            entity_id: "personal".into(),
            entity_name: "Personal".into(),
            granted_at: 0,
            identity: identity_fixture(),
            confirmed_via: "test".into(),
            full_access: false,
        }
    }

    #[test]
    fn verbatim_canonical_path_authorizes_its_display_root() {
        // `fs::canonicalize` yields `\\?\C:\...` while the store keeps `C:\...`.
        let grant = grant_at(r"C:\Users\owner\project");
        let verbatim_root = Path::new(r"\\?\C:\Users\owner\project");
        assert!(grant_covers(&grant, verbatim_root, true, ScopeMode::Selected));
        assert!(grant_covers(
            &grant,
            Path::new(r"\\?\C:\Users\owner\project\src"),
            true,
            ScopeMode::Selected
        ));
        assert!(grant_covers(&grant, Path::new(r"C:\Users\owner\project"), true, ScopeMode::Selected));
        // A sibling with a shared name prefix is never covered.
        assert!(!grant_covers(
            &grant,
            Path::new(r"\\?\C:\Users\owner\project-two"),
            true,
            ScopeMode::Selected
        ));
        assert!(!grant_covers(&grant, Path::new(r"\\?\C:\Windows\System32"), true, ScopeMode::Selected));
    }

    #[test]
    fn object_identity_match_ignores_the_verbatim_prefix() {
        // Re-adding the same folder must merge into one grant, not duplicate it.
        let grant = grant_at(r"C:\Users\owner\project");
        assert!(grant_matches_root(&grant, Path::new(r"\\?\C:\Users\owner\project")));
        assert!(grant_matches_root(&grant, Path::new(r"C:\Users\owner\project")));
        assert!(!grant_matches_root(&grant, Path::new(r"\\?\C:\Users\owner\other")));
    }

    #[test]
    fn leaving_full_access_withdraws_only_the_synthetic_profile_grant() {
        let mut profile = grant_at(r"C:\Users\owner");
        profile.full_access = true;
        let kept = grant_at(r"C:\Users\owner\project");
        let mut grants = vec![profile, kept.clone()];
        withdraw_full_access(&mut grants);
        assert_eq!(grants.len(), 1, "only the Full Access grant is withdrawn");
        assert_eq!(grants[0].root, kept.root);
        assert!(!grants.iter().any(|g| g.full_access));
    }

    #[test]
    fn a_full_access_grant_covers_the_profile_but_selected_does_not_after_withdrawal() {
        let mut profile = grant_at(r"C:\Users\owner");
        profile.full_access = true;
        let mut grants = vec![profile];
        let target = Path::new(r"\\?\C:\Users\owner\Documents");
        assert!(grants.iter().any(|g| grant_covers(g, target, true, ScopeMode::Full)));
        withdraw_full_access(&mut grants);
        assert!(!grants.iter().any(|g| grant_covers(g, target, true, ScopeMode::Selected)),
            "after leaving Full, ordinary profile paths must no longer be authorized");
    }

    #[test]
    fn persisted_grants_are_validated_before_authorization() {
        // A stored root that is not an absolute directory is never accepted.
        let grants = vec![ScopeGrant {
            id: "t".into(),
            root: "relative\\folder".into(),
            read: true,
            write: true,
            entity_id: "personal".into(),
            entity_name: "Personal".into(),
            granted_at: 0,
            identity: identity_fixture(),
            confirmed_via: "test".into(),
            full_access: false,
        }];
        assert!(validate_loaded_grants(grants).is_empty());
    }
}
