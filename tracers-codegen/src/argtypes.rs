//! Almost all of the probing code takes advantage of Rust's type system to ensure any type passed
//! to a probe can be represented as a C type, and that the appropriate conversion, if any, is
//! applied before calling the C code.
//!
//! Except, unfortunately, in the code that operates on a Rust AST as represented by `syn`.  This
//! isn't actually compiled Rust, it's only parsed Rust, so something like `Option<&str>` doesn't
//! have a type, it's basically just a slightly more structured version of a string.
//!
//! Thus, in order to resolve a `syn::Type` to a corresponding Rust type requires some manual
//! hard-coding work.  That work is done here.
//!
//! Anyone who is extending this crate to support additional types, or even just type aliases, must
//! update the `from_syn_type` function accordingly
#[cfg(unix)]
use std::ffi::{OsStr, OsString};

use std::ffi::{CStr, CString};

use tracers_core::argtypes::*;
use tracers_core::{ProbeArgType, ProbeArgWrapper};
use syn::parse_quote;

macro_rules! maybe_type {
    ($syn_t:expr, $rust_t:ty) => {
        let rust_syn_t: syn::Type = parse_quote! { $rust_t };
        if *$syn_t == rust_syn_t {
            return Some(ArgTypeInfo::new::<$rust_t>());
        }
    };
    (@naked $syn_t:expr, $rust_t:ty) => {
        maybe_type!($syn_t, $rust_t);
    };
    (@ref $syn_t:expr, $rust_t:ty) => {
        maybe_type!($syn_t, &$rust_t);
    };
    (@opt $syn_t:expr, $rust_t:ty) => {
        maybe_type!($syn_t, &Option<$rust_t>);
    };
    (@opt_ref $syn_t:expr, $rust_t:ty) => {
        maybe_type!($syn_t, &Option<&$rust_t>);
    };
    (@ptr $syn_t:expr, $rust_t:ty) => {
        maybe_type!($syn_t, *const $rust_t);
    };
    (@primitive $syn_t:expr, $rust_t:ty) => {
        maybe_type!(@naked $syn_t, $rust_t);
        maybe_type!(@ref $syn_t, $rust_t);
        maybe_type!(@opt $syn_t, $rust_t);
        maybe_type!(@opt_ref $syn_t, $rust_t);
        maybe_type!(@ptr $syn_t, $rust_t);
    };
    (@string $syn_t:expr, $rust_t:ty) => {
        maybe_type!(@naked $syn_t, $rust_t);
        maybe_type!(@opt $syn_t, $rust_t);
    };
}

macro_rules! maybe_types {
    ($syn_t:expr, $($rust_t:ty),+) => {
        $(
            maybe_type!($syn_t, $rust_t);
        )*
    };
    (@$tag:ident $syn_t:expr, $($rust_t:ty),+) => {
        $(
            maybe_type!(@$tag $syn_t, $rust_t);
        )*
    };
}

/// Given a type expression from a Rust AST, tries to get the type information for that type.
/// If it can't be resolved, returns `None`
///
/// This function has a massive cyclomatic complexity due to all of the macro-generated code, but
/// in this case it's safe to ignore the clippy lint.
#[allow(clippy::cyclomatic_complexity)]
pub(crate) fn from_syn_type(ty: &syn::Type) -> Option<ArgTypeInfo> {
    //TODO: There HAS to be a better and more performant way to do this, but working with the syn
    //type hierarchy directly is just agony
    maybe_types!(@primitive ty, i8, u8, i16, u16, i32, u32, i64, u64, usize, isize);
    maybe_types!(@string ty, &str, &String);

    #[cfg(unix)] // Only the unix impl of OsStr/OsString exposes the string as bytes
    maybe_types!(@string ty, &OsStr, &OsString);
    maybe_types!(@string ty, &CStr, &CString);

    maybe_type!(@primitive ty, bool);

    //Else, this isn't a type we recognize
    None
}

#[derive(Debug, PartialEq)]
pub(crate) struct ArgTypeInfo {
    c_type: CType,
    c_type_str: &'static str,
    rust_type_str: &'static str,
}

#[allow(dead_code)] //TODO: temporary
impl ArgTypeInfo {
    pub fn new<T: ProbeArgType<T>>() -> ArgTypeInfo {
        ArgTypeInfo {
            c_type: <<<T as ProbeArgType<T>>::WrapperType as ProbeArgWrapper>::CType as ProbeArgNativeTypeInfo>::get_c_type(),
            c_type_str: <<<T as ProbeArgType<T>>::WrapperType as ProbeArgWrapper>::CType as ProbeArgNativeTypeInfo>::get_c_type_str(),
            rust_type_str: <<<T as ProbeArgType<T>>::WrapperType as ProbeArgWrapper>::CType as ProbeArgNativeTypeInfo>::get_rust_type_str()
        }
    }

    /// Gets the `CType` enum which corresponds to the C data type which is used to represent this
    /// type when calling probes
    fn get_c_type_enum(&self) -> CType {
        self.c_type.clone()
    }

    /// Gets a string which contains the C type for use generating C/C++ code.  For example if the
    /// `CType` is `VoidPtr`, this function returns `void *`
    fn get_c_type_str(&self) -> &'static str {
        self.c_type_str
    }

    /// Gets a string containing the Rust type corresponding to the native C type.  This also is
    /// used for generating code.  It corresponds directly to the type which the `ProbeArgWrapper`
    /// returns and passes to the auto-generated Rust bindings.
    ///
    /// That means that as long as each of the C types in `get_c_type_str` correctly match the
    /// corresponding Rust types returned by this function, the Rust type system will ensure there
    /// are no errors.
    ///
    /// For example, if somewhere else in the code we have a bug whereby `&str` is passed as
    /// `void*`, but this code thinks it should be `char*`, when Rust compiles the call to the
    /// generated Rust bindings it will fail.
    fn get_rust_type_str(&self) -> &'static str {
        self.rust_type_str
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::testdata::*;

    macro_rules! test_type {
        ($rust_t:ty, $c_type:expr, $rust_type_str:expr) => {
            let syn_typ: syn::Type = parse_quote! { $rust_t };
            assert_eq!(
                Some(ArgTypeInfo {
                    c_type: $c_type,
                    c_type_str: $c_type.into(),
                    rust_type_str: $rust_type_str,
                }),
                from_syn_type(&syn_typ),
                "Got unexpected arg type info for type expression '{}'", stringify!($rust_t)
            );
        };
        (@naked $rust_t:ty, $c_type:expr, $rust_type_str:expr) => {
            test_type!($rust_t, $c_type, $rust_type_str);
        };
        // Primitive types marshal refs the same as the value type
        (@primitive_ref $rust_t:ty, $c_type:expr, $rust_type_str:expr) => {
            test_type!(&$rust_t, $c_type, $rust_type_str);
        };
        // Primitive types marshal optionals the same as the value type (using the default value
        // for None)
        (@primitive_opt $rust_t:ty, $c_type:expr, $rust_type_str:expr) => {
            test_type!(&Option<$rust_t>, $c_type, $rust_type_str);
        };
        // Primitive types marshal Option<&..> the same also
        (@primitive_opt_ref $rust_t:ty, $c_type:expr, $rust_type_str:expr) => {
            test_type!(&Option<&$rust_t>, $c_type, $rust_type_str);
        };
        // Primitive pointers are just cast to void pointers, except for the char types
        (@primitive_ptr $rust_t:ty, $c_type:expr, $rust_type_str:expr) => {
            test_type!(*const $rust_t, CType::VoidPtr, "*const std::os::raw::c_void");
        };
        (@primitive_ptr i8, $c_type:expr, $rust_type_str:expr) => {
            test_type!(*const $rust_t, CType::CharPtr, "*const std::os::raw::c_char");
        };
        (@primitive_ptr u8, $c_type:expr, $rust_type_str:expr) => {
            test_type!(*const $rust_t, CType::UCharPtr, "*const std::os::raw::c_uchar");
        };
        (@primitive $rust_t:ty, $c_type:expr, $rust_type_str:expr) => {
            test_type!(@naked $rust_t, $c_type, $rust_type_str);
            test_type!(@primitive_ptr $rust_t, $c_type, $rust_type_str);
            test_type!(@primitive_ref $rust_t, $c_type, $rust_type_str);
            test_type!(@primitive_opt $rust_t, $c_type, $rust_type_str);
            test_type!(@primitive_opt_ref $rust_t, $c_type, $rust_type_str);
        };
        (@string $rust_t:ty, $c_type:expr, $rust_type_str:expr) => {
            test_type!(@naked $rust_t, $c_type, $rust_type_str);
            test_type!(@primitive_opt $rust_t, $c_type, $rust_type_str);
        };
    }

    #[test]
    fn test_type_support() {
        test_type!(@primitive i8, CType::Char, "std::os::raw::c_char");
        test_type!(@primitive u8, CType::UChar, "std::os::raw::c_uchar");
        test_type!(@primitive i16, CType::Short, "std::os::raw::c_short");
        test_type!(@primitive u16, CType::UShort, "std::os::raw::c_ushort");
        test_type!(@primitive i32, CType::Int, "std::os::raw::c_int");
        test_type!(@primitive u32, CType::UInt, "std::os::raw::c_uint");
        test_type!(@primitive i64, CType::LongLong, "std::os::raw::c_longlong");
        test_type!(@primitive u64, CType::ULongLong, "std::os::raw::c_ulonglong");
        test_type!(@primitive usize, CType::SizeT, "libc::size_t");
        test_type!(@primitive isize, CType::SSizeT, "libc::ssize_t");
        test_type!(@primitive bool, CType::Int, "std::os::raw::c_int");

        test_type!(@string &str, CType::CharPtr, "*const std::os::raw::c_char");
        test_type!(@string &String, CType::CharPtr, "*const std::os::raw::c_char");

        #[cfg(unix)] // Only the unix impl of OsStr/OsString exposes the string as bytes
        test_type!(@string &OsStr, CType::CharPtr, "*const std::os::raw::c_char");
        #[cfg(unix)] // Only the unix impl of OsStr/OsString exposes the string as bytes
        test_type!(@string &OsString, CType::CharPtr, "*const std::os::raw::c_char");

        test_type!(@string &CStr, CType::CharPtr, "*const std::os::raw::c_char");
        test_type!(@string &CString, CType::CharPtr, "*const std::os::raw::c_char");
    }

    #[test]
    fn test_support_for_all_test_traits() {
        //Anything in our corpus of valid provider traits should correspond to a known type
        //Plus, this way if I ever go and add support for a new type, if I add an example of it to
        //the test cases, this test will fail, which will remind me I need to enable support for
        //that type here.
        for test_trait in
            get_test_provider_traits(|t: &TestProviderTrait| t.expected_error.is_none()).into_iter()
        {
            //The test data don't tell us anything about what the expected C wrapper type of each
            //arg will be.  Nor should they; that's what this module's tests are for.
            //The purpose of this test is to ensure there are no probe args in any of the traits
            //which are expected to be valid, that cannot be identified from their `syn::Type`
            //representation
            for probe in test_trait.probes.unwrap().into_iter() {
                for (name, rust_syn_type, c_type) in probe.args.into_iter() {
                    let arg_type_info = from_syn_type(&rust_syn_type);

                    assert_ne!(None, arg_type_info,
                               "test trait '{}' probe '{}' arg '{}' has a type which `from_syn_type` can't identify",
                               test_trait.description,
                               probe.name,
                               name);

                    let arg_type_info = arg_type_info.unwrap();
                    assert_eq!(c_type, arg_type_info.get_c_type_enum(),
                               "test trait '{}' probe '{}' arg '{}' has a type for which `from_syn_type` returned an incorrect `CType` (and, probably, other wrapper types also)",
                               test_trait.description,
                               probe.name,
                               name);
                }
            }
        }
    }
}
