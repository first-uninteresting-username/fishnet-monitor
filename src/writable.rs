//! Startup check that the stats file and the history snapshot can be
//! written. A named volume inherits `/data`'s owner (10001), but a bind
//! mount keeps the host directory's owner; if that isn't writable for us,
//! fishnet can't write its stats file and every snapshot fails. Saying so
//! once at startup puts the cause in the log.

use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
};

/// Returns one warning per directory that can't be written, naming the
/// files in it (as `(what, path)` pairs), the uid and how to fix it. Files
/// that share a directory get one warning. An existing file that can't be
/// opened for writing is reported on its own.
pub fn check(files: &[(&str, &Path)], uid: u32) -> Vec<String> {
    let mut by_dir: BTreeMap<PathBuf, Vec<(&str, &Path)>> = BTreeMap::new();
    for &(what, path) in files {
        let dir = match path.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
            _ => PathBuf::from("."),
        };
        by_dir.entry(dir).or_default().push((what, path));
    }

    let mut warnings = Vec::new();
    for (dir, files) in by_dir {
        let names = files
            .iter()
            .map(|(what, path)| format!("{what} {}", path.display()))
            .collect::<Vec<_>>()
            .join(" and the ");
        if let Err(err) = probe_dir(&dir) {
            warnings.push(format!(
                "{} is not writable by uid {uid} ({err}), so the {names} can't be saved; \
                 make it writable for uid {uid}, for example with `chown {uid} <host dir>` \
                 on a bind mount",
                dir.display()
            ));
            continue;
        }
        for (what, path) in files {
            if let Err(err) = probe_file(path) {
                warnings.push(format!(
                    "{what} {} is not writable by uid {uid} ({err}); \
                     make it writable for uid {uid}",
                    path.display()
                ));
            }
        }
    }
    warnings
}

/// Creates and removes a file in `dir`, which also catches read-only
/// mounts that permission bits don't show.
fn probe_dir(dir: &Path) -> io::Result<()> {
    let probe = dir.join(format!(".fishnet-monitor-probe-{}", std::process::id()));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)?;
    fs::remove_file(&probe)
}

/// Opens an existing file for writing without changing it. A missing file
/// is fine: the directory check already showed it can be created.
fn probe_file(path: &Path) -> io::Result<()> {
    match OpenOptions::new().write(true).open(path) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    /// Permission bits don't stop root, so those tests can't run as root.
    fn is_root() -> bool {
        // SAFETY: geteuid has no preconditions.
        unsafe { libc::geteuid() == 0 }
    }

    #[test]
    fn writable_directory_gives_no_warning() {
        let dir = tempfile::tempdir().unwrap();
        let stats = dir.path().join("fishnet-stats");
        fs::write(&stats, "{}").unwrap();
        let history = dir.path().join("history.json");
        assert_eq!(
            check(&[("stats file", &stats), ("history file", &history)], 10001),
            Vec::<String>::new()
        );
        // The probe file is gone again.
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn missing_directory_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let history = dir.path().join("nope").join("history.json");
        let warnings = check(&[("history file", &history)], 10001);
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("nope is not writable by uid 10001"),
            "{}",
            warnings[0]
        );
    }

    #[test]
    fn read_only_directory_gets_one_warning_naming_both_files() {
        if is_root() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let stats = dir.path().join("fishnet-stats");
        let history = dir.path().join("history.json");
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();

        let warnings = check(&[("stats file", &stats), ("history file", &history)], 10001);
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(warnings.len(), 1, "{warnings:?}");
        let w = &warnings[0];
        assert!(
            w.starts_with(&format!(
                "{} is not writable by uid 10001",
                dir.path().display()
            )),
            "{w}"
        );
        assert!(
            w.contains("stats file ") && w.contains("fishnet-stats"),
            "{w}"
        );
        assert!(
            w.contains("history file ") && w.contains("history.json"),
            "{w}"
        );
        assert!(w.contains("chown 10001"), "{w}");
    }

    #[test]
    fn read_only_file_in_a_writable_directory_is_reported() {
        if is_root() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let stats = dir.path().join("fishnet-stats");
        fs::write(&stats, "{}").unwrap();
        fs::set_permissions(&stats, fs::Permissions::from_mode(0o444)).unwrap();

        let warnings = check(&[("stats file", &stats)], 10001);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].starts_with("stats file "), "{}", warnings[0]);
        // Opening it for writing didn't touch it.
        assert_eq!(fs::read_to_string(&stats).unwrap(), "{}");
    }
}
