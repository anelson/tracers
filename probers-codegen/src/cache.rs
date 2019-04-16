//! Implements a simple filesystem-based caching system, where the results of idempotent
//! computations on either filesystem or `TokenStream` inputs are cached in a file system directory
//! (intended to be the `OUT_DIR` of a Cargo build).
#![allow(dead_code)] //TODO: Only temporary

use crate::hashing::*;
use failure::{format_err, Fallible};
use proc_macro2::TokenStream;
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
                    "Input file {} contains invalid UTF-8 text: {}",
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

/// Similar to `cache_file_computation`, except the computation is not on the contents of a file
/// but rather on a `TokenStream`.
pub(crate) fn cache_tokenstream_computation<
    T: Serialize + DeserializeOwned,
    F: FnOnce(&TokenStream) -> Fallible<T>,
>(
    cache_path: &Path,
    token_stream: &TokenStream,
    f: F,
) -> Fallible<T> {
    //Just like the file scenario, use the tokenstream's hash to detect changes
    let hash = hash_token_stream(token_stream);

    let results_path = cached_results_path(cache_path, &Path::new("tokenstream"), hash);

    //Try to load a cached results file.  If it doesn't exist or there's any kind of error loading
    //it, just invoke the function again
    load_cached_results::<T>(&results_path).or_else(|_| {
        f(token_stream).and_then(|result| {
            //Result was computed; serialize it back
            save_results::<T>(&results_path, &result).and(Ok(result))
        })
    })
}

/// Given the path to some root directory, generates a path to a subdirectory which is suitable for
/// use as a cache.  This automatically adds the version of the crate to the path to ensure caches
/// are invalidated whenever a new version is released
pub(crate) fn get_cache_path(root: &Path) -> PathBuf {
    let mut root = root.to_owned();
    root.push("cache");
    root.push(format!(concat!(
        env!("CARGO_PKG_NAME"),
        "-",
        env!("CARGO_PKG_VERSION")
    )));
    root
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
    use quote::quote;
    use serde::Deserialize;
    use std::io::Write;
    use syn::ItemTrait;

    #[derive(Deserialize, Serialize, Debug)]
    struct TestResult {
        answer: usize,
    }

    #[test]
    fn caches_file_results() {
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

    #[test]
    fn caches_tokenstream_results() {
        let root_dir = tempfile::tempdir().unwrap();
        let cache_dir = root_dir.path().join("cache");

        let trait_foo1 = quote! {
            trait FearIsTheMindKiller {}
        };
        let trait_foo2 = quote! {
            #trait_foo1
        };
        let trait_bar = quote! {
            trait IWillFaceMyFear {}
        };

        let mut compute_count: usize = 0;

        //Invoke the computation for the first time; closure should be called and the result saved
        let result: TestResult = cache_tokenstream_computation(&cache_dir, &trait_foo1, |input| {
            compute_count += 1;
            let trait_item = syn::parse2::<ItemTrait>(input.clone()).unwrap();
            Ok(TestResult {
                answer: trait_item.ident.to_string().len(),
            })
        })
        .unwrap();

        assert_eq!(1, compute_count);
        assert_eq!("FearIsTheMindKiller".len(), result.answer);

        //Now invoke again; the result should have been cached
        let result: TestResult = cache_tokenstream_computation(&cache_dir, &trait_foo1, |input| {
            compute_count += 1;
            let trait_item = syn::parse2::<ItemTrait>(input.clone()).unwrap();
            Ok(TestResult {
                answer: trait_item.ident.to_string().len(),
            })
        })
        .unwrap();

        assert_eq!(1, compute_count);
        assert_eq!("FearIsTheMindKiller".len(), result.answer);

        //Now use another trait with a different name; a new result should be computed
        let result: TestResult = cache_tokenstream_computation(&cache_dir, &trait_bar, |input| {
            compute_count += 1;
            let trait_item = syn::parse2::<ItemTrait>(input.clone()).unwrap();
            Ok(TestResult {
                answer: trait_item.ident.to_string().len(),
            })
        })
        .unwrap();

        assert_eq!(2, compute_count);
        assert_eq!("IWillFaceMyFear".len(), result.answer);

        //Finally, use another instance of the TokenStream that should have the same content, and
        //thus use the cached result
        let result: TestResult = cache_tokenstream_computation(&cache_dir, &trait_foo2, |input| {
            compute_count += 1;
            let trait_item = syn::parse2::<ItemTrait>(input.clone()).unwrap();
            Ok(TestResult {
                answer: trait_item.ident.to_string().len(),
            })
        })
        .unwrap();

        assert_eq!(2, compute_count);
        assert_eq!("FearIsTheMindKiller".len(), result.answer);
    }
}
