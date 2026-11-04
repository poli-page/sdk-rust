//! The temporary file `render_to_file` (async and blocking) writes the PDF
//! into, renamed over the target only once the PDF is complete, so that a
//! failed call leaves the target as it was.
//!
//! A symbolic link at the target is resolved first, so the rename lands on
//! the file it points to and the link stays a link. An existing target's
//! permissions are copied onto the temporary file before the rename. Its
//! owner and hard links cannot survive a rename: the target becomes a new
//! file.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use crate::internal::uuid::new_v4_string;
use crate::Error;

/// `io_error`: the file could not be written. The I/O error's own text is
/// the reason, and the error itself the `source()`.
pub(crate) fn io_error(source: io::Error) -> Error {
    Error::Io { source }
}

/// Links followed before giving up, as Linux's `ELOOP` limit.
const MAX_LINKS: usize = 40;

/// The file `render_to_file` writes: `target`, or, when `target` is a
/// symbolic link, the file it points to (followed through every link, as
/// opening the path would). A dangling link resolves to the missing file it
/// names, which is then created.
pub(crate) fn resolve_target(target: &Path) -> io::Result<PathBuf> {
    match std::fs::symlink_metadata(target) {
        Ok(meta) if meta.file_type().is_symlink() => match std::fs::canonicalize(target) {
            Ok(resolved) => Ok(resolved),
            Err(e) if e.kind() == io::ErrorKind::NotFound => follow_dangling(target),
            Err(e) => Err(e),
        },
        // Not a link, or nothing there yet: the path itself.
        _ => Ok(target.to_path_buf()),
    }
}

/// Follows the links from `link` to the first path that is not one, without
/// requiring it to exist. A relative link is read from the link's real
/// directory, as the OS does: `..` in it climbs from there, not from the
/// path as written, which may itself go through a link.
fn follow_dangling(link: &Path) -> io::Result<PathBuf> {
    let mut current = link.to_path_buf();
    for _ in 0..MAX_LINKS {
        let next = std::fs::read_link(&current)?;
        current = if next.is_relative() {
            let dir = match current.parent() {
                Some(dir) if !dir.as_os_str().is_empty() => dir,
                _ => Path::new("."),
            };
            std::fs::canonicalize(dir)?.join(next)
        } else {
            next
        };
        // Spell the result through its real directory when that exists,
        // so the `..` of the link is resolved the way the OS does.
        if let (Some(dir), Some(name)) = (current.parent(), current.file_name()) {
            if let Ok(real_dir) = std::fs::canonicalize(dir) {
                current = real_dir.join(name);
            }
        }
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {}
            _ => return Ok(current),
        }
    }
    Err(io::Error::other(format!(
        "{}: too many levels of symbolic links",
        link.display()
    )))
}

/// Fails when `target` is an existing file this process may not write, so a
/// read-only file is refused before any render and never replaced. Opening
/// it for writing, without truncating, changes nothing.
pub(crate) fn ensure_writable(target: &Path) -> io::Result<()> {
    match std::fs::metadata(target) {
        Ok(meta) if meta.is_file() => std::fs::OpenOptions::new()
            .write(true)
            .open(target)
            .map(drop),
        _ => Ok(()),
    }
}

/// The file to write for the path the caller gave: its symbolic links
/// resolved, and refused when it is an existing file that may not be
/// written.
pub(crate) fn prepare_target(path: &Path) -> io::Result<PathBuf> {
    let target = resolve_target(path)?;
    ensure_writable(&target)?;
    Ok(target)
}

/// Gives the temporary file `part` the permissions of the existing file at
/// `target`, so the rename keeps them. Nothing to do when there is no file.
pub(crate) fn keep_permissions(target: &Path, part: &Path) -> io::Result<()> {
    match std::fs::metadata(target) {
        Ok(meta) if meta.is_file() => std::fs::set_permissions(part, meta.permissions()),
        _ => Ok(()),
    }
}

/// Runs blocking file-system work off the async runtime's threads.
pub(crate) async fn blocking<T, F>(work: F) -> io::Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> io::Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(io::Error::other)?
}

/// The parent directory to create for `target`, if it has one.
pub(crate) fn parent_dir(target: &Path) -> Option<&Path> {
    target.parent().filter(|p| !p.as_os_str().is_empty())
}

/// `.<name>.<uuid>.part` next to `target`: in the same directory, so the
/// final rename stays on one filesystem.
pub(crate) fn part_path(target: &Path) -> io::Result<PathBuf> {
    let name = target.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} has no file name", target.display()),
        )
    })?;
    let mut part = OsString::from(".");
    part.push(name);
    part.push(format!(".{}.part", new_v4_string()));
    Ok(target.with_file_name(part))
}

/// Removes the temporary file when dropped, unless it was renamed into
/// place: an error, a panic or a dropped future does not leave it behind.
/// Only a killed process can.
pub(crate) struct PartGuard {
    path: PathBuf,
    armed: bool,
}

impl PartGuard {
    /// Takes charge of the temporary file at `path`, which exists.
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The file was renamed into place: keep it.
    pub(crate) fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for PartGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_path_is_a_hidden_sibling_of_the_target() {
        let part = part_path(Path::new("out/dir/invoice.pdf")).expect("part path");
        assert_eq!(part.parent(), Some(Path::new("out/dir")));
        let name = part.file_name().and_then(|n| n.to_str()).expect("name");
        assert!(name.starts_with(".invoice.pdf."), "{name}");
        assert_eq!(part.extension().and_then(|e| e.to_str()), Some("part"));
        // Two calls never collide.
        assert_ne!(part, part_path(Path::new("out/dir/invoice.pdf")).unwrap());
    }

    #[test]
    fn part_path_of_a_bare_file_name_is_in_the_current_directory() {
        let part = part_path(Path::new("invoice.pdf")).expect("part path");
        assert_eq!(part.parent(), Some(Path::new("")));
        assert_eq!(parent_dir(Path::new("invoice.pdf")), None);
    }

    #[test]
    fn part_path_needs_a_file_name() {
        let err = part_path(Path::new("out/..")).expect_err("no file name");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        let err = io_error(err);
        assert_eq!(err.code(), "io_error");
        assert_eq!(
            err.message(),
            "Failed to write PDF: out/.. has no file name"
        );
    }

    #[test]
    fn resolve_target_keeps_a_path_that_is_not_a_link() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("new.pdf");
        assert_eq!(resolve_target(&missing).unwrap(), missing);
        let file = dir.path().join("file.pdf");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(resolve_target(&file).unwrap(), file);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_target_follows_links_existing_or_dangling() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().expect("tempdir");
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir(root.join("store")).unwrap();
        std::fs::write(root.join("store/real.pdf"), b"x").unwrap();
        // A chain of two links, the first one relative.
        symlink("hop", root.join("link.pdf")).unwrap();
        symlink(root.join("store/real.pdf"), root.join("hop")).unwrap();
        assert_eq!(
            resolve_target(&root.join("link.pdf")).unwrap(),
            root.join("store/real.pdf")
        );
        // A dangling relative link resolves to the file it names.
        symlink("store/next.pdf", root.join("dangling.pdf")).unwrap();
        assert_eq!(
            resolve_target(&root.join("dangling.pdf")).unwrap(),
            root.join("store/next.pdf")
        );
        // A relative hop climbs from the link's real directory: through
        // `sym -> real/dir`, `sym/link -> ../a.pdf` names `real/a.pdf`.
        std::fs::create_dir_all(root.join("real/dir")).unwrap();
        symlink(root.join("real/dir"), root.join("sym")).unwrap();
        symlink("../a.pdf", root.join("real/dir/link")).unwrap();
        assert_eq!(
            resolve_target(&root.join("sym/link")).unwrap(),
            root.join("real/a.pdf"),
            "dangling"
        );
        std::fs::write(root.join("real/a.pdf"), b"x").unwrap();
        assert_eq!(
            resolve_target(&root.join("sym/link")).unwrap(),
            root.join("real/a.pdf"),
            "existing"
        );
        // A loop is an error, not a hang.
        symlink("loop-b", root.join("loop-a")).unwrap();
        symlink("loop-a", root.join("loop-b")).unwrap();
        assert!(resolve_target(&root.join("loop-a")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn keep_permissions_copies_the_mode_of_an_existing_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let (target, part) = (dir.path().join("t.pdf"), dir.path().join("p.part"));
        std::fs::write(&part, b"x").unwrap();
        keep_permissions(&target, &part).expect("no target: nothing to do");
        std::fs::write(&target, b"x").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
        keep_permissions(&target, &part).unwrap();
        let mode = std::fs::metadata(&part).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o640);
    }

    #[test]
    fn the_guard_removes_the_file_unless_disarmed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let kept = dir.path().join("kept.part");
        let removed = dir.path().join("removed.part");
        std::fs::write(&kept, b"x").unwrap();
        std::fs::write(&removed, b"x").unwrap();
        PartGuard::new(kept.clone()).disarm();
        drop(PartGuard::new(removed.clone()));
        assert!(kept.exists());
        assert!(!removed.exists());
    }
}
