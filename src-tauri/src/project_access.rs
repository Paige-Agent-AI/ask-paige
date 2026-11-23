use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::ErrorKind,
    os::windows::io::AsRawHandle,
    path::Path,
};
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_BASIC_INFO, FILE_ID_INFO,
        FileBasicInfo, FileIdInfo, GetFileInformationByHandleEx,
    },
};

const STORE_NAME: &str = "project-access.v1.dpapi";
const STORE_FORMAT: &str = "paige-remembered-projects-v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct FolderIdentity {
    volume: u64,
    file: [u8; 16],
    created: i64,
}

impl FolderIdentity {
    pub(crate) fn read(directory: &File) -> Result<Self, String> {
        let handle = HANDLE(directory.as_raw_handle());
        let mut file = FILE_ID_INFO::default();
        let mut basic = FILE_BASIC_INFO::default();
        // The handle is held by the broker and points to the actual directory.
        unsafe {
            GetFileInformationByHandleEx(handle, FileIdInfo, (&mut file as *mut FILE_ID_INFO).cast(),
                std::mem::size_of::<FILE_ID_INFO>() as u32)
                .and_then(|()| GetFileInformationByHandleEx(handle, FileBasicInfo, (&mut basic as *mut FILE_BASIC_INFO).cast(),
                    std::mem::size_of::<FILE_BASIC_INFO>() as u32))
        }.map_err(|_| "Windows could not identify this folder for remembered access.")?;
        if file.FileId.Identifier == [0; 16] || basic.CreationTime <= 0
            || basic.FileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 == 0
            || basic.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
        {
            return Err("This folder cannot be identified reliably for remembered access.".into());
        }
        Ok(Self { volume: file.VolumeSerialNumber, file: file.FileId.Identifier, created: basic.CreationTime })
    }
}

#[derive(Deserialize, Serialize)]
struct AccessFile {
    format: String,
    projects: BTreeMap<String, FolderIdentity>,
}

pub(crate) fn load() -> Result<BTreeMap<String, FolderIdentity>, String> {
    if crate::broker::uses_test_confirmation() {
        #[cfg(feature = "insecure-test-bypass")]
        if let Some(settings) = test_fixture_settings() {
            let mut projects = load_at(&settings)?;
            projects.retain(|root, _| can_remember_test_project(root));
            return Ok(projects);
        }
        return Ok(BTreeMap::new());
    }
    load_at(&crate::secrets::config_dir()?)
}

fn load_at(dir: &Path) -> Result<BTreeMap<String, FolderIdentity>, String> {
    let bytes = match fs::read(dir.join(STORE_NAME)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(format!("Could not read remembered project access: {error}")),
    };
    let data = crate::secrets::dpapi_unprotect(&bytes)
        .map_err(|_| "Windows could not verify remembered project access.")?;
    let file: AccessFile = serde_json::from_slice(&data)
        .map_err(|_| "Remembered project access is invalid.")?;
    if file.format != STORE_FORMAT {
        return Err("Remembered project access uses an unsupported format.".into());
    }
    Ok(file.projects)
}

pub(crate) fn lock_for_publication() -> Result<File, String> {
    lock_at(&crate::secrets::config_dir()?, false)
}

#[cfg(feature = "insecure-test-bypass")]
fn test_fixture_settings() -> Option<std::path::PathBuf> {
    if std::env::var("PAIGE_TEST_REMEMBER_FIXTURE").as_deref() != Ok("1") {
        return None;
    }
    let override_dir = std::env::var("PAIGE_TEST_CONFIG_DIR").ok().filter(|dir| !dir.is_empty())?;
    let settings = fs::canonicalize(override_dir).ok()?;
    let parent = settings.parent()?.file_name()?.to_str()?;
    (settings.ends_with("settings") && parent.starts_with("continuation-")).then_some(settings)
}

pub(crate) fn can_remember_test_project(root: &str) -> bool {
    #[cfg(feature = "insecure-test-bypass")]
    {
        let (Some(settings), Ok(root)) = (test_fixture_settings(), fs::canonicalize(root)) else {
            return false;
        };
        root.ends_with("engineering-worktree") && settings.parent() == root.parent()
    }
    #[cfg(not(feature = "insecure-test-bypass"))]
    { let _ = root; false }
}

pub(crate) fn update(root: &str, identity: Option<FolderIdentity>) -> Result<(), String> {
    if crate::broker::uses_test_confirmation() && !can_remember_test_project(root) {
        return if identity.is_none() { Ok(()) } else { Err("Test approval is session-only.".into()) };
    }
    update_at(&crate::secrets::config_dir()?, root, identity, None)
}

pub(crate) fn forget_replaced(root: &str, previous: &FolderIdentity) -> Result<(), String> {
    update_at(&crate::secrets::config_dir()?, root, None, Some(previous))
}

fn lock_at(dir: &Path, exclusive: bool) -> Result<File, String> {
    let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false)
        .open(dir.join("project-access.lock")).map_err(|error| error.to_string())?;
    let result = if exclusive { lock.lock() } else { lock.lock_shared() };
    result.map_err(|error| format!("Could not lock remembered project access: {error}"))?;
    Ok(lock)
}

fn update_at(dir: &Path, root: &str, identity: Option<FolderIdentity>, expected: Option<&FolderIdentity>) -> Result<(), String> {
    let _lock = lock_at(dir, true)?;
    let mut projects = load_at(dir)?;
    if expected.is_some_and(|expected| projects.get(root) != Some(expected)) {
        return Ok(());
    }
    match identity {
        Some(identity) => { projects.insert(root.to_owned(), identity); }
        None if projects.remove(root).is_none() => return Ok(()),
        None => {}
    }
    let bytes = serde_json::to_vec(&AccessFile { format: STORE_FORMAT.into(), projects })
        .map_err(|error| error.to_string())?;
    let sealed = crate::secrets::dpapi_protect(&bytes)
        .map_err(|_| "Windows could not protect the remembered project.")?;

    let mut nonce = [0u8; 16];
    crate::broker::os_random_bytes(&mut nonce)?;
    let pending = dir.join(format!("project-access-{}.pending", hex::encode(nonce)));
    crate::broker::write_temp_file(&pending, &sealed)
        .map_err(|error| format!("Could not save remembered project access: {error}"))?;
    if let Err(error) = fs::rename(&pending, dir.join(STORE_NAME)) {
        let _ = fs::remove_file(&pending);
        return Err(format!("Could not save remembered project access: {error}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembered_access_round_trips_and_rejects_a_replaced_folder() {
        let mut random = [0u8; 16];
        crate::broker::os_random_bytes(&mut random).unwrap();
        let config = std::env::temp_dir().join(format!("paige-project-access-{}", hex::encode(random)));
        fs::create_dir(&config).unwrap();
        let root = config.join("project");
        fs::create_dir(&root).unwrap();
        let canonical = fs::canonicalize(&root).unwrap();
        let folder = crate::broker::hold_project_directory(&canonical).unwrap();
        let identity = FolderIdentity::read(&folder).unwrap();
        drop(folder);

        let key = canonical.to_str().unwrap();
        assert!(load_at(&config).unwrap().is_empty());
        update_at(&config, key, Some(identity.clone()), None).unwrap();
        assert_eq!(load_at(&config).unwrap().get(key), Some(&identity));

        fs::rename(&root, config.join("original")).unwrap();
        fs::create_dir(&root).unwrap();
        let replacement = crate::broker::hold_project_directory(&canonical).unwrap();
        let replacement_identity = FolderIdentity::read(&replacement).unwrap();
        assert_ne!(replacement_identity, identity);
        drop(replacement);

        update_at(&config, key, None, Some(&replacement_identity)).unwrap();
        assert_eq!(load_at(&config).unwrap().get(key), Some(&identity));
        update_at(&config, key, None, Some(&identity)).unwrap();
        assert!(load_at(&config).unwrap().is_empty());
        fs::write(config.join(STORE_NAME), b"unencrypted authorization").unwrap();
        assert!(load_at(&config).is_err());
        fs::remove_file(config.join(STORE_NAME)).unwrap();
        fs::remove_file(config.join("project-access.lock")).unwrap();
        fs::remove_dir(config.join("original")).unwrap();
        fs::remove_dir(root).unwrap();
        fs::remove_dir(config).unwrap();
    }
}
