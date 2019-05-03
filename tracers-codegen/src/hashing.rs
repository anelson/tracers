//! This module provides a fast but not cryptographically secure hashing algorithm and helper
//! functions which are used to detect changes in Rust code.
//!
//! To maximize build time, this crate is very aggressive at caching and reuse.  To make this easy
//! and avoid the possibility of mistakenly re-using a cached result when something has changed, a
//! fast hashing algorithm is used.  This doesn't need to be cryptographically secure but it does
//! need to be very fast.

use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use twox_hash::XxHash;

pub(crate) type HashCode = u64;

pub(crate) fn hash_buf(buf: &[u8]) -> HashCode {
    hash(buf)
}

pub(crate) fn hash<T: Hash>(something: T) -> HashCode {
    let mut hasher = XxHash::with_seed(42);
    something.hash(&mut hasher);
    hasher.finish()
}

/// Helper method with takes a path (eg `foo/bar/baz.lib`) and a hash code (eg `0xBADF00D`), and
/// produces a version of that path with the hash added to the file name but the path otherwise
/// unchanged.
///
/// For example:
///
/// ```no_exec
/// use std::path::Path;
///
/// let path = &Path::from("/foo/bar/baz.lib");
/// let hash: HashCode = 0xbadf00d;
///
/// assert_eq!(&Path::new("/foo/bar/baz-badf00d.lib"), add_hash_to_path(&path, hash));
/// ```
pub(crate) fn add_hash_to_path(path: &Path, hash: HashCode) -> PathBuf {
    let mut path = path.to_owned();
    let name = path
        .file_stem()
        .expect("path is missing a file name")
        .to_str()
        .expect("file name contains invalid UTF-8");
    let ext = path
        .extension()
        .unwrap_or_else(|| OsStr::new(""))
        .to_str()
        .expect("file extension contains invalid UTF-8");
    let new_name = &format!("{}-{:x}.{}", name, hash, ext);

    path.set_file_name(OsStr::new(new_name));

    path
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn hash_equality() {
        assert_eq!(hash("foo"), hash(&"foo".to_owned()));
    }

    #[test]
    fn hash_inequality() {
        assert_ne!(hash("foo"), hash("foo1"));
    }

    #[test]
    fn file_name_generation() {
        let path = &Path::new("/foo/bar/baz/mylib.a");
        let hash: HashCode = 0xcafebabedeadbeef;

        assert_eq!(
            Path::new("/foo/bar/baz/mylib-cafebabedeadbeef.a"),
            add_hash_to_path(&path, hash)
        );
    }
}
