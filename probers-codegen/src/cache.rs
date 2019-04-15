//! Implements a simple filesystem-based caching system, where the results of idempotent
//! computations on either filesystem or `TokenStream` inputs are cached in a file system directory
//! (intended to be the `OUT_DIR` of a Cargo build).
#![allow(dead_code)] //TODO: Only temporary

use crate::hashing::*;
use failure::{format_err, Fallible};
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
pub fn cache_file_computation<T: Serialize + DeserializeOwned, F: FnOnce(&str) -> Fallible<T>>(
    cache_path: &Path,
    input_path: &Path,
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

    let results_path = cached_results_path(cache_path, input_path, hash);

    //Try to load a cached results file.  If it doesn't exist or there's any kind of error loading
    //it, just invoke the function again
    load_cached_results::<T>(&results_path).or_else(|_| {
        str::from_utf8(&content)
            .map_err(|e| {
                format_err!(
                    "Input file file {} contains invalid UTF-8 text: {}",
                    results_path.display(),
                    e
                )
            })
            .and_then(|str_contents| f(str_contents))
            .and_then(|result| {
                //Result was computed; serialize it back
                save_results::<T>(&results_path, &result).and(Ok(result))
            })
    })
}

fn load_cached_results<T: Serialize + DeserializeOwned>(results_path: &Path) -> Fallible<T> {
    let file = File::open(results_path)?;
    let reader = BufReader::new(file);

    serde_json::from_reader(reader).map_err(|e| e.into())
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
fn cached_results_path(cache_path: &Path, input_path: &Path, hash: HashCode) -> PathBuf {
    //There is no need to preseve the entire input path.  If the hashes match then they have the
    //same content; we include the file name of the input path only to aid in debugging
    let input_with_hash = add_hash_to_path(input_path, hash);
    let input_name = input_with_hash
        .file_name()
        .expect("Input path is missing a file name");

    cache_path.join(input_name)
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
    fn caches_results() {
        let root_dir = tempfile::tempdir().unwrap();
        let cache_dir = root_dir.path().join("cache");
        let data_dir = root_dir.path().join("data");
        let input_path = data_dir.join("input.txt");

        std::fs::create_dir_all(&data_dir).unwrap();

        let mut input_file = File::create(&input_path).unwrap();
        write!(&mut input_file, "Fear is the mind killer").unwrap();
        drop(input_file);

        let mut compute_count: usize = 0;

        //Invoke the computation for the first time; closure should be called and the result saved
        let result: TestResult = cache_file_computation(&cache_dir, &input_path, |input| {
            compute_count += 1;
            Ok(TestResult {
                answer: input.len(),
            })
        })
        .unwrap();

        assert_eq!(1, compute_count);
        assert_eq!("Fear is the mind killer".len(), result.answer);

        //Now invoke again; the result should have been cached
        let result: TestResult = cache_file_computation(&cache_dir, &input_path, |input| {
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

        let result: TestResult = cache_file_computation(&cache_dir, &input_path, |input| {
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
        let result: TestResult = cache_file_computation(&cache_dir, &input_path, |input| {
            compute_count += 1;
            Ok(TestResult {
                answer: input.len(),
            })
        })
        .unwrap();

        assert_eq!(2, compute_count);
        assert_eq!("Fear is the mind killer".len(), result.answer);
    }
}
