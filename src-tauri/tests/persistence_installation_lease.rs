use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

use relay_pool_desktop_lib::{InstallationLease, LeaseError};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[test]
fn second_installation_lease_fails_without_mutating_data_store() {
    let root = temp_installation();
    let config_dir = root.config_dir();
    let first = InstallationLease::try_acquire(&config_dir).expect("first lease");
    let before = snapshot_tree(root.path());

    let error = InstallationLease::try_acquire(&config_dir).unwrap_err();

    assert!(matches!(error, LeaseError::AlreadyRunning));
    assert_eq!(snapshot_tree(root.path()), before);
    drop(first);
    InstallationLease::try_acquire(&config_dir).expect("released by file handle");
}

#[test]
fn child_process_installation_lease_blocks_second_helper_until_release() {
    let root = temp_installation();

    let mut holder = HelperHolder::new(spawn_helper("hold", root.path()));
    wait_for_helper_ready(root.path());
    let before = snapshot_tree(root.path());

    let contended = spawn_helper("probe", root.path());
    assert_eq!(
        contended.wait_with_output().expect("probe").status.code(),
        Some(23)
    );
    assert_eq!(snapshot_tree(root.path()), before);

    holder.stop();

    let probe = spawn_helper("probe", root.path());
    assert!(probe
        .wait_with_output()
        .expect("probe after release")
        .status
        .success());
}

#[test]
fn persistence_installation_lease_helper() {
    let mode = match std::env::var("PERSISTENCE_INSTALLATION_LEASE_HELPER_MODE") {
        Ok(mode) => mode,
        Err(_) => return,
    };
    let root = std::env::var_os("PERSISTENCE_INSTALLATION_LEASE_ROOT")
        .map(PathBuf::from)
        .expect("helper root");
    let config_dir = root.join("config");

    match mode.as_str() {
        "hold" => {
            let _lease = InstallationLease::try_acquire(&config_dir).expect("hold lease");
            fs::write(root.join("holder-ready"), b"ready").expect("write ready marker");
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }
        "probe" => match InstallationLease::try_acquire(&config_dir) {
            Ok(_lease) => std::process::exit(0),
            Err(LeaseError::AlreadyRunning) => std::process::exit(23),
            Err(error) => panic!("unexpected helper error: {error:?}"),
        },
        other => panic!("unexpected helper mode: {other}"),
    }
}

fn spawn_helper(mode: &str, root: &Path) -> Child {
    let mut command = Command::new(std::env::current_exe().expect("current exe"));
    command
        .args([
            "--exact",
            "persistence_installation_lease_helper",
            "--nocapture",
        ])
        .env("PERSISTENCE_INSTALLATION_LEASE_HELPER_MODE", mode)
        .env("PERSISTENCE_INSTALLATION_LEASE_ROOT", root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.spawn().expect("spawn helper")
}

struct HelperHolder {
    child: Option<Child>,
}

impl HelperHolder {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for HelperHolder {
    fn drop(&mut self) {
        self.stop();
    }
}

fn wait_for_helper_ready(root: &Path) {
    for _ in 0..300 {
        if root.join("holder-ready").exists() {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    panic!("helper did not acquire lease");
}

fn temp_installation() -> TempInstallation {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("relay-pool-installation-lease-{id}"));
    if root.exists() {
        fs::remove_dir_all(&root).expect("clean stale temp root");
    }
    fs::create_dir_all(&root).expect("create temp root");
    TempInstallation { root }
}

fn snapshot_tree(root: &Path) -> Vec<String> {
    let mut entries = Vec::new();
    snapshot_tree_inner(root, root, &mut entries);
    entries.sort();
    entries
}

fn snapshot_tree_inner(root: &Path, current: &Path, out: &mut Vec<String>) {
    let Ok(read_dir) = fs::read_dir(current) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .expect("relative path")
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            out.push(format!("{relative}/"));
            snapshot_tree_inner(root, &path, out);
        } else {
            out.push(relative);
        }
    }
}

struct TempInstallation {
    root: PathBuf,
}

impl TempInstallation {
    fn path(&self) -> &Path {
        &self.root
    }

    fn config_dir(&self) -> PathBuf {
        self.root.join("config")
    }
}
