//! Contains the native C++ code generator and the Rust bindings generator to support Linux
//! SystemTap user-mode tracing
use crate::gen::r#static::native_code::NativeCodeGenerator;
use crate::gen::NativeLib;
use crate::spec::ProviderSpecification;
use crate::TracersError;
use crate::TracersResult;
use askama::Template;
use failure::format_err;
use std::fs::{self, File};
use std::io::prelude::*;
use std::path::{Path, PathBuf};

#[derive(Template)]
#[template(path = "stap/provider_wrapper.cpp", escape = "none")]
struct NativeProviderWrapperTemplate<'a> {
    spec: &'a ProviderSpecification,
}

impl<'a> NativeProviderWrapperTemplate<'a> {
    fn from_provider_spec<'b: 'a>(
        provider: &'b ProviderSpecification,
    ) -> NativeProviderWrapperTemplate<'a> {
        NativeProviderWrapperTemplate { spec: provider }
    }
}

pub(crate) struct StapNativeCodeGenerator {
    out_dir: PathBuf,
    provider: ProviderSpecification,
}

impl StapNativeCodeGenerator {
    pub fn new(out_dir: &Path, provider: ProviderSpecification) -> StapNativeCodeGenerator {
        StapNativeCodeGenerator {
            out_dir: out_dir.to_owned(),
            provider,
        }
    }
}

impl NativeCodeGenerator for StapNativeCodeGenerator {
    fn generate_native_lib(&self) -> TracersResult<Vec<NativeLib>> {
        let wrapper_code = NativeProviderWrapperTemplate::from_provider_spec(&self.provider)
            .render()
            .map_err(|e| {
                TracersError::native_code_generation_error("Rendering native wrapper template", e)
            })?;

        let code_dir = self.build_dir();
        fs::create_dir_all(&code_dir).map_err(|e| {
            TracersError::native_code_generation_error("Creating build directory", e)
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

        let lib_dir = self.output_dir();
        fs::create_dir_all(&lib_dir).map_err(|e| {
            TracersError::native_code_generation_error("Creating output directory", e)
        })?;
        let lib_name = self.provider.name_with_hash();
        let lib_path = lib_dir.join(&lib_name);

        cc::Build::new()
            .cpp(true)
            .cpp_link_stdlib(None) //The wrapper code doesn't use any of the C++ std lib
            .static_flag(true)
            .out_dir(&lib_dir)
            .file(code_path)
            .try_compile(&lib_name)
            .map_err(|e| {
                //Unfortunately, the type `cc::Error` does not implement `std::error::Error` for
                //some reason, so we have to special-case it here
                let error = format_err!("{:?}", e).compat();
                TracersError::native_code_generation_error(
                    "Compiling native wrapper library",
                    error,
                )
            })?;

        println!("Compiled native wrapper library {}", lib_path.display());

        Ok(vec![
            NativeLib::StaticWrapperLib(lib_name),
            NativeLib::StaticWrapperLibPath(lib_dir),
        ])
    }

    fn out_dir(&self) -> &Path {
        &self.out_dir
    }
}
