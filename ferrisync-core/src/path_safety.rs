use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Validate that `untrusted` is a relative path safely contained within `root`.
///
/// Rejects:
/// - Absolute paths
/// - `..` components (directory traversal)
/// - Empty components (e.g. `foo//bar`)
/// - Null bytes
/// - Paths that resolve outside `root`
///
/// Returns the resolved absolute path on success.
pub fn safe_join(root: &Path, untrusted: &str) -> Result<PathBuf> {
    if untrusted.contains('\0') {
        bail!("path contains null byte");
    }

    let p = Path::new(untrusted);
    if p.is_absolute() {
        bail!("absolute path rejected: {untrusted}");
    }

    for component in p.components() {
        use std::path::Component::*;
        match component {
            ParentDir => bail!("path traversal rejected: {untrusted}",
            ),
            Normal(c) if c.is_empty() => bail!("empty path component in: {untrusted}"),
            _ => {}
        }
    }

    let joined = root.join(untrusted);

    // The root must exist and be a directory.
    let root_canonical = root
        .canonicalize()
        .with_context(|| format!("sync folder does not exist: {}", root.display()))?;
    if !root_canonical.is_dir() {
        bail!("sync folder is not a directory: {}", root.display());
    }

    // The resolved path must be within root.  For files that don't yet exist
    // (write path), canonicalize the parent chain instead.
    let resolved = if joined.exists() {
        joined.canonicalize().with_context(|| {
            format!("could not resolve path: {}", joined.display())
        })?
    } else {
        // Walk up until we find an existing ancestor, canonicalize it,
        // then re-join the remaining relative components.
        let mut remaining = Vec::new();
        let mut cursor = joined.as_path();
        while !cursor.exists() {
            match cursor.parent() {
                Some(parent) if parent != cursor => {
                    remaining.push(cursor.file_name().unwrap());
                    cursor = parent;
                }
                _ => break,
            }
        }
        let base = cursor
            .canonicalize()
            .with_context(|| format!("could not resolve parent: {}", cursor.display()))?;
        let mut resolved = base;
        for comp in remaining.into_iter().rev() {
            resolved = resolved.join(comp);
        }
        resolved
    };

    if !resolved.starts_with(&root_canonical) {
        bail!(
            "path escapes sync folder: {} resolves outside {}",
            untrusted,
            root.display()
        );
    }

    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp() -> PathBuf {
        let d = PathBuf::from(format!(
            "/tmp/path_safety_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn normal_relative_path() {
        let root = tmp();
        let result = safe_join(&root, "foo/bar.txt");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), root.join("foo/bar.txt"));
    }

    #[test]
    fn rejects_absolute_path() {
        let root = tmp();
        assert!(safe_join(&root, "/etc/passwd").is_err());
    }

    #[test]
    fn rejects_dotdot() {
        let root = tmp();
        assert!(safe_join(&root, "../etc/passwd").is_err());
    }

    #[test]
    fn rejects_embedded_dotdot() {
        let root = tmp();
        assert!(safe_join(&root, "foo/../../etc/passwd").is_err());
    }

    #[test]
    fn rejects_dotdot_in_middle() {
        let root = tmp();
        assert!(safe_join(&root, "a/../../../b").is_err());
    }

    #[test]
    fn rejects_null_byte() {
        let root = tmp();
        assert!(safe_join(&root, "foo\0bar").is_err());
    }

    #[test]
    fn allows_dot_components() {
        let root = tmp();
        let result = safe_join(&root, "foo/./bar.txt");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), root.join("foo/./bar.txt"));
    }

    #[test]
    fn nested_dotdot_rejected() {
        let root = tmp();
        assert!(safe_join(&root, "sub/../../secret").is_err());
    }
}
