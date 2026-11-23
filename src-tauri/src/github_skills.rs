//! Public skill discovery and owner-approved, immutable GitHub package installs.
use crate::ask::{AskReceipt, default_actor, now_ms, publish_receipt};
use crate::chats::ChatMessage;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, atomic::{AtomicBool, Ordering}},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, State, Url};

pub(crate) const ROUTE: &str = "skills-discovery";
const MAX_FILE: usize = 2 * 1024 * 1024;
const MAX_PACKAGE: usize = 8 * 1024 * 1024;
const MAX_FILES: usize = 96;
const MAX_GUIDANCE: usize = 48_000;
const RECEIPT: &str = ".paige-install.json";

#[derive(Clone, Serialize, Deserialize)]
struct Candidate {
    repository: String,
    skill: Option<String>,
    description: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct Discovery {
    source: &'static str,
    results: Vec<Candidate>,
    installed: Vec<SkillInfo>,
}

#[derive(Serialize)]
pub(crate) struct Catalog {
    repository: String,
    commit: String,
    manifests: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SkillInfo {
    id: String,
    name: String,
    description: String,
    source: String,
    repository: String,
    commit: String,
    manifest: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct FileRecord {
    path: String,
    size: usize,
    sha256: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct InstallReceipt {
    skill: SkillInfo,
    files: Vec<FileRecord>,
    installed_at: u64,
}

#[derive(Serialize)]
pub(crate) struct Preview {
    token: String,
    skill: SkillInfo,
    files: Vec<FileRecord>,
    guidance: String,
}

struct Package {
    token: String,
    created_at: Instant,
    skill: SkillInfo,
    files: Vec<FileRecord>,
    bytes: Vec<Vec<u8>>,
}

#[derive(Default)]
pub(crate) struct SkillInstallState(Mutex<Option<Package>>);

#[derive(Deserialize)]
struct Commit {
    sha: String,
    commit: CommitData,
}

#[derive(Deserialize)]
struct CommitData { tree: TreeRef }

#[derive(Deserialize)]
struct TreeRef { sha: String }

#[derive(Deserialize)]
struct GitTree {
    truncated: bool,
    tree: Vec<TreeEntry>,
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    mode: String,
    #[serde(rename = "type")]
    kind: String,
    size: Option<usize>,
    sha: String,
}

fn request_agent() -> ureq::Agent {
    ureq::AgentBuilder::new().redirects(0).timeout(Duration::from_secs(12)).build()
}

fn url(host: &str, path: &[&str]) -> Url {
    let mut url = Url::parse(&format!("https://{host}")).expect("fixed HTTPS host");
    url.path_segments_mut().expect("HTTPS path").extend(path);
    url
}

fn get(agent: &ureq::Agent, url: Url, limit: usize) -> Result<Vec<u8>, String> {
    let source = url.host_str().unwrap_or("Skill provider");
    let response = match agent.get(url.as_str()).set("User-Agent", "PAIGE-Desktop/skill-discovery").call() {
        Ok(response) if response.status() == 200 => response,
        Err(ureq::Error::Status(403 | 429, _)) => return Err(format!("{source} denied the request or reached its public rate limit.")),
        Err(ureq::Error::Status(404, _)) => return Err(format!("{source} could not find this public resource.")),
        Err(ureq::Error::Status(status, _)) => return Err(format!("{source} returned HTTP {status}.")),
        _ => return Err(format!("PAIGE could not retrieve data from {source}.")),
    };
    let mut bytes = Vec::new();
    response.into_reader().take(limit as u64 + 1).read_to_end(&mut bytes)
        .map_err(|_| format!("The response from {source} was interrupted."))?;
    if bytes.len() > limit { return Err(format!("The response from {source} exceeded the size limit.")); }
    Ok(bytes)
}

fn get_json<T: DeserializeOwned>(agent: &ureq::Agent, url: Url, limit: usize) -> Result<T, String> {
    serde_json::from_slice(&get(agent, url, limit)?)
        .map_err(|_| "The skill provider returned an unreadable response.".into())
}

fn repository(input: &str) -> Result<(&str, &str), String> {
    let input = input.strip_prefix("https://github.com/").unwrap_or(input).trim_end_matches('/');
    let (owner, repo) = input.split_once('/').ok_or("Enter a public GitHub repository as owner/repository.")?;
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    if owner.is_empty() || owner.len() > 39 || !owner.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        || repo.is_empty() || repo.len() > 100 || matches!(repo, "." | "..")
        || !repo.bytes().all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        || repo.ends_with('.') {
        return Err("Enter a public GitHub repository as owner/repository.".into());
    }
    Ok((owner, repo))
}

fn sha(input: &str, len: usize) -> bool {
    input.len() == len && input.bytes().all(|b| b.is_ascii_hexdigit())
}

fn slug(input: &str) -> bool {
    !input.is_empty() && input.len() <= 64 && !input.starts_with('-')
        && !input.ends_with('-') && !input.contains("--")
        && input.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn skill_id(input: &str) -> bool {
    input.strip_prefix("github-").is_some_and(|hash| sha(hash, 24))
}

fn safe_path(path: &str) -> bool {
    !path.is_empty() && path.len() <= 220 && path.split('/').all(|part| {
        let lower = part.to_ascii_lowercase();
        let stem = lower.split('.').next().unwrap_or_default().trim_end();
        !part.is_empty() && part.len() <= 120 && !part.ends_with(['.', ' '])
            && !matches!(lower.as_str(), ".git" | RECEIPT)
            && !matches!(stem, "con" | "prn" | "aux" | "nul" | "conin$" | "conout$")
            && !(stem.len() == 4 && (stem.starts_with("com") || stem.starts_with("lpt"))
                && stem.as_bytes()[3].is_ascii_digit())
            && part.bytes().all(|c| (32..=126).contains(&c) && !b"\\:<>|\"?*~".contains(&c))
    })
}

fn query(input: &str) -> Result<&str, String> {
    let input = input.trim();
    if input.is_empty() || input.len() > 220 || input.chars().any(char::is_control) {
        return Err("Enter a one-line public skill search or a GitHub repository.".into());
    }
    Ok(input)
}

pub(crate) fn command(message: &str) -> Result<Option<&str>, String> {
    let message = message.trim();
    let lower = message.to_ascii_lowercase();
    if lower == "/skills" { return query("").map(Some); }
    if lower.starts_with("/skills ") { return query(&message[8..]).map(Some); }
    let mut rest = message;
    for group in [
        &["hey paige, ", "okay paige, ", "paige, ", "paige: "][..],
        &["could you ", "can you ", "would you ", "will you "][..],
        &["please "][..],
    ] {
        if let Some(prefix) = group.iter().find(|p| rest.to_ascii_lowercase().starts_with(**p)) {
            rest = rest[prefix.len()..].trim_start();
        }
    }
    let lower = rest.to_ascii_lowercase();
    let Some(prefix) = ["look for ", "search github for ", "search for ", "find and install ", "find me ", "find ", "install "]
        .into_iter().find(|prefix| lower.starts_with(prefix)) else { return Ok(None); };
    rest = rest[prefix.len()..].trim_end_matches(['.', '?', '!']).trim_end();
    if repository(rest.split_once('@').map_or(rest, |(repo, _)| repo)).is_ok() {
        return query(rest).map(Some);
    }
    let lower = rest.to_ascii_lowercase();
    if lower.contains(":\\") || lower.contains(":/") || lower.contains(" from this project")
        || lower.contains(" in this project") || lower.contains(" in this repo")
        || lower.contains(" in my project") || lower.contains(" in my repo")
        || lower.contains(" on my laptop") || lower.contains(" on this device")
        || lower.contains(" locally") || lower.contains(" without going online")
    {
        return Ok(None);
    }
    for suffix in [" skill from github", " skill on github"] {
        if lower.ends_with(suffix) {
            let name = rest[..rest.len() - suffix.len()].trim();
            let name = name.strip_prefix("the ").unwrap_or(name);
            if !name.is_empty() && !name.contains('/') { return query(name).map(Some); }
        }
    }
    for subject in ["a new github skill", "new github skills", "a github skill", "github skills",
        "a new skill", "new skills", "a skill", "skills", "the skill"] {
        let Some(tail) = lower.strip_prefix(subject).filter(|tail| tail.starts_with(' ')) else { continue; };
        let qualifier = [" on github", " from github"].into_iter().find(|part| tail.starts_with(*part));
        let original = &rest[subject.len() + qualifier.map_or(0, str::len)..];
        for connector in [" for ", " about ", " to ", " named ", " called "] {
            if original.to_ascii_lowercase().starts_with(connector) {
                return query(&original[connector.len()..]).map(Some);
            }
        }
        return Ok(None);
    }
    Ok(None)
}

fn discover(root: &Path, input: &str) -> Result<Discovery, String> {
    let input = query(input)?;
    let key = input.to_ascii_lowercase();
    let installed = list_installed(root)?.into_iter().filter(|item| {
        key.split_whitespace().all(|word| item.name.contains(word)
            || item.description.to_ascii_lowercase().contains(word) || item.repository.to_ascii_lowercase().contains(word))
    }).collect::<Vec<_>>();
    let (repo_input, exact_skill) = input.split_once('@').map_or((input, None), |(repo, name)| (repo, Some(name)));
    if let Ok((owner, repo)) = repository(repo_input) {
        if exact_skill.is_some_and(|name| !slug(name)) { return Err("Use owner/repository@skill-name.".into()); }
        return Ok(Discovery { source: "GitHub repository supplied", results: vec![Candidate {
            repository: format!("{owner}/{repo}"), skill: exact_skill.map(str::to_owned), description: None,
        }], installed });
    }
    let agent = request_agent();
    let mut endpoint = url("skills.sh", &["api", "search"]);
    endpoint.query_pairs_mut().append_pair("q", input).append_pair("limit", "10");
    if let Ok(index) = get_json::<Value>(&agent, endpoint, 256_000)
        && let Some(skills) = index["skills"].as_array()
    {
        let mut seen = BTreeSet::new();
        let results = skills.iter().filter_map(|item| {
            let (owner, repo) = repository(item["source"].as_str()?).ok()?;
            let name = item["name"].as_str()?.trim();
            if !slug(name) || !seen.insert(format!("{owner}/{repo}@{name}").to_ascii_lowercase()) { return None; }
            Some(Candidate { repository: format!("{owner}/{repo}"), skill: Some(name.into()), description: None })
        }).take(10).collect::<Vec<_>>();
        if !results.is_empty() { return Ok(Discovery { source: "Skills.sh index", results, installed }); }
    }
    let mut endpoint = url("api.github.com", &["search", "repositories"]);
    endpoint.query_pairs_mut().append_pair("q", &format!("{input} skill archived:false")).append_pair("per_page", "8");
    let found = get_json::<Value>(&agent, endpoint, 256_000);
    let results = match found {
        Ok(repos) => repos["items"].as_array().ok_or("GitHub did not return repository results.")?.iter().filter_map(|item| {
            let (owner, repo) = repository(item["full_name"].as_str()?).ok()?;
            Some(Candidate { repository: format!("{owner}/{repo}"), skill: None,
                description: item["description"].as_str().map(|text| text.chars().filter(|c| !c.is_control()).take(180).collect()) })
        }).take(8).collect(),
        Err(_) if !installed.is_empty() => return Ok(Discovery {
            source: "Installed skills (public search unavailable)", results: vec![], installed,
        }),
        Err(error) => return Err(error),
    };
    Ok(Discovery { source: "GitHub public repositories", results, installed })
}

fn tree(agent: &ureq::Agent, owner: &str, repo: &str, reference: &str) -> Result<(String, GitTree), String> {
    let commit: Commit = get_json(agent, url("api.github.com", &["repos", owner, repo, "commits", reference]), 4 * MAX_FILE)?;
    if !sha(&commit.sha, 40) || !sha(&commit.commit.tree.sha, 40)
        || (reference != "HEAD" && !commit.sha.eq_ignore_ascii_case(reference)) {
        return Err("GitHub did not return the requested immutable commit.".into());
    }
    let mut endpoint = url("api.github.com", &["repos", owner, repo, "git", "trees", &commit.commit.tree.sha]);
    endpoint.query_pairs_mut().append_pair("recursive", "1");
    let tree: GitTree = get_json(agent, endpoint, 8 * MAX_FILE)?;
    if tree.truncated { return Err("GitHub returned an incomplete file list. Choose a smaller skill repository.".into()); }
    Ok((commit.sha, tree))
}

fn is_manifest(path: &str) -> bool {
    path.rsplit('/').next() == Some("SKILL.md") && safe_path(path)
}

fn catalog(repo: &str) -> Result<Catalog, String> {
    let (owner, repo) = repository(repo)?;
    let (commit, tree) = tree(&request_agent(), owner, repo, "HEAD")?;
    let mut manifests: Vec<_> = tree.tree.into_iter()
        .filter(|e| is_manifest(&e.path) && matches!(e.mode.as_str(), "100644" | "100755") && e.kind == "blob")
        .map(|e| e.path).collect();
    manifests.sort();
    if manifests.len() > 500 { return Err("This repository has more than 500 skill manifests. Choose a smaller repository.".into()); }
    Ok(Catalog { repository: format!("{owner}/{repo}"), commit, manifests })
}

fn package_entries<'a>(tree: &'a GitTree, manifest: &str) -> Result<Vec<(&'a str, usize, &'a str)>, String> {
    if !is_manifest(manifest) { return Err("Choose a valid SKILL.md from the repository.".into()); }
    let prefix = manifest.strip_suffix("SKILL.md").unwrap_or_default();
    let mut paths = BTreeMap::new();
    let mut entries = Vec::new();
    let mut total = 0;
    for entry in &tree.tree {
        let Some(path) = entry.path.strip_prefix(prefix) else { continue; };
        if !safe_path(path) { return Err("This skill has a file path Windows cannot safely install.".into()); }
        let key = path.to_ascii_lowercase();
        let is_dir = entry.kind == "tree" && entry.mode == "040000";
        if paths.insert(key, is_dir).is_some() { return Err("This skill has paths that collide on Windows.".into()); }
        if is_dir { continue; }
        if entry.kind != "blob" || !matches!(entry.mode.as_str(), "100644" | "100755") {
            return Err("This skill contains a link, submodule, or unsupported file type.".into());
        }
        let size = entry.size.filter(|size| *size <= MAX_FILE).ok_or("A skill file exceeds the 2 MiB file limit.")?;
        total += size;
        if total > MAX_PACKAGE || entries.len() >= MAX_FILES { return Err("This skill exceeds the 8 MiB or 96-file package limit.".into()); }
        if !sha(&entry.sha, 40) { return Err("GitHub returned an invalid blob object ID.".into()); }
        entries.push((path, size, entry.sha.as_str()));
    }
    for key in paths.keys() {
        for (index, _) in key.match_indices('/') {
            if paths.get(&key[..index]) != Some(&true) { return Err("The skill’s directory tree is incomplete.".into()); }
        }
    }
    if !entries.iter().any(|(path, _, _)| *path == "SKILL.md") {
        return Err("The selected SKILL.md is missing at this commit.".into());
    }
    entries.sort_unstable_by_key(|entry| entry.0);
    Ok(entries)
}

fn metadata(content: &str) -> Result<(String, String), String> {
    let mut lines = content.trim_start_matches('\u{feff}').lines().peekable();
    if lines.next().map(str::trim) != Some("---") { return Err("SKILL.md is missing Agent Skills front matter.".into()); }
    let (mut name, mut description) = (None, None);
    let mut closed = false;
    while let Some(line) = lines.next() {
        if line.trim() == "---" { closed = true; break; }
        let Some((key, value)) = line.split_once(':') else { continue; };
        let target = match key { "name" => &mut name, "description" => &mut description, _ => continue };
        if target.is_some() { return Err("SKILL.md repeats a required metadata field.".into()); }
        let value = value.trim();
        let mut value = if matches!(value, "" | ">" | ">-" | ">+" | "|" | "|-" | "|+") {
            let mut parts = Vec::new();
            while lines.peek().is_some_and(|line| line.starts_with([' ', '\t']) || line.is_empty()) {
                parts.push(lines.next().unwrap_or_default().trim());
            }
            parts.join(" ").trim().to_owned()
        } else {
            value.trim_matches(['\'', '"']).to_owned()
        };
        if key == "description" {
            while lines.peek().is_some_and(|line| line.starts_with([' ', '\t'])) {
                value.push(' ');
                value.push_str(lines.next().unwrap_or_default().trim());
            }
        }
        *target = Some(value);
    }
    let name = name.filter(|value| slug(value)).ok_or("SKILL.md needs a valid lowercase, hyphenated skill name.")?;
    let description = description.filter(|value| !value.is_empty() && value.chars().count() <= 1024
        && !value.chars().any(char::is_control)).ok_or("SKILL.md needs a description of at most 1,024 characters.")?;
    if !closed { return Err("SKILL.md front matter has no closing delimiter.".into()); }
    Ok((name, description))
}

fn digest(bytes: &[u8]) -> String { hex::encode(Sha256::digest(bytes)) }

fn git_blob_hash(bytes: &[u8]) -> String {
    let mut hash = ring::digest::Context::new(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY);
    hash.update(format!("blob {}\0", bytes.len()).as_bytes());
    hash.update(bytes);
    hex::encode(hash.finish())
}

fn random_id() -> Result<String, String> {
    let mut bytes = [0; 16];
    crate::broker::os_random_bytes(&mut bytes)?;
    Ok(hex::encode(bytes))
}

fn prepare(repo_input: &str, commit: &str, manifest: &str) -> Result<(Package, Preview), String> {
    let (owner, repo) = repository(repo_input)?;
    if !sha(commit, 40) || !is_manifest(manifest) { return Err("Select a skill from the pinned GitHub commit.".into()); }
    let agent = request_agent();
    let (commit, tree) = tree(&agent, owner, repo, commit)?;
    let entries = package_entries(&tree, manifest)?;
    let prefix = manifest.strip_suffix("SKILL.md").unwrap_or_default();
    let mut files = Vec::new();
    let mut bytes = Vec::new();
    let mut guidance = String::new();
    let deadline = Instant::now() + Duration::from_secs(65);
    for (path, size, object_id) in entries {
        if Instant::now() >= deadline { return Err("The GitHub package download timed out.".into()); }
        let full_path = format!("{prefix}{path}");
        let mut segments = vec![owner, repo, commit.as_str()];
        segments.extend(full_path.split('/'));
        let contents = get(&agent, url("raw.githubusercontent.com", &segments), size)?;
        if contents.len() != size || !git_blob_hash(&contents).eq_ignore_ascii_case(object_id) {
            return Err(format!("The downloaded {path} does not match the pinned GitHub blob."));
        }
        if path == "SKILL.md" {
            if size > MAX_GUIDANCE { return Err("SKILL.md exceeds PAIGE’s 48 kB guidance limit.".into()); }
            guidance = String::from_utf8(contents.clone()).map_err(|_| "SKILL.md must use UTF-8.")?;
        }
        files.push(FileRecord { path: path.into(), size, sha256: digest(&contents) });
        bytes.push(contents);
    }
    let (name, description) = metadata(&guidance)?;
    let repository = format!("{owner}/{repo}");
    let origin = format!("{}/{manifest}", repository.to_ascii_lowercase());
    let id = format!("github-{}", &digest(origin.as_bytes())[..24]);
    let mut source = vec![owner, repo, "blob", commit.as_str()];
    source.extend(manifest.split('/'));
    let skill = SkillInfo { id, name, description, source: url("github.com", &source).into(),
        repository, commit, manifest: manifest.into() };
    let token = random_id()?;
    let preview = Preview { token: token.clone(), skill: skill.clone(), files: files.clone(), guidance };
    Ok((Package { token, created_at: Instant::now(), skill, files, bytes }, preview))
}

fn stat(path: &Path) -> Result<fs::Metadata, std::io::Error> {
    let meta = fs::symlink_metadata(path)?;
    let redirect = meta.file_type().is_symlink();
    #[cfg(windows)]
    let redirect = {
        use std::os::windows::fs::MetadataExt;
        redirect || meta.file_attributes() & 0x400 != 0
    };
    if redirect { return Err(std::io::Error::other("A skill path is a link or reparse point.")); }
    Ok(meta)
}

fn root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|_| "Could not locate PAIGE’s app data folder.")?.join("skills"))
}

fn existing_dir(path: &Path) -> Result<bool, String> {
    match stat(path) {
        Ok(meta) if meta.is_dir() => Ok(true),
        Ok(_) => Err("The skill storage path is not a directory.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

fn read_sized(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let meta = stat(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > limit as u64 { return Err("An installed skill file has an unexpected size or type.".into()); }
    let mut bytes = Vec::new();
    fs::File::open(path).and_then(|file| file.take(limit as u64 + 1).read_to_end(&mut bytes))
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit { return Err("An installed skill file exceeded its size limit.".into()); }
    Ok(bytes)
}

fn load_receipt(folder: &Path, id: &str) -> Result<InstallReceipt, String> {
    if !skill_id(id) || !existing_dir(folder)? { return Err("The skill is not installed.".into()); }
    let data = read_sized(&folder.join(RECEIPT), 100_000)?;
    let receipt: InstallReceipt = serde_json::from_slice(&data).map_err(|_| "The skill install receipt is unreadable.")?;
    if receipt.skill.id != id || !slug(&receipt.skill.name) || !sha(&receipt.skill.commit, 40)
        || receipt.files.len() > MAX_FILES {
        return Err("The skill install receipt is invalid.".into());
    }
    Ok(receipt)
}

fn checked_file(folder: &Path, record: &FileRecord) -> Result<Vec<u8>, String> {
    if !safe_path(&record.path) || record.size > MAX_FILE {
        return Err("The skill receipt names an invalid file.".into());
    }
    let mut path = folder.to_owned();
    let mut parts = record.path.split('/').peekable();
    while let Some(part) = parts.next() {
        path.push(part);
        if parts.peek().is_some() && !existing_dir(&path)? { return Err("A skill directory is missing.".into()); }
    }
    let bytes = read_sized(&path, record.size)?;
    if bytes.len() != record.size || digest(&bytes) != record.sha256 {
        return Err(format!("The installed {} no longer matches its receipt.", record.path));
    }
    Ok(bytes)
}

fn list_installed(root: &Path) -> Result<Vec<SkillInfo>, String> {
    if !existing_dir(root)? { return Ok(vec![]); }
    let mut installed = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if let Some(id) = entry.file_name().to_str()
            && skill_id(id) && let Ok(receipt) = load_receipt(&entry.path(), id) {
            installed.push(receipt.skill);
        }
    }
    installed.sort_by(|a, b| (&a.name, &a.repository).cmp(&(&b.name, &b.repository)));
    Ok(installed)
}

pub(crate) fn installed_skill(app: &AppHandle, id: &str) -> Result<crate::skills::ComposerSkill, String> {
    if !skill_id(id) { return Err("The skill identifier is invalid.".into()); }
    let root = root(app)?;
    if !existing_dir(&root)? { return Err("The skill is not installed.".into()); }
    let folder = root.join(id);
    let receipt = load_receipt(&folder, id)?;
    let guidance = installed_guidance(&folder, &receipt)?;
    Ok(crate::skills::ComposerSkill { id: id.into(), name: receipt.skill.name,
        guidance: guidance.to_string(), design: false, source: Some(receipt.skill.source) })
}

fn installed_guidance(folder: &Path, receipt: &InstallReceipt) -> Result<Value, String> {
    let manifest = receipt.files.iter().find(|entry| entry.path == "SKILL.md").ok_or("The skill has no verified SKILL.md.")?;
    if manifest.size > MAX_GUIDANCE { return Err("The installed SKILL.md exceeds the guidance limit.".into()); }
    let guidance = String::from_utf8(checked_file(folder, manifest)?).map_err(|_| "The installed SKILL.md is not UTF-8.")?;
    let mut references = BTreeMap::new();
    let mut remaining = 32_000;
    for record in receipt.files.iter().filter(|entry| entry.path != "SKILL.md"
        && (entry.path.ends_with(".md") || entry.path.ends_with(".txt")) && guidance.contains(&entry.path)) {
        if record.size > remaining { continue; }
        let bytes = checked_file(folder, record)?;
        if let Ok(text) = String::from_utf8(bytes) {
            remaining -= record.size;
            references.insert(&record.path, text);
        }
    }
    Ok(json!({"SKILL.md":guidance, "references":references}))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| e.to_string())?;
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())
}

fn previous_install(root: &Path, package: &Package) -> Result<Option<InstallReceipt>, String> {
    if !existing_dir(root)? { return Ok(None); }
    let destination = root.join(&package.skill.id);
    if !existing_dir(&destination)? { return Ok(None); }
    let receipt = load_receipt(&destination, &package.skill.id)?;
    if receipt.skill.commit != package.skill.commit {
        return Err("A different commit of this skill is already installed.".into());
    }
    if receipt.files.len() != package.files.len() || receipt.files.iter().zip(&package.files)
        .any(|(saved, proposed)| saved.path != proposed.path || saved.sha256 != proposed.sha256 || saved.size != proposed.size) {
        return Err("The existing installation does not match this package.".into());
    }
    for record in &receipt.files { checked_file(&destination, record)?; }
    Ok(Some(receipt))
}

fn persist(root: &Path, package: Package) -> Result<InstallReceipt, String> {
    if let Some(receipt) = previous_install(root, &package)? { return Ok(receipt); }
    let parent = root.parent().ok_or("The skill store has no app data directory.")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    existing_dir(parent)?;
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    existing_dir(root)?;
    let stage = root.join(format!(".install-{}", package.token));
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let destination = root.join(&package.skill.id);
    let result = (|| {
        for (record, bytes) in package.files.iter().zip(&package.bytes) {
            let path = stage.join(&record.path);
            fs::create_dir_all(path.parent().ok_or("The skill file has no parent.")?).map_err(|e| e.to_string())?;
            write_new(&path, bytes)?;
            checked_file(&stage, record)?;
        }
        let receipt = InstallReceipt { skill: package.skill.clone(), files: package.files.clone(), installed_at: now_ms() };
        let data = serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?;
        write_new(&stage.join(RECEIPT), &data)?;
        load_receipt(&stage, &package.skill.id)?;
        match fs::rename(&stage, &destination) {
            Ok(()) => Ok(receipt),
            Err(error) => previous_install(root, &package)?.ok_or_else(|| format!("Could not register the skill package. {error}")),
        }
    })();
    if stage.exists() { let _ = fs::remove_dir_all(stage); }
    result
}

#[tauri::command]
pub(crate) fn skills_installed(app: AppHandle) -> Result<Vec<SkillInfo>, String> {
    list_installed(&root(&app)?)
}

#[tauri::command]
pub(crate) async fn skills_search(app: AppHandle, input: String) -> Result<Discovery, String> {
    let root = root(&app)?;
    tauri::async_runtime::spawn_blocking(move || discover(&root, &input)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn skills_catalog(repository: String) -> Result<Catalog, String> {
    tauri::async_runtime::spawn_blocking(move || catalog(&repository)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn skills_prepare(app: AppHandle, repository: String, commit: String, manifest: String) -> Result<Preview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (package, preview) = prepare(&repository, &commit, &manifest)?;
        *app.state::<SkillInstallState>().0.lock().unwrap() = Some(package);
        Ok(preview)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn skills_install(app: AppHandle, state: State<'_, SkillInstallState>, token: String) -> Result<InstallReceipt, String> {
    let package = {
        let mut slot = state.0.lock().unwrap();
        if !slot.as_ref().is_some_and(|preview| preview.token == token && preview.created_at.elapsed() < Duration::from_secs(15 * 60)) {
            return Err("This review expired. Reopen the GitHub skill preview.".into());
        }
        slot.take().ok_or("No skill is ready to install.")?
    };
    let root = root(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(receipt) = previous_install(&root, &package)? { return Ok(receipt); }
        let message = format!("Install {} from {}?\n\nCommit: {}\nManifest: {}\nFiles: {}\n\nThe files will be saved to PAIGE’s Skills folder. Downloaded scripts will not run during installation.",
            package.skill.name, package.skill.repository, package.skill.commit, package.skill.manifest, package.files.len());
        if !crate::broker::native_confirm(&message, "PAIGE Desktop - Install GitHub skill") {
            return Err("Skill installation was declined in Windows. Reopen the preview to try again.".into());
        }
        persist(&root, package)
    }).await.map_err(|e| e.to_string())?
}

pub(crate) fn run(app: &AppHandle, chat: Option<&str>, turn: &str, input: &str, cancel: &AtomicBool) -> Result<Value, String> {
    if cancel.load(Ordering::Relaxed) { return Err("cancelled".into()); }
    let start = Instant::now();
    let result = discover(&root(app)?, input);
    if cancel.load(Ordering::Relaxed) { return Err("cancelled".into()); }
    let elapsed_ms = start.elapsed().as_millis();
    let (text, details, outcome) = match result {
        Ok(found) if found.results.is_empty() && found.installed.is_empty() => (
            "I didn’t find a matching skill. Try a different phrase or paste a public GitHub repository.".into(),
            json!({"discovery":found,"elapsed_ms":elapsed_ms}), "ok"),
        Ok(found) => (
            "Here are the matches. Open a GitHub result to review its files, or select an installed skill to use in this chat.".into(),
            json!({"discovery":found,"elapsed_ms":elapsed_ms}), "ok"),
        Err(error) => (error.clone(), json!({"error":error,"elapsed_ms":elapsed_ms}), "error"),
    };
    publish_receipt(app, chat, AskReceipt {
        timestamp: now_ms(), turn_id: turn.into(), kind: "tool".into(), tool: "skills_search".into(),
        gateway: ROUTE.into(), model: String::new(), summary: text.clone(),
        outcome: outcome.into(), details: details.to_string(), actor: default_actor(),
    }, None)?;
    publish_receipt(app, chat, AskReceipt {
        timestamp: now_ms(), turn_id: turn.into(), kind: "connector_turn".into(), tool: "final_answer".into(),
        gateway: ROUTE.into(), model: String::new(), summary: text.clone(), outcome: "complete".into(),
        details: String::new(), actor: default_actor(),
    }, Some(ChatMessage {
        role: "assistant".into(), text: text.clone(), tool_events: vec![], model_label: None, model: None,
        gateway: Some(ROUTE.into()), turn_id: Some(turn.into()),
    }))?;
    Ok(json!({"turn_id":turn,"status":"complete","text":text}))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_skill_search_without_capturing_other_work() {
        for (request, expected) in [
            ("Can you find a GitHub skill for accessibility reviews?", "accessibility reviews"),
            ("Find me a skill on GitHub for reviewing Markdown.", "reviewing Markdown"),
            ("Install https://github.com/example/tools", "https://github.com/example/tools"),
            ("Find and install example/tools@formatting", "example/tools@formatting"),
            ("Paige, can you please install the web design guidelines skill from GitHub?", "web design guidelines"),
            ("/skills documents", "documents"),
        ] {
            assert_eq!(command(request), Ok(Some(expected)), "{request}");
        }
        assert_eq!(command("Find a bug in this project"), Ok(None));
        assert_eq!(command("Find a skill in this project"), Ok(None));
        assert_eq!(command(r"Find skills in C:\Users\Private"), Ok(None));
        assert_eq!(command("Find a skill for the styles located in this repo"), Ok(None));
        assert_eq!(command("Use the formatting skill to check this file"), Ok(None));
        assert!(command("/skills private\nnotes").is_err());
        assert!(repository("https://github.com.example.com/a/b").is_err());
        assert!(repository("a/../b").is_err());
    }

    #[test]
    fn parses_agent_skill_front_matter_without_interpreting_body() {
        let content = "---\nname: quality-check\ndescription: >-\n  Read the scoped source\n  and inspect tests.\nmetadata:\n  version: 1\n---\n# Quality";
        assert_eq!(metadata(content), Ok(("quality-check".into(), "Read the scoped source and inspect tests.".into())));
        assert!(metadata(&content.replace("quality-check", "../quality")).is_err());
        assert!(metadata(&content.replace("metadata:", "name: duplicate")).is_err());
    }

    const SAMPLE_BLOB: &str = "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391";

    #[test]
    fn uses_git_object_identity_for_the_downloaded_bytes() {
        assert_eq!(git_blob_hash(b""), SAMPLE_BLOB);
        assert_ne!(git_blob_hash(b"changed"), SAMPLE_BLOB);
    }

    fn tree_with(path: &str, kind: &str, mode: &str) -> GitTree {
        serde_json::from_value(json!({"truncated":false,"tree":[
            {"path":"skills/quality/SKILL.md","type":"blob","mode":"100644","size":50,"sha":SAMPLE_BLOB},
            {"path":"skills/quality/references","type":"tree","mode":"040000","sha":SAMPLE_BLOB},
            {"path":"skills/quality/references/notes.md","type":"blob","mode":"100644","size":20,"sha":SAMPLE_BLOB},
            {"path":path,"type":kind,"mode":mode,"size":10,"sha":SAMPLE_BLOB},
        ]})).unwrap()
    }

    #[test]
    fn validates_the_entire_pinned_folder_before_download() {
        let manifest = "skills/quality/SKILL.md";
        let valid = tree_with("skills/other/SKILL.md", "blob", "100644");
        assert_eq!(package_entries(&valid, manifest).unwrap(), vec![("SKILL.md",50,SAMPLE_BLOB), ("references/notes.md",20,SAMPLE_BLOB)]);
        for (path, kind, mode) in [
            ("skills/quality/../evil", "blob", "100644"),
            ("skills/quality/aux.txt", "blob", "100644"),
            ("skills/quality/CONOUT$", "blob", "100644"),
            ("skills/quality/references/NOTES.md", "blob", "100644"),
            ("skills/quality/REFERENCES", "tree", "040000"),
            ("skills/quality/a/link", "blob", "120000"),
            ("skills/quality/vendor", "commit", "160000"),
            ("skills/quality/file:stream", "blob", "100644"),
            ("skills/quality/.paige-install.json", "blob", "100644"),
        ] {
            assert!(package_entries(&tree_with(path, kind, mode), manifest).is_err(), "{path}");
        }
    }

    #[test]
    fn installs_and_reopens_full_package_without_executing_or_rewriting_it() {
        let root = std::env::temp_dir().join(format!("paige-skill-test-{}", random_id().unwrap())).join("skills");
        let make = |commit: &str| {
            let bytes = vec![b"---\nname: quality\ndescription: Check code.\n---\nRead [notes](references/notes.md), [keys](keyboard.md), and scripts/check.ps1".to_vec(),
                b"Safety checklist".to_vec(), b"Keyboard checklist".to_vec(), b"throw 'This must never execute'".to_vec()];
            let files = ["SKILL.md", "references/notes.md", "keyboard.md", "scripts/check.ps1"].into_iter().zip(&bytes)
                .map(|(path, bytes)| FileRecord { path:path.into(), size:bytes.len(), sha256:digest(bytes) }).collect();
            Package {
                token: random_id().unwrap(), created_at: Instant::now(),
                skill: SkillInfo {
                    id: format!("github-{}", "a".repeat(24)), name: "quality".into(), description: "Check code.".into(),
                    source: "https://github.com/example/skills".into(), repository: "example/skills".into(),
                    commit: commit.into(), manifest: "skills/quality/SKILL.md".into(),
                },
                files, bytes,
            }
        };
        let first = persist(&root, make(&"a".repeat(40))).unwrap();
        let readback = list_installed(&root).unwrap();
        assert_eq!(readback[0].id, first.skill.id);
        let destination = root.join(&first.skill.id);
        for record in &first.files { checked_file(&destination, record).unwrap(); }
        let context = installed_guidance(&destination, &first).unwrap();
        assert_eq!(context["references"], json!({"references/notes.md":"Safety checklist","keyboard.md":"Keyboard checklist"}));
        assert_eq!(persist(&root, make(&"a".repeat(40))).unwrap().installed_at, first.installed_at);
        assert!(persist(&root, make(&"b".repeat(40))).is_err());
        fs::write(destination.join("references").join("notes.md"), "Changed").unwrap();
        assert!(installed_guidance(&destination, &first).is_err());
        assert!(persist(&root, make(&"a".repeat(40))).is_err());
        fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }
}
