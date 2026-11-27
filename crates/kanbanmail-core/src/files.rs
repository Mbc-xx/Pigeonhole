//! Writing the user's own files (settings, columns, notes…) safely.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;

/// Writes a whole file at once: the new content goes to a temporary file
/// beside it, reaches the disk, then takes the old one's place. A crash or a
/// power cut halfway leaves the old file intact, never a cut one.
pub fn write_atomic(path: &Path, content: impl AsRef<[u8]>) -> Result<()> {
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let name = path.file_name().context("a file name is needed")?.to_string_lossy();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let written = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(content.as_ref())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        std::fs::remove_file(&tmp).ok();
    }
    written.with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_the_file_and_leaves_nothing_behind() {
        let dir = std::env::temp_dir().join(format!("pigeonhole-files-{}", std::process::id()));
        let path = dir.join("sub").join("settings.toml");
        write_atomic(&path, "a = 1\n").unwrap();
        write_atomic(&path, "a = 2\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = 2\n");
        let left: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(left, ["settings.toml"], "no temporary file is left");
        std::fs::remove_dir_all(&dir).ok();
    }
}
