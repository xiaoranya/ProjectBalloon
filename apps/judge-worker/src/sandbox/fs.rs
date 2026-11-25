use std::path::Path;

#[cfg(unix)]
pub(super) fn read_regular_output_no_follow(
    path: &Path,
) -> Result<Option<Vec<u8>>, std::io::Error> {
    use std::{fs::OpenOptions, io::Read, os::unix::fs::OpenOptionsExt};

    let mut file = match OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        // Linux returns ELOOP for O_NOFOLLOW on a symbolic link. Treat it as invalid output.
        Err(error) if error.raw_os_error() == Some(libc::ELOOP) => return Ok(None),
        Err(error) => return Err(error),
    };
    if !file.metadata()?.file_type().is_file() {
        return Ok(None);
    }
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    Ok(Some(contents))
}

#[cfg(not(unix))]
pub(super) fn read_regular_output_no_follow(
    path: &Path,
) -> Result<Option<Vec<u8>>, std::io::Error> {
    use std::{fs::OpenOptions, io::Read};

    let mut file = match OpenOptions::new().read(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !file.metadata()?.file_type().is_file() {
        return Ok(None);
    }
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    Ok(Some(contents))
}

/// Copies trusted test-case input into a destination that lives inside the
/// contestant-writable `/work` bind without ever following a symlink there.
///
/// `tokio::fs::copy`/`std::fs::copy` resolve the destination path in the
/// host namespace and follow symlinks, so a contestant could replace
/// `/work/current.in` with a link to `~/.ssh/authorized_keys` (or any other
/// host file the worker can open) and have the next test case overwrite it.
/// This helper unlinks the previous directory entry first and creates a
/// brand-new regular file (`O_CREAT|O_EXCL` plus `O_NOFOLLOW`), then applies
/// permissions through the open descriptor instead of re-resolving the path.
pub(super) async fn copy_regular_source_no_follow(
    source: &Path,
    destination: &Path,
) -> Result<(), std::io::Error> {
    let source = source.to_owned();
    let destination = destination.to_owned();
    tokio::task::spawn_blocking(move || copy_regular_source_no_follow_sync(&source, &destination))
        .await
        .map_err(std::io::Error::other)?
}

#[cfg(unix)]
fn copy_regular_source_no_follow_sync(
    source: &Path,
    destination: &Path,
) -> Result<(), std::io::Error> {
    use std::{
        fs::{File, OpenOptions},
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };

    // `remove_file` unlinks the directory entry itself; it never follows the
    // symlink, so the link target is untouched.
    match std::fs::remove_file(destination) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(with_path_context(error, "remove previous test-case input", destination));
        }
    }
    let mut source = File::open(source)
        .map_err(|error| with_path_context(error, "open test-case input", source))?;
    // `create_new` refuses any existing node — including a dangling symlink
    // planted between the unlink and this open — and `O_NOFOLLOW` keeps the
    // no-symlink guarantee explicit.
    let mut target = OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(destination)
        .map_err(|error| with_path_context(error, "create test-case input", destination))?;
    if !target
        .metadata()
        .map_err(|error| with_path_context(error, "inspect test-case input", destination))?
        .file_type()
        .is_file()
    {
        return Err(std::io::Error::other(format!(
            "test-case input {} was not created as a regular file",
            destination.display()
        )));
    }
    target.set_permissions(std::fs::Permissions::from_mode(0o600)).map_err(|error| {
        with_path_context(error, "set test-case input permissions", destination)
    })?;
    std::io::copy(&mut source, &mut target)
        .map_err(|error| with_path_context(error, "copy test-case input", destination))?;
    Ok(())
}

#[cfg(not(unix))]
fn copy_regular_source_no_follow_sync(
    source: &Path,
    destination: &Path,
) -> Result<(), std::io::Error> {
    match std::fs::remove_file(destination) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(with_path_context(error, "remove previous test-case input", destination));
        }
    }
    std::fs::copy(source, destination)
        .map(|_| ())
        .map_err(|error| with_path_context(error, "copy test-case input", destination))
}

/// Reads at most `max_bytes` from a regular file without following symlinks.
/// `Ok(None)` means the path is absent, a symlink, or not a regular file.
///
/// The diagnostic files this reads are written by the contestant, so a
/// symlink must never be resolved in the host namespace: `program.err ->
/// /proc/self/environ` would otherwise expose the worker's own credentials.
/// `O_NONBLOCK` additionally keeps a planted FIFO from blocking the worker.
pub(super) async fn read_regular_file_no_follow_bounded(
    path: &Path,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, std::io::Error> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || read_regular_file_no_follow_bounded_sync(&path, max_bytes))
        .await
        .map_err(std::io::Error::other)?
}

#[cfg(unix)]
fn read_regular_file_no_follow_bounded_sync(
    path: &Path,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, std::io::Error> {
    use std::{fs::OpenOptions, io::Read, os::unix::fs::OpenOptionsExt};

    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        // Linux returns ELOOP for O_NOFOLLOW on a symbolic link.
        Err(error) if error.raw_os_error() == Some(libc::ELOOP) => return Ok(None),
        Err(error) => return Err(error),
    };
    if !file.metadata()?.file_type().is_file() {
        return Ok(None);
    }
    let mut contents = Vec::new();
    (&mut file).take(max_bytes as u64).read_to_end(&mut contents)?;
    Ok(Some(contents))
}

#[cfg(not(unix))]
fn read_regular_file_no_follow_bounded_sync(
    path: &Path,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, std::io::Error> {
    use std::{fs::File, io::Read};

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !file.metadata()?.file_type().is_file() {
        return Ok(None);
    }
    let mut contents = Vec::new();
    (&mut file).take(max_bytes as u64).read_to_end(&mut contents)?;
    Ok(Some(contents))
}

pub(super) fn truncate_log(log: &str, max_bytes: usize) -> String {
    let end = log
        .char_indices()
        .take_while(|(index, character)| index + character.len_utf8() <= max_bytes)
        .last()
        .map_or(0, |(index, character)| index + character.len_utf8());
    log[..end].to_owned()
}

/// Attaches the failing operation and path to a filesystem error while
/// preserving its [`std::io::ErrorKind`], so operator-facing judgement logs
/// say `open /jobs/…/data/1.out: No such file or directory` instead of a bare
/// `os error 2`. Every `tokio::fs` / `std::fs` call site whose error reaches
/// the compile log goes through this.
pub(crate) fn with_path_context(
    error: std::io::Error,
    operation: &'static str,
    path: &Path,
) -> std::io::Error {
    std::io::Error::new(error.kind(), format!("{operation} {}: {error}", path.display()))
}

pub(super) fn nonempty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

pub(super) async fn remove_dir_if_present(path: &Path) -> Result<(), std::io::Error> {
    match tokio::fs::remove_dir_all(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(with_path_context(error, "remove job directory", path)),
    }
}

pub(super) async fn remove_file_if_present(path: &Path) -> Result<(), std::io::Error> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(with_path_context(error, "remove file", path)),
    }
}

pub(super) async fn create_private_dir(path: &Path) -> Result<(), std::io::Error> {
    tokio::fs::create_dir_all(path)
        .await
        .map_err(|error| with_path_context(error, "create private directory", path))?;
    #[cfg(unix)]
    tokio::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o700))
        .await
        .map_err(|error| with_path_context(error, "set private directory permissions", path))?;
    Ok(())
}

pub(super) async fn set_private_file_permissions(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    tokio::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))
        .await
        .map_err(|error| with_path_context(error, "set file permissions", path))?;
    Ok(())
}

pub(super) async fn set_executable_file_permissions(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    tokio::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o700))
        .await
        .map_err(|error| with_path_context(error, "set executable permissions", path))?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::{
        fs,
        os::unix::fs::symlink,
        path::{Path, PathBuf},
    };

    use super::{copy_regular_source_no_follow_sync, read_regular_file_no_follow_bounded_sync};

    struct TempDirectory(PathBuf);

    impl TempDirectory {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!("pb-fs-{label}-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).expect("create test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _cleanup = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn input_copy_replaces_a_planted_symlink_without_touching_its_target() {
        let directory = TempDirectory::new("copy");
        let victim = directory.path().join("authorized_keys");
        fs::write(&victim, b"do not touch").expect("write victim");
        let destination = directory.path().join("current.in");
        symlink(&victim, &destination).expect("plant symlink");
        let source = directory.path().join("1.in");
        fs::write(&source, b"test input").expect("write source");

        copy_regular_source_no_follow_sync(&source, &destination).expect("copy input");

        assert_eq!(fs::read(&victim).expect("read victim"), b"do not touch");
        assert_eq!(fs::read(&destination).expect("read destination"), b"test input");
        assert!(
            !fs::symlink_metadata(&destination).expect("destination metadata").is_symlink(),
            "the destination must be a freshly created regular file"
        );
    }

    #[test]
    fn bounded_read_refuses_symlinks_and_caps_regular_files() {
        let directory = TempDirectory::new("read");
        let victim = directory.path().join("environ");
        fs::write(&victim, b"SECRET=1").expect("write victim");
        let link = directory.path().join("program.err");
        symlink(&victim, &link).expect("plant symlink");

        assert_eq!(
            read_regular_file_no_follow_bounded_sync(&link, 64).expect("bounded read"),
            None,
            "a symlink must never be resolved in the host namespace"
        );
        assert_eq!(
            read_regular_file_no_follow_bounded_sync(&victim, 4).expect("bounded read"),
            Some(b"SECR".to_vec()),
            "regular files are truncated to the byte budget"
        );
        assert_eq!(
            read_regular_file_no_follow_bounded_sync(&directory.path().join("missing"), 4)
                .expect("bounded read"),
            None
        );
    }
}
