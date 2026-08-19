use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::file_identity::{identity_for_path, FileIdentity, FileIdentityError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApprovedLeaf {
    parent: PathBuf,
    canonical_parent: PathBuf,
    leaf: OsString,
}

impl ApprovedLeaf {
    pub(crate) fn approve(
        parent: impl AsRef<Path>,
        leaf: impl Into<OsString>,
    ) -> Result<Self, AtomicFileError> {
        let parent = parent.as_ref();
        if !parent.is_absolute() {
            return Err(AtomicFileError::PathRejected);
        }
        let leaf = leaf.into();
        if !is_safe_leaf(&leaf) {
            return Err(AtomicFileError::PathRejected);
        }
        let metadata = fs::symlink_metadata(parent).map_err(AtomicFileError::Io)?;
        if !metadata.is_dir() || is_reparse_or_symlink(&metadata) {
            return Err(AtomicFileError::PathRejected);
        }
        validate_parent_chain(parent)?;
        let canonical_parent = parent.canonicalize().map_err(AtomicFileError::Io)?;
        let approved = Self {
            parent: parent.to_path_buf(),
            canonical_parent,
            leaf,
        };
        approved.target_metadata()?;
        Ok(approved)
    }

    pub(crate) fn path(&self) -> PathBuf {
        self.parent.join(&self.leaf)
    }

    /// Reads the target without following a symlink/reparse point. A missing
    /// leaf is allowed because `CreateNew` materialization uses the same
    /// approved parent for its first publish.
    pub(crate) fn target_metadata(&self) -> Result<Option<fs::Metadata>, AtomicFileError> {
        match fs::symlink_metadata(self.path()) {
            Ok(metadata) if metadata.is_file() && !is_reparse_or_symlink(&metadata) => {
                Ok(Some(metadata))
            }
            Ok(_) => Err(AtomicFileError::PathRejected),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(AtomicFileError::Io(error)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublishEvidence {
    pub(crate) target: PathBuf,
    pub(crate) identity: FileIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublishMode {
    CreateNew,
    ReplaceExisting,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AtomicFileError {
    #[error("approved path rejected")]
    PathRejected,
    #[error("file already exists")]
    AlreadyExists,
    #[error("file is missing")]
    Missing,
    #[error("atomic file I/O failed")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Identity(#[from] FileIdentityError),
}

pub(crate) trait AtomicFilePublishPort {
    fn publish(
        &self,
        prepared: &Path,
        target: &ApprovedLeaf,
        mode: PublishMode,
    ) -> Result<PublishEvidence, AtomicFileError>;
}

pub(crate) trait AtomicJournalPort {
    fn publish_and_readback(
        &self,
        bytes: &[u8],
        target: &ApprovedLeaf,
    ) -> Result<Vec<u8>, AtomicFileError>;
}

pub(crate) trait AtomicDatabaseReplacePort {
    fn replace_with_rollback(
        &self,
        prepared_replacement: &Path,
        active: &ApprovedLeaf,
        rollback: &ApprovedLeaf,
    ) -> Result<DatabaseReplaceEvidence, AtomicFileError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DatabaseReplaceEvidence {
    pub(crate) active: PublishEvidence,
    pub(crate) rollback: PublishEvidence,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct LocalAtomicFileAdapter;

impl AtomicFilePublishPort for LocalAtomicFileAdapter {
    fn publish(
        &self,
        prepared: &Path,
        target: &ApprovedLeaf,
        mode: PublishMode,
    ) -> Result<PublishEvidence, AtomicFileError> {
        publish_prepared(prepared, target, mode)
    }
}

impl AtomicJournalPort for LocalAtomicFileAdapter {
    fn publish_and_readback(
        &self,
        bytes: &[u8],
        target: &ApprovedLeaf,
    ) -> Result<Vec<u8>, AtomicFileError> {
        let prepared = unique_sibling(&target.path(), "journal");
        write_new(&prepared, bytes)?;
        let evidence = publish_prepared(&prepared, target, PublishMode::ReplaceExisting)?;
        let readback = fs::read(&evidence.target)?;
        Ok(readback)
    }
}

impl AtomicDatabaseReplacePort for LocalAtomicFileAdapter {
    fn replace_with_rollback(
        &self,
        prepared_replacement: &Path,
        active: &ApprovedLeaf,
        rollback: &ApprovedLeaf,
    ) -> Result<DatabaseReplaceEvidence, AtomicFileError> {
        replace_database_with_rollback(prepared_replacement, active, rollback)
    }
}

pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<File, AtomicFileError> {
    let mut file = create_new_file(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(file)
}

pub(crate) fn create_new_file(path: &Path) -> Result<File, AtomicFileError> {
    Ok(OpenOptions::new().write(true).create_new(true).open(path)?)
}

pub(crate) fn publish_prepared(
    prepared: &Path,
    target: &ApprovedLeaf,
    mode: PublishMode,
) -> Result<PublishEvidence, AtomicFileError> {
    let target_path = target.path();
    if prepared
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .as_ref()
        != Some(&target.canonical_parent)
    {
        return Err(AtomicFileError::PathRejected);
    }
    validate_prepared_file(prepared)?;
    sync_file(prepared)?;
    match (target.target_metadata()?.is_some(), mode) {
        (true, PublishMode::CreateNew) => return Err(AtomicFileError::AlreadyExists),
        (false, PublishMode::ReplaceExisting) => fs::rename(prepared, &target_path)?,
        (false, PublishMode::CreateNew) => fs::rename(prepared, &target_path)?,
        (true, PublishMode::ReplaceExisting) => replace_existing_file(prepared, &target_path)?,
    }
    sync_parent(&target.parent)?;
    Ok(PublishEvidence {
        identity: identity_for_path(&target_path)?,
        target: target_path,
    })
}

pub(crate) fn replace_database_with_rollback(
    prepared_replacement: &Path,
    active: &ApprovedLeaf,
    rollback: &ApprovedLeaf,
) -> Result<DatabaseReplaceEvidence, AtomicFileError> {
    let active_path = active.path();
    let rollback_path = rollback.path();
    if active.canonical_parent != rollback.canonical_parent {
        return Err(AtomicFileError::PathRejected);
    }
    if prepared_replacement
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .as_ref()
        != Some(&active.canonical_parent)
    {
        return Err(AtomicFileError::PathRejected);
    }
    if active.target_metadata()?.is_none() {
        return Err(AtomicFileError::Missing);
    }
    if rollback.target_metadata()?.is_some() {
        return Err(AtomicFileError::AlreadyExists);
    }
    validate_prepared_file(prepared_replacement)?;

    sync_file(prepared_replacement)?;
    sync_file(&active_path)?;
    replace_existing_file_with_backup(prepared_replacement, &active_path, &rollback_path)?;
    sync_parent(&active.parent)?;
    Ok(DatabaseReplaceEvidence {
        active: PublishEvidence {
            identity: identity_for_path(&active_path)?,
            target: active_path,
        },
        rollback: PublishEvidence {
            identity: identity_for_path(&rollback_path)?,
            target: rollback_path,
        },
    })
}

pub(crate) fn sync_file(path: &Path) -> Result<(), AtomicFileError> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?
        .sync_all()?;
    Ok(())
}

pub(crate) fn unique_sibling(path: &Path, operation: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    path.with_extension(format!("{operation}-{}-{unique}.tmp", std::process::id()))
}

fn is_safe_leaf(leaf: &OsStr) -> bool {
    let leaf_path = Path::new(leaf);
    if leaf_path.is_absolute() {
        return false;
    }
    matches!(
        leaf_path.components().collect::<Vec<_>>().as_slice(),
        [Component::Normal(_)]
    )
}

fn validate_parent_chain(parent: &Path) -> Result<(), AtomicFileError> {
    // Walk from the approved leaf toward the filesystem root.  Constructing
    // a path one Windows prefix component at a time is not reliable for
    // extended (`\\?\`) paths, while ancestor paths preserve the original
    // namespace and can be checked directly with symlink_metadata.
    for ancestor in parent.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(AtomicFileError::Io)?;
        if !metadata.is_dir() || is_reparse_or_symlink(&metadata) {
            return Err(AtomicFileError::PathRejected);
        }
        if ancestor.parent().is_none() {
            break;
        }
    }
    Ok(())
}

fn validate_prepared_file(path: &Path) -> Result<(), AtomicFileError> {
    let metadata = fs::symlink_metadata(path).map_err(AtomicFileError::Io)?;
    if !metadata.is_file() || is_reparse_or_symlink(&metadata) {
        return Err(AtomicFileError::PathRejected);
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse_or_symlink(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_or_symlink(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(windows)]
pub(crate) fn replace_existing_file(
    temporary: &Path,
    destination: &Path,
) -> Result<(), AtomicFileError> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let temporary: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let ok = unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            temporary.as_ptr(),
            ptr::null(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        Err(AtomicFileError::Io(io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub(crate) fn replace_existing_file_with_backup(
    replacement: &Path,
    destination: &Path,
    backup: &Path,
) -> Result<(), AtomicFileError> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let replacement: Vec<u16> = replacement
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let backup: Vec<u16> = backup.as_os_str().encode_wide().chain(Some(0)).collect();
    let ok = unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            replacement.as_ptr(),
            backup.as_ptr(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        Err(AtomicFileError::Io(io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
pub(crate) fn replace_existing_file_with_backup(
    replacement: &Path,
    destination: &Path,
    backup: &Path,
) -> Result<(), AtomicFileError> {
    fs::rename(destination, backup)?;
    fs::rename(replacement, destination)?;
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn replace_existing_file(
    temporary: &Path,
    destination: &Path,
) -> Result<(), AtomicFileError> {
    fs::rename(temporary, destination)?;
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn sync_parent(parent: &Path) -> Result<(), AtomicFileError> {
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(windows)]
pub(crate) fn sync_parent(_parent: &Path) -> Result<(), AtomicFileError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        ApprovedLeaf, AtomicDatabaseReplacePort, AtomicFilePublishPort, AtomicJournalPort,
        LocalAtomicFileAdapter, PublishMode,
    };

    #[test]
    fn publish_create_new_and_reject_stale_overwrite() {
        let root = tempfile::tempdir().expect("tempdir");
        let target = ApprovedLeaf::approve(root.path(), "package.rpd-move").expect("approve");
        let prepared = root.path().join("package.tmp");
        std::fs::write(&prepared, b"first").expect("prepare");

        let adapter = LocalAtomicFileAdapter;
        let evidence = adapter
            .publish(&prepared, &target, PublishMode::CreateNew)
            .expect("publish");
        assert_eq!(std::fs::read(&evidence.target).expect("read"), b"first");

        let stale = root.path().join("stale.tmp");
        std::fs::write(&stale, b"stale").expect("prepare stale");
        let error = adapter
            .publish(&stale, &target, PublishMode::CreateNew)
            .expect_err("stale overwrite rejected");
        assert!(matches!(error, super::AtomicFileError::AlreadyExists));
        assert_eq!(std::fs::read(target.path()).expect("read target"), b"first");
    }

    #[test]
    fn publish_replace_existing_reports_new_identity() {
        let root = tempfile::tempdir().expect("tempdir");
        let target = ApprovedLeaf::approve(root.path(), "config.json").expect("approve");
        std::fs::write(target.path(), b"old").expect("old");
        let prepared = root.path().join("config.tmp");
        std::fs::write(&prepared, b"new").expect("prepare");

        let evidence = LocalAtomicFileAdapter
            .publish(&prepared, &target, PublishMode::ReplaceExisting)
            .expect("replace");

        assert_eq!(std::fs::read(target.path()).expect("read"), b"new");
        assert_eq!(evidence.identity.length, 3);
    }

    #[test]
    fn database_replace_preserves_previous_active_as_rollback() {
        let root = tempfile::tempdir().expect("tempdir");
        let active = ApprovedLeaf::approve(root.path(), "active.sqlite3").expect("active");
        let rollback = ApprovedLeaf::approve(root.path(), "rollback.sqlite3").expect("rollback");
        std::fs::write(active.path(), b"old").expect("old");
        let prepared = root.path().join("staged.sqlite3");
        std::fs::write(&prepared, b"new").expect("new");

        let evidence = LocalAtomicFileAdapter
            .replace_with_rollback(&prepared, &active, &rollback)
            .expect("replace");

        assert_eq!(
            std::fs::read(evidence.active.target).expect("active"),
            b"new"
        );
        assert_eq!(
            std::fs::read(evidence.rollback.target).expect("rollback"),
            b"old"
        );
    }

    #[test]
    fn journal_publish_returns_exact_readback() {
        let root = tempfile::tempdir().expect("tempdir");
        let target = ApprovedLeaf::approve(root.path(), "journal.json").expect("approve");
        let readback = LocalAtomicFileAdapter
            .publish_and_readback(br#"{"phase":"prepared"}"#, &target)
            .expect("journal");

        assert_eq!(readback, br#"{"phase":"prepared"}"#);
    }

    #[test]
    fn approved_leaf_rejects_parent_escape_and_nested_names() {
        let root = tempfile::tempdir().expect("tempdir");

        for leaf in [
            PathBuf::from("nested").join("file.txt").into_os_string(),
            PathBuf::from("..").into_os_string(),
            PathBuf::from(".").into_os_string(),
            root.path().join("absolute.txt").into_os_string(),
        ] {
            let error = ApprovedLeaf::approve(root.path(), leaf).expect_err("reject unsafe leaf");
            assert!(matches!(error, super::AtomicFileError::PathRejected));
        }
    }

    #[test]
    fn publish_rejects_prepared_file_outside_approved_parent() {
        let root = tempfile::tempdir().expect("root");
        let outside = tempfile::tempdir().expect("outside");
        let target = ApprovedLeaf::approve(root.path(), "package.rpd-move").expect("approve");
        let prepared = outside.path().join("package.tmp");
        std::fs::write(&prepared, b"payload").expect("prepared");

        let error = LocalAtomicFileAdapter
            .publish(&prepared, &target, PublishMode::CreateNew)
            .expect_err("prepared file must be sibling");

        assert!(matches!(error, super::AtomicFileError::PathRejected));
        assert!(!target.path().exists());
    }

    #[test]
    fn approved_leaf_rejects_existing_directory_target() {
        let root = tempfile::tempdir().expect("root");
        std::fs::create_dir(root.path().join("config.json")).expect("target directory");

        let error = ApprovedLeaf::approve(root.path(), "config.json")
            .expect_err("a managed document target must be a regular file");
        assert!(matches!(error, super::AtomicFileError::PathRejected));
    }

    #[cfg(unix)]
    #[test]
    fn approved_leaf_rejects_symlink_target_and_parent() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("root");
        let target_file = root.path().join("real.json");
        std::fs::write(&target_file, b"outside").expect("target file");
        symlink(&target_file, root.path().join("config.json")).expect("target symlink");
        let target_error = ApprovedLeaf::approve(root.path(), "config.json")
            .expect_err("symlink target must be rejected");
        assert!(matches!(target_error, super::AtomicFileError::PathRejected));

        let parent = root.path().join("managed");
        let real_parent = root.path().join("real-parent");
        std::fs::create_dir(&real_parent).expect("real parent");
        symlink(&real_parent, &parent).expect("parent symlink");
        let parent_error = ApprovedLeaf::approve(&parent, "config.json")
            .expect_err("symlink parent must be rejected");
        assert!(matches!(parent_error, super::AtomicFileError::PathRejected));
    }
}
