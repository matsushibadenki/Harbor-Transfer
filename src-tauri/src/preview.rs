//! Private snapshots for Quick Look; never pass a remote URL or shell command to macOS.
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
pub const MAX_PREVIEW_BYTES: u64 = 64 * 1024 * 1024;
static SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub fn check_size(size: u64) -> Result<(), String> {
    if size > MAX_PREVIEW_BYTES {
        Err("Quick Look previews are limited to 64 MiB.".into())
    } else {
        Ok(())
    }
}
pub fn cache_name(name: &str) -> Result<&str, String> {
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.len() > 255
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        Err("Invalid preview file name.".into())
    } else {
        Ok(name)
    }
}
pub struct PreviewCache {
    directory: PathBuf,
}
impl PreviewCache {
    pub fn new(parent: &Path) -> Result<Self, String> {
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.recursive(true).create(parent).map_err(|e| e.to_string())?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
        let directory = parent.join(format!(
            "preview-{}-{stamp}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        builder.recursive(false).create(&directory).map_err(|e| e.to_string())?;
        Ok(Self { directory })
    }
    pub fn file(&self, name: &str) -> Result<PathBuf, String> {
        Ok(self.directory.join(cache_name(name)?))
    }
}
impl Drop for PreviewCache {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
pub fn command(path: &Path) -> Result<tokio::process::Command, String> {
    if !path.is_absolute() {
        return Err("Preview paths must be absolute.".into());
    }
    let mut command = tokio::process::Command::new("/usr/bin/qlmanage");
    command
        .arg("-p")
        .arg(path)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    Ok(command)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_are_private_unique_and_removed_on_drop() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("original.txt");
        std::fs::write(&original, "original").unwrap();
        let first = PreviewCache::new(&root.path().join("cache")).unwrap();
        let second = PreviewCache::new(&root.path().join("cache")).unwrap();
        assert_ne!(first.directory, second.directory);
        let path = first.file("original.txt").unwrap();
        std::fs::copy(&original, &path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&first.directory).unwrap().permissions().mode() & 0o777, 0o700);
        }
        drop(first);
        assert!(!path.exists());
        assert!(original.exists());
        assert!(second.directory.exists());
    }
    #[test]
    fn rejects_unsafe_names_and_oversized_files() {
        for name in ["", ".", "..", "../file", "/etc/file", "a\\b", "a\0b", "a\nb"] {
            assert!(cache_name(name).is_err());
        }
        assert_eq!(cache_name("日本語 report.pdf").unwrap(), "日本語 report.pdf");
        assert!(check_size(MAX_PREVIEW_BYTES).is_ok());
        assert!(check_size(MAX_PREVIEW_BYTES + 1).is_err());
    }
    #[test]
    fn uses_a_fixed_program_and_literal_path_argument() {
        let command = command(Path::new("/private/tmp/a;$(command).txt")).unwrap();
        assert_eq!(command.as_std().get_program(), "/usr/bin/qlmanage");
        assert_eq!(command.as_std().get_args().collect::<Vec<_>>(), ["-p", "/private/tmp/a;$(command).txt"]);
        assert!(super::command(Path::new("-p")).is_err());
    }
}
