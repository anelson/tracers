//! This module provides a fast but not cryptographically secure hashing algorithm and helper
//! functions which are used to detect changes in Rust code.
//!
//! To maximize build time, this crate is very aggressive at caching and reuse.  To make this easy
//! and avoid the possibility of mistakenly re-using a cached result when something has changed, a
//! fast hashing algorithm is used.  This doesn't need to be cryptographically secure but it does
//! need to be very fast.

use fasthash::xx::hash64;
use proc_macro2::TokenStream;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub(crate) type HashCode = u64;

pub(crate) fn hash_string(string: &str) -> HashCode {
    hash_buf(string.as_bytes())
}

pub(crate) fn hash_buf(buf: &[u8]) -> HashCode {
    hash64(buf)
}

pub(crate) fn hash_token_stream(stream: &TokenStream) -> HashCode {
    hash_string(&stream.to_string())
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
    use quote::quote;

    #[test]
    fn hash_string_equality() {
        assert_eq!(hash_string("foo"), hash_string(&"foo".to_owned()));
    }

    #[test]
    fn hash_string_inequality() {
        assert_ne!(hash_string("foo"), hash_string("foo1"));
    }

    #[test]
    fn hash_token_stream_equality() {
        let code1 = quote! {
            #[foo(bar baz boo)]
            trait MyTrait {}
        };
        let code2 = quote! {
            #[foo(bar baz boo)]
            trait MyTrait {}
        };

        assert_eq!(hash_token_stream(&code1), hash_token_stream(&code2));
    }

    #[test]
    fn hash_token_stream_inequality() {
        let code1 = quote! {
            #[foo(bar baz boo)]
            trait MyTrait {}
        };
        let code2 = quote! {
            #[foo(bar baz boo)]
            // Even comments will cause a mismatch
            trait MyTrait {}
        };

        assert_eq!(hash_token_stream(&code1), hash_token_stream(&code2));
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
