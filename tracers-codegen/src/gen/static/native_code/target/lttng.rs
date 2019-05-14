//! Contains the native C++ code generator and the Rust bindings generator to support Linux
//! SystemTap user-mode tracing
use crate::cache;
use crate::gen::r#static::native_code::NativeCodeGenerator;
use crate::gen::NativeLib;
use crate::spec::ProbeArgSpecification;
use crate::spec::ProbeSpecification;
use crate::spec::ProviderSpecification;
use crate::TracersError;
use crate::TracersResult;
use askama::Template;
use failure::format_err;
use pkg_config;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::prelude::*;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracers_core::argtypes::CType;

/// Holds the important bits from `pkg-config` in a serializable form that we can cache to avoid
/// repetitive lookups
#[derive(Serialize, Deserialize, Clone, Debug)]
struct LttngUstLibInfo {
    libs: Vec<String>,
    link_paths: Vec<PathBuf>,
    include_paths: Vec<PathBuf>,
    defines: HashMap<String, Option<String>>,
}

impl LttngUstLibInfo {
    fn get(cache_dir: &Path) -> TracersResult<Self> {
        cache::cache_object_computation(&cache_dir, "pkg-config", 0, "lttng-ust", move || {
            //Try to find the lttng-udt lib and include dirs with pkgconfig
            //This won't work on all distros; if it does'nt just assume the lib and include paths are
            //already set correctly and try to compile
            if let Ok(lib) = pkg_config::Config::new()
                .cargo_metadata(false)
                .probe("lttng-ust")
            {
                //Found it.
                Ok(LttngUstLibInfo {
                    libs: lib.libs,
                    link_paths: lib.link_paths,
                    include_paths: lib.include_paths,
                    defines: lib.defines,
                })
            } else {
                //Not found with pkgconfig.  Either it's missing, in which case this compile will fail,
                //or it's in the existing library path.
                Ok(LttngUstLibInfo {
                    libs: vec!["lttng-ust".to_owned()],
                    link_paths: vec![],
                    include_paths: vec![],
                    defines: HashMap::new(),
                })
            }
        })
        .map_err(|e| TracersError::other_error(e.context("cached pkg-config results")))
    }

    fn into_native_libs(self) -> Vec<NativeLib> {
        //Get all of the information available out where to find the libs and which ones to link to
        let mut libs = Vec::new();

        libs.append(
            &mut self
                .link_paths
                .into_iter()
                .map(NativeLib::SupportLibPath)
                .collect::<Vec<NativeLib>>(),
        );

        libs.append(
            &mut self
                .libs
                .into_iter()
                .map(NativeLib::DynamicSupportLib)
                .collect::<Vec<NativeLib>>(),
        );

        libs
    }
}

#[derive(Template)]
#[template(path = "lttng/provider_wrapper.cpp", escape = "none")]
struct NativeProviderWrapperTemplate<'a> {
    spec: &'a ProviderSpecification,
}

impl<'a> NativeProviderWrapperTemplate<'a> {
    fn from_provider_spec<'b: 'a>(
        provider: &'b ProviderSpecification,
    ) -> NativeProviderWrapperTemplate<'a> {
        NativeProviderWrapperTemplate { spec: provider }
    }

    fn get_probe_args<'args>(
        &self,
        probe: &'args ProbeSpecification,
    ) -> Vec<&'args ProbeArgSpecification> {
        get_probe_args(probe)
    }
}

/// LTTng has a tool, `lttng-gen-tp`, which can generate the header and C source file for a
/// provider given only it's definitino in a template.  This is easier and more stable than
/// manually generating both.
#[derive(Template)]
#[template(path = "lttng/provider.tp", escape = "none")]
struct NativeProviderTemplate<'a> {
    spec: &'a ProviderSpecification,
}

impl<'a> NativeProviderTemplate<'a> {
    fn from_provider_spec<'b: 'a>(
        provider: &'b ProviderSpecification,
    ) -> NativeProviderTemplate<'a> {
        NativeProviderTemplate { spec: provider }
    }

    /// LTTng has a rather sophisticated approach to defining probes.  Like all such systems probes
    /// have arguments each with some kind of C data type (in LTTng terms these are called
    /// "arguments").  But LTTng also has the concept output fields or just "fields", which is the
    /// output produced by the probe if it is enabled and fires.  Each output is the result of
    /// evaluating some kind of expression, and it's not necessarily a one-to-one match with the
    /// arguments.  For example if you want to provide an array output, there would be two
    /// arguments: the pointer to the array and its length.  This could be one output field, using
    /// the `ctf_array` macro.
    ///
    /// Our implementation is not so sophisticated, so every argument corresponds to exactly one
    /// output field.  Perhaps in the future this will be extended to provide richer support for
    /// things like arrays, enums, etc.
    fn get_probe_arg_ctf_macro(arg: &ProbeArgSpecification) -> Option<String> {
        match arg.arg_type_info().get_c_type_enum() {
            CType::NoArg => None,
            CType::VoidPtr | CType::UCharPtr => {
                //LTTng doesn't have an option to output a pointer.  It wants something more
                //detailed like a string or an array.  So to output a pointer we'll use the
                //`ctf_integer_hex` option
                Some(format!(
                    "ctf_integer_hex(uintptr_t, {0}, (uintptr_t){0})",
                    arg.name()
                ))
            }
            CType::CharPtr => {
                //This is a null-terminated string
                Some(format!("ctf_string({0}, {0})", arg.name()))
            }
            int_type => {
                //Anything else is some kind of integer type.
                let type_name: &'static str = int_type.into();
                Some(format!("ctf_integer({0}, {1}, {1})", type_name, arg.name()))
            }
        }
    }

    /// Gets all of the output fields for this probe in one string.
    ///
    /// As above operates only on the first 10
    fn get_probe_output_fields(&self, probe: &ProbeSpecification) -> String {
        let fields: Vec<_> = self
            .get_probe_args(probe)
            .iter()
            .map(|arg| Self::get_probe_arg_ctf_macro(arg))
            .flatten()
            .collect();

        fields.join("\n")
    }

    fn get_probe_args<'args>(
        &self,
        probe: &'args ProbeSpecification,
    ) -> Vec<&'args ProbeArgSpecification> {
        get_probe_args(probe)
    }
}

/// Gets the probe's arguments (up to a maximum of 10).  LTTng supports no more than 10
/// arguments and will produce compile errors if any more are used
fn get_probe_args<'args>(probe: &'args ProbeSpecification) -> Vec<&'args ProbeArgSpecification> {
    probe.args.iter().take(10).collect()
}

pub(crate) struct LttngNativeCodeGenerator {
    out_dir: PathBuf,
    provider: ProviderSpecification,
}

impl LttngNativeCodeGenerator {
    pub fn new(out_dir: &Path, provider: ProviderSpecification) -> LttngNativeCodeGenerator {
        LttngNativeCodeGenerator {
            out_dir: out_dir.to_owned(),
            provider,
        }
    }
}

impl NativeCodeGenerator for LttngNativeCodeGenerator {
    fn generate_native_lib(&self) -> TracersResult<Vec<NativeLib>> {
        //LTTng is a bit more complex than STap because we generate the header and an
        //implementation C file with a tool, then generated a C++ wrapper around it which we'll
        //expose to the Rust code.
        let cache_dir = cache::get_cache_path(&self.out_dir);
        let code_dir = self.build_dir();
        fs::create_dir_all(&code_dir).map_err(|e| {
            TracersError::native_code_generation_error("Creating build directory", e)
        })?;

        //Create the <provider>.tp template file
        let provider_template = NativeProviderTemplate::from_provider_spec(&self.provider)
            .render()
            .map_err(|e| {
                TracersError::native_code_generation_error("Rendering LTTNG template", e)
            })?;
        let provider_template_path =
            code_dir.join(format!("{}.tp", self.provider.name_with_hash()));
        let mut file = File::create(&provider_template_path).map_err(|e| {
            TracersError::native_code_generation_error(
                format!(
                    "Creating template file {}",
                    provider_template_path.display()
                ),
                e,
            )
        })?;

        #[cfg(debug_assertions)]
        println!("Generated LTTng template code:\n{}", &provider_template);

        file.write_all(provider_template.as_bytes()).map_err(|e| {
            TracersError::native_code_generation_error("Writing to LTTng template file", e)
        })?;

        drop(file);

        //Create the <provider>.cpp wrapper file
        let wrapper_code = NativeProviderWrapperTemplate::from_provider_spec(&self.provider)
            .render()
            .map_err(|e| {
                TracersError::native_code_generation_error("Rendering native wrapper template", e)
            })?;

        let code_path = code_dir.join(format!("{}.cpp", self.provider.name_with_hash()));
        let mut file = File::create(&code_path).map_err(|e| {
            TracersError::native_code_generation_error(
                format!("Creating wrapper file {}", code_path.display()),
                e,
            )
        })?;

        #[cfg(debug_assertions)]
        println!("Generated wrapper code:\n{}", wrapper_code);

        file.write_all(wrapper_code.as_bytes()).map_err(|e| {
            TracersError::native_code_generation_error("Writing to wrapper file", e)
        })?;

        drop(file);

        // Invoke the `lttng-gen-tp` command line to generate C code for the provider
        // implementation functions
        let provider_impl = code_dir.join(format!("{}_provider.c", self.provider.name_with_hash()));
        let provider_header =
            code_dir.join(format!("{}_provider.h", self.provider.name_with_hash()));
        println!(
            "Generating {} and {} with `lttng-gen-tp`...",
            provider_impl.display(),
            provider_header.display()
        );
        let output = match Command::new("lttng-gen-tp")
            .current_dir(&code_dir)
            .arg(provider_template_path)
            .arg("-o").arg(&provider_impl.file_name().expect("Expected impl file name"))
            .arg("-o").arg(&provider_header.file_name().expect("Expected header file name"))
            .output()
        {
            Err(ref e) if e.kind() == ErrorKind::NotFound => Err(
                TracersError::native_code_generation_error("Generating native tracing code",
                                                                                                        format_err!(
                "The `lttng-gen-tp` executable wasn't found; make sure it's installed and in the path"
            ))),
            Err(e) => Err(TracersError::native_code_generation_error(
                "Error generating provider code with `lttng-gen-tp`",
                e,
            )),
            Ok(output) => Ok(output),
        }?;

        //Echo the output to our stdout/stderr, with errors output as a warning
        for line in String::from_utf8(output.stdout)
            .expect("Expected valid UTF-8 output")
            .lines()
        {
            println!("{}", line);
        }
        for line in String::from_utf8(output.stderr)
            .expect("Expected valid UTF-8 output")
            .lines()
        {
            eprintln!("cargo:warning={}", line);
        }

        if !output.status.success() {
            return Err(TracersError::native_code_generation_error::<
                &str,
                failure::Error,
            >(
                "Error generating provider code with `lttng-gen-tp`",
                format_err!("lttng-gen-tp failed with exit code {}", output.status),
            ));
        }

        let lib_dir = self.output_dir();
        fs::create_dir_all(&lib_dir).map_err(|e| {
            TracersError::native_code_generation_error("Creating output directory", e)
        })?;
        let lib_name = self.provider.name_with_hash();
        let lib_path = lib_dir.join(&lib_name);

        let mut cc = cc::Build::new();
        cc.cpp(true)
            .cpp_link_stdlib(None) //The wrapper code doesn't use any of the C++ std lib
            .cargo_metadata(false) //Don't instruct cargo to link this lib
            .static_flag(true)
            .out_dir(&lib_dir)
            .file(&code_path)
            .file(&provider_impl)
            .include(code_dir);

        let lttng_ust_info = LttngUstLibInfo::get(&cache_dir)?;
        for include in lttng_ust_info.include_paths.iter() {
            cc.include(include);
        }

        for (key, value) in lttng_ust_info.defines.iter() {
            let opt_value = value.as_ref().map(std::convert::AsRef::as_ref);
            cc.define(&key, opt_value);
        }

        cc.try_compile(&lib_name).map_err(|e| {
            //Unfortunately, the type `cc::Error` does not implement `std::error::Error` for
            //some reason, so we have to special-case it here
            let error = format_err!("{:?}", e).compat();
            TracersError::native_code_generation_error("Compiling native wrapper library", error)
        })?;
        println!("Compiled native wrapper library {}", lib_path.display());

        let mut libs = lttng_ust_info.into_native_libs();
        libs.push(NativeLib::StaticWrapperLib(lib_name));
        libs.push(NativeLib::StaticWrapperLibPath(lib_dir));

        Ok(libs)
    }

    fn out_dir(&self) -> &Path {
        &self.out_dir
    }
}
