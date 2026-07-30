use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileIdentity {
    pub(crate) volume_serial: Option<u64>,
    pub(crate) file_id: Option<u128>,
    pub(crate) length: u64,
    pub(crate) sha256: String,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum FileIdentityError {
    #[error("failed to read file identity")]
    Io(#[from] io::Error),
    #[cfg(windows)]
    #[error("failed to query Windows file identity")]
    WindowsIdentity(io::Error),
}

pub(crate) fn identity_for_path(path: &Path) -> Result<FileIdentity, FileIdentityError> {
    let mut file = File::open(path)?;
    identity_for_file(&mut file)
}

pub(crate) fn identity_for_file(file: &mut File) -> Result<FileIdentity, FileIdentityError> {
    let metadata = file.metadata()?;
    let (volume_serial, file_id) = platform_identity(file)?;
    file.seek(SeekFrom::Start(0))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(FileIdentity {
        volume_serial,
        file_id,
        length: metadata.len(),
        sha256: format!("{:x}", hasher.finalize()),
    })
}

#[cfg(windows)]
fn platform_identity(file: &File) -> Result<(Option<u64>, Option<u128>), FileIdentityError> {
    use std::{mem, os::windows::io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    let mut info = unsafe { mem::zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) };
    if ok == 0 {
        return Err(FileIdentityError::WindowsIdentity(
            io::Error::last_os_error(),
        ));
    }
    let file_id = ((info.nFileIndexHigh as u128) << 32) | info.nFileIndexLow as u128;
    Ok((Some(info.dwVolumeSerialNumber as u64), Some(file_id)))
}

#[cfg(not(windows))]
fn platform_identity(_file: &File) -> Result<(Option<u64>, Option<u128>), FileIdentityError> {
    Ok((None, None))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::identity_for_path;

    #[test]
    fn identity_contains_length_and_sha256() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("identity.txt");
        std::fs::write(&path, b"identity-canary").expect("write");

        let identity = identity_for_path(&path).expect("identity");

        assert_eq!(identity.length, 15);
        assert_eq!(
            identity.sha256,
            "b835579bad041a30c1c09c947818e448c49b76a04366426335e6ff1fa65c4ab2"
        );
        #[cfg(windows)]
        {
            assert!(identity.volume_serial.is_some());
            assert!(identity.file_id.is_some());
        }
    }

    #[test]
    fn identity_hash_changes_after_rewrite() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("identity.txt");
        std::fs::write(&path, b"before").expect("write before");
        let before = identity_for_path(&path).expect("before");

        let mut file = std::fs::File::create(&path).expect("open rewrite");
        file.write_all(b"after").expect("write after");
        file.sync_all().expect("sync");
        drop(file);
        let after = identity_for_path(&path).expect("after");

        assert_ne!(before.sha256, after.sha256);
        assert_eq!(after.length, 5);
    }
}
