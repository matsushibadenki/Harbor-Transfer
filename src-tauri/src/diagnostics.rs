use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const RETAINED_REPORTS: usize = 5;

/// The initial development bundle id ended in `.app`, which macOS tooling
/// treats as an application extension. Preserve existing bookmarks when
/// moving to the release-safe identifier.
pub fn migrate_legacy_data_directory(data_directory: &Path) -> Result<(), String> {
    let Some(parent) = data_directory.parent() else {
        return Ok(());
    };
    let legacy = parent.join("com.harbortransfer.app");
    let database = data_directory.join("harbor-transfer.sqlite3");
    if database.exists() || !legacy.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(data_directory).map_err(|error| error.to_string())?;
    for name in ["harbor-transfer.sqlite3", "harbor-transfer.sqlite3-wal", "harbor-transfer.sqlite3-shm"] {
        let source = legacy.join(name);
        if source.is_file() {
            fs::copy(&source, data_directory.join(name)).map_err(|error| {
                format!("Could not migrate Harbor Transfer data from '{}': {error}", source.display())
            })?;
        }
    }
    Ok(())
}

/// Install a deliberately local-only panic report. Reports contain the app
/// version and source location, but omit panic payloads, server names, paths,
/// credentials, and file contents. Nothing is uploaded automatically.
pub fn install_local_panic_reporter(directory: &Path) {
    let directory = directory.to_path_buf();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        if fs::create_dir_all(&directory).is_ok() {
            let report = directory.join(format!("crash-{timestamp}.log"));
            if let Ok(mut file) = OpenOptions::new().create_new(true).write(true).open(report) {
                let location = info
                    .location()
                    .map(|location| format!("{}:{}:{}", location.file(), location.line(), location.column()))
                    .unwrap_or_else(|| "unknown".to_string());
                let _ = writeln!(file, "Harbor Transfer {}", env!("CARGO_PKG_VERSION"));
                let _ = writeln!(file, "timestamp_unix={timestamp}");
                let _ = writeln!(file, "panic_location={location}");
            }
            let _ = prune_reports(&directory, RETAINED_REPORTS);
        }
        previous(info);
    }));
}

fn prune_reports(directory: &Path, retain: usize) -> std::io::Result<()> {
    let mut reports: Vec<PathBuf> = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("crash-") && name.ends_with(".log"))
        })
        .collect();
    reports.sort();
    let remove_count = reports.len().saturating_sub(retain);
    for report in reports.into_iter().take(remove_count) {
        fs::remove_file(report)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_only_the_newest_reports() {
        let directory = tempfile::tempdir().unwrap();
        for index in 0..8 {
            fs::write(directory.path().join(format!("crash-{index:02}.log")), "safe").unwrap();
        }
        prune_reports(directory.path(), 5).unwrap();
        let mut names: Vec<_> = fs::read_dir(directory.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["crash-03.log", "crash-04.log", "crash-05.log", "crash-06.log", "crash-07.log"]);
    }

    #[test]
    fn migrates_the_legacy_database_without_overwriting_new_data() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join("com.harbortransfer.app");
        let current = root.path().join("com.harbortransfer.desktop");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("harbor-transfer.sqlite3"), "legacy").unwrap();
        migrate_legacy_data_directory(&current).unwrap();
        assert_eq!(fs::read_to_string(current.join("harbor-transfer.sqlite3")).unwrap(), "legacy");

        fs::write(current.join("harbor-transfer.sqlite3"), "current").unwrap();
        migrate_legacy_data_directory(&current).unwrap();
        assert_eq!(fs::read_to_string(current.join("harbor-transfer.sqlite3")).unwrap(), "current");
    }
}
