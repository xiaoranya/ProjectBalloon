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
