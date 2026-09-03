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
            ParentDir => bail!("path traversal rejected: {untrusted}",),
            Normal(c) if c.is_empty() => bail!("empty path component in: {untrusted}"),
            _ => {}
        }
    }

    let joined = root.join(untrusted);

    // Reject any symlink in the path.  Walk every ancestor from root down
    // to the target and call symlink_metadata (lstat) which does NOT follow
    // symlinks.  If any component is a symlink, bail — this prevents
    // symlink-based escapes even if canonicalize() would later catch them.
    {
        let mut cursor = joined.as_path();
        loop {
            if cursor == root {
                break;
            }
            match std::fs::symlink_metadata(cursor) {
                Ok(meta) => {
                    if meta.file_type().is_symlink() {
                        bail!("symlink rejected: {}", cursor.display());
                    }
                    // Not a symlink — check parent
                }
                Err(_) => {
                    // Doesn't exist yet — check parent to see if
                    // an existing ancestor is a symlink.
                }
            }
            match cursor.parent() {
                Some(p) if p != cursor => cursor = p,
                _ => break,
            }
        }
    }

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
        joined
            .canonicalize()
            .with_context(|| format!("could not resolve path: {}", joined.display()))?
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
    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

    fn tmp() -> PathBuf {
        // A unique directory per call (not reused), so path_safety tests can
        // run concurrently without one test's cleanup removing another's root.
        let d = PathBuf::from(format!(
            "/tmp/path_safety_test_{}_{}",
            std::process::id(),
            TMP_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
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

    #[test]
    fn rejects_symlink_file() {
        let root = tmp();
        let target = root.join("target.txt");
        fs::write(&target, "secret").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, root.join("link.txt")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&target, root.join("link.txt")).unwrap();
        assert!(
            safe_join(&root, "link.txt").is_err(),
            "should reject symlink path"
        );
    }

    #[test]
    fn rejects_symlink_in_middle_of_path() {
        let root = tmp();
        let outside = PathBuf::from("/tmp/symlink_escape_target");
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "data").unwrap();
        fs::create_dir_all(root.join("sub")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join("sub/link")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&outside, root.join("sub/link")).unwrap();
        // Even reading through a symlink in a middle component should be rejected
        assert!(
            safe_join(&root, "sub/link/secret.txt").is_err(),
            "should reject path with symlink component"
        );
    }
}
