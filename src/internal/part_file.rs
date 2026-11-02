//! The temporary file `render_to_file` (async and blocking) writes the PDF
//! into, renamed over the target only once the PDF is complete, so that a
//! failed call leaves the target as it was.

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
