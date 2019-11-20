//! Implements a simple filesystem-based caching system, where the results of idempotent
//! computations on either filesystem or `TokenStream` inputs are cached in a file system directory
//! (intended to be the `OUT_DIR` of a Cargo build).
use crate::hashing::*;
use failure::{bail, format_err, Fallible};
use serde::{de::DeserializeOwned, Serialize};
use std::fs::File;
use std::io::Read;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::str;

/// Given the path to a file, and a function that takes as an argument a `String` with the contents
/// of that file and returns the (serializable) result of some computation on those contents,
/// implements a cache whereby first the cache directory is checked to see if a previous operation
/// has been attempted on the same file contents.  If found, that version is returned and the
/// function is never invoked.
pub(crate) fn cache_file_computation<
    T: Serialize + DeserializeOwned,
    F: FnOnce(&str) -> Fallible<T>,
>(
    cache_path: &Path,
    input_path: &Path,
    key: &str,
    f: F,
) -> Fallible<T> {
    //NB: astute readers may notice that this unconditionally loads and hashes the input file,
    //without trying to check if the date/time metadata are older than the latest cached entry.
    //This is intentional. It simplifies the code, and the hashing algorithm we use runs at 6GB/s
    //on a crappy Core i5 (https://github.com/Cyan4973/xxHash).  For the kind of workloads are are
    //caching, that's a rounding error
    let mut file = File::open(input_path)?;
    let mut content = Vec::new();
    file.read_to_end(&mut content)?;

    let hash = hash_buf(&content);

    let results_path = cached_results_path(input_path, key, hash);

    //Try to load a cached results file.  If it doesn't exist or there's any kind of error loading
    //it, just invoke the function again
    let abs_path = cache_generated_file(cache_path, &results_path, |abs_path| {
        str::from_utf8(&content)
            .map_err(|e| {
                format_err!(
                    "Input file {} contains invalid UTF-8 text: {}",
                    results_path.display(),
                    e
                )
            })
            .and_then(|str_contents| f(str_contents))
            .and_then(|result| {
                //Result was computed; serialize it back
                save_results::<T>(&abs_path, &result).and(Ok(abs_path))
            })
    })?;

    load_cached_results::<T>(&abs_path)
}

/// Similar to `cache_file_computation`, except the computation is not on the contents of a file
/// but rather on some arbitrary object with a name and a hash.  If the computation has previously
/// been performed on the same name and hash, the previous result is returned.  Otherwise the
/// computation is performed and saved in the cache for future invocations.
pub(crate) fn cache_object_computation<
    T: Serialize + DeserializeOwned,
    F: FnOnce() -> Fallible<T>,
>(
    cache_path: &Path,
    object_name: &str,
    hash: HashCode,
    key: &str,
    f: F,
) -> Fallible<T> {
    //Just like the file scenario, use the object's hash to detect changes
    let results_path = cached_results_path(Path::new(object_name), key, hash);

    //Try to load a cached results file.  If it doesn't exist or there's any kind of error loading
    //it, just invoke the function again
    let abs_path = cache_generated_file(cache_path, &results_path, |abs_path| {
        f().and_then(|result| {
            //Result was computed; serialize it back
            save_results::<T>(&abs_path, &result).and(Ok(abs_path))
        })
    })?;

    load_cached_results::<T>(&abs_path)
}

/// Identical to `cache_object_computation` except this is read-only; if the computation does not
/// exist in the cache it returns an error
pub(crate) fn get_cached_object_computation<T: Serialize + DeserializeOwned>(
    cache_path: &Path,
    object_name: &str,
    hash: HashCode,
    key: &str,
) -> Fallible<T> {
    //Just like the file scenario, use the object's hash to detect changes
    let results_path = cached_results_path(Path::new(object_name), key, hash);

    //Try to load a cached results file.  If it doesn't exist or there's any kind of error loading
    //it, just invoke the function again
    let abs_path = cache_path.join(results_path);
    load_cached_results::<T>(&abs_path)
}

/// Lower-level caching function.  Given some arbitrary file name (and optional path components)
/// relative to the cache path, if the file exists, returns the fully qualified path to the file in
/// the cache, if not, it passes that fully qualified path to the provided closure, and if that
/// closure succeeds and the file was created, then this function returns the fully qualified path.
///
/// If the closure returns an error, or if it returns success but the file still doesn't exist,
/// this function fails
pub(crate) fn cache_generated_file<F: FnOnce(PathBuf) -> Fallible<PathBuf>>(
    cache_path: &Path,
    results_path: &Path,
    f: F,
) -> Fallible<PathBuf> {
    let abs_path = cache_path.join(results_path);

    if abs_path.exists() {
        Ok(abs_path)
    } else {
        let abs_path = f(abs_path)?;

        if abs_path.exists() {
            Ok(abs_path)
        } else {
            bail!(
                "The result file {} was not created as expected",
                abs_path.display()
            )
        }
    }
}

/// Given the path to some root directory, generates a path to a subdirectory which is suitable for
/// use as a cache.  This automatically adds the version of the crate to the path to ensure caches
/// are invalidated whenever a new version is released
pub(crate) fn get_cache_path(root: &Path) -> PathBuf {
    let mut root = root.to_owned();
    root.push(concat!(
        env!("CARGO_PKG_NAME"),
        "-",
        env!("CARGO_PKG_VERSION")
    ));
    root.push("cache");
    root
}

fn load_cached_results<T: Serialize + DeserializeOwned>(results_path: &Path) -> Fallible<T> {
    let file = File::open(results_path)?;
    let reader = BufReader::new(file);

    serde_json::from_reader(reader).map_err(std::convert::Into::into) //convert the error to a failure-compatible type
}

fn save_results<T: Serialize + DeserializeOwned>(results_path: &Path, results: &T) -> Fallible<()> {
    //Make sure the directory exists
    results_path
        .parent()
        .map(|p| {
            std::fs::create_dir_all(p)
                .map_err(|e| format_err!("Error creating output directory {}: {}", p.display(), e))
        })
        .unwrap_or(Ok(()))?;

    let file = File::create(results_path).map_err(|e| {
        format_err!(
            "Error creating cached results file {}: {}",
            results_path.display(),
            e
        )
    })?;
    let writer = BufWriter::new(file);
    serde_json::to_writer(writer, results).map_err(|e| {
        format_err!(
            "Error saving cached results to {}: {}",
            results_path.display(),
            e
        )
    })
}

/// Compute the path of the directory within the cache path which contains all results related to a
/// specific input path and hash.
fn cached_results_path(input_path: &Path, key: &str, hash: HashCode) -> PathBuf {
    //There is no need to preseve the entire input path.  If the hashes match then they have the
    //same content; we include the file name of the input path only to aid in debugging
    let input_with_hash = add_hash_to_path(input_path, hash);
    let input_name = input_with_hash
        .file_name()
        .expect("Input path is missing a file name");

    PathBuf::from(input_name).join(format!("{}.json", key))
}

#[cfg(test)]
mod test {
    use super::*;
    use serde::Deserialize;
    use std::io::Write;

    #[derive(Deserialize, Serialize, Debug)]
    struct TestResult {
        answer: usize,
    }

    #[test]
    fn caches_file_results() {
        let key = "mylib.c";
        let root_dir = tempfile::tempdir().unwrap();
        let cache_dir = get_cache_path(root_dir.path());
        let data_dir = root_dir.path().join("data");
        let input_path = data_dir.join("input.txt");

        std::fs::create_dir_all(&data_dir).unwrap();

        let mut input_file = File::create(&input_path).unwrap();
        write!(&mut input_file, "Fear is the mind killer").unwrap();
        drop(input_file);

        let mut compute_count: usize = 0;

        //Invoke the computation for the first time; closure should be called and the result saved
        let result: TestResult = cache_file_computation(&cache_dir, &input_path, key, |input| {
            compute_count += 1;
            Ok(TestResult {
                answer: input.len(),
            })
        })
        .unwrap();

        assert_eq!(1, compute_count);
        assert_eq!("Fear is the mind killer".len(), result.answer);

        //Now invoke again; the result should have been cached
        let result: TestResult = cache_file_computation(&cache_dir, &input_path, key, |input| {
            compute_count += 1;
            Ok(TestResult {
                answer: input.len(),
            })
        })
        .unwrap();

        assert_eq!(1, compute_count);
        assert_eq!("Fear is the mind killer".len(), result.answer);

        //Now overwrite the contents of the input file and expect the result to be re-computed
        let mut input_file = File::create(&input_path).unwrap();
        write!(&mut input_file, "I will face my fear").unwrap();
        drop(input_file);

        let result: TestResult = cache_file_computation(&cache_dir, &input_path, key, |input| {
            compute_count += 1;
            Ok(TestResult {
                answer: input.len(),
            })
        })
        .unwrap();

        assert_eq!(2, compute_count);
        assert_eq!("I will face my fear".len(), result.answer);

        //Finally, put back the original content.  The cached result from that version should still
        //be on the filesystem and should still be used
        let mut input_file = File::create(&input_path).unwrap();
        write!(&mut input_file, "Fear is the mind killer").unwrap();
        drop(input_file);

        //Invoke the computation for the first time; closure should be called and the result saved
        let result: TestResult = cache_file_computation(&cache_dir, &input_path, key, |input| {
            compute_count += 1;
            Ok(TestResult {
                answer: input.len(),
            })
        })
        .unwrap();

        assert_eq!(2, compute_count);
        assert_eq!("Fear is the mind killer".len(), result.answer);
    }

    #[test]
    fn caches_objects_results() {
        let key = "mylib.c";
        let root_dir = tempfile::tempdir().unwrap();
        let cache_dir = root_dir.path().join("cache");

        let hash_foo1: HashCode = 5;
        let hash_foo2: HashCode = 6;

        let mut compute_count: usize = 0;

        //Invoke the computation for the first time; closure should be called and the result saved
        let result: TestResult =
            cache_object_computation(&cache_dir, "foo", hash_foo1, key, || {
                compute_count += 1;
                Ok(TestResult {
                    answer: hash_foo1 as usize,
                })
            })
            .unwrap();

        assert_eq!(1, compute_count);
        assert_eq!(hash_foo1 as usize, result.answer);

        //Now invoke again; the result should have been cached
        let result: TestResult =
            cache_object_computation(&cache_dir, "foo", hash_foo1, key, || {
                compute_count += 1;
                Ok(TestResult {
                    answer: hash_foo1 as usize,
                })
            })
            .unwrap();

        assert_eq!(1, compute_count);
        assert_eq!(hash_foo1 as usize, result.answer);

        //Now use another object with a different hash; a new result should be computed
        let result: TestResult =
            cache_object_computation(&cache_dir, "foo", hash_foo2, key, || {
                compute_count += 1;
                Ok(TestResult {
                    answer: hash_foo2 as usize,
                })
            })
            .unwrap();

        assert_eq!(2, compute_count);
        assert_eq!(hash_foo2 as usize, result.answer);

        //Finally, use another instance of the object that should have the same content, and
        //thus use the cached result
        let result: TestResult =
            cache_object_computation(&cache_dir, "foo", hash_foo1, key, || {
                compute_count += 1;
                Ok(TestResult {
                    answer: hash_foo1 as usize,
                })
            })
            .unwrap();

        assert_eq!(2, compute_count);
        assert_eq!(hash_foo1 as usize, result.answer);
    }

    #[test]
    fn caches_generated_data() {
        let key = "mylib.c";
        let root_dir = tempfile::tempdir().unwrap();
        let cache_dir = root_dir.path().join("cache");

        let hash_foo1: HashCode = 5;

        //Query the cached computation.  It's not happened yet so that should be an error
        let result: Fallible<TestResult> =
            get_cached_object_computation(&cache_dir, "foo", hash_foo1, key);
        assert!(result.is_err());

        //Invoke the computation for the first time; closure should be called and the result saved
        cache_object_computation(&cache_dir, "foo", hash_foo1, key, || {
            Ok(TestResult {
                answer: hash_foo1 as usize,
            })
        })
        .unwrap();

        //Now the computation is cached; it should return
        let result: Fallible<TestResult> =
            get_cached_object_computation(&cache_dir, "foo", hash_foo1, key);
        assert!(result.is_ok());

        assert_eq!(hash_foo1 as usize, result.unwrap().answer);
    }
}
