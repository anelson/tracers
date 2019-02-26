//! Implements the `ProviderBuilder` and `Provider` traits for SystemTap
use failure::{Fail, Fallible};
use libstapsdt_sys::*;
use probers_core::argtypes::CType;
use probers_core::ProbeDefinition;
use probers_core::{Provider, ProviderBuilder};
use std::collections::HashMap;
use std::ffi::CString;
use std::ptr;

use super::{StapProbe, StapTracer};

#[derive(Debug, Fail)]
pub enum StapError {
    #[fail(display = "probe name is required")]
    ProbeNameRequired,

    #[fail(display = "duplicate probe name '{}'", name)]
    DuplicateProbeName { name: String },

    #[fail(display = "libstapsdt call failed: {}", func)]
    NativeCallFailed { func: &'static str },

    #[fail(display = "probe definition not found: {:?}", probe)]
    ProbeDefinitionNotFound { probe: ProbeDefinition },
}

pub struct StapProviderBuilder {
    probes: Vec<ProbeDefinition>,
}

impl StapProviderBuilder {
    pub(crate) fn new() -> StapProviderBuilder {
        StapProviderBuilder { probes: Vec::new() }
    }
}

impl ProviderBuilder<StapTracer> for StapProviderBuilder {
    fn add_probe(&mut self, definition: &ProbeDefinition) -> Fallible<()> {
        if definition.name.is_empty() {
            return Err(StapError::ProbeNameRequired.into());
        }

        // Make sure a probe by this name hasn't already been added
        if self.probes.iter().any(|p| p.name == definition.name) {
            return Err(StapError::DuplicateProbeName {
                name: definition.name.to_string(),
            }
            .into());
        }

        self.probes.push(definition.clone());

        Ok(())
    }

    fn build(self, name: &str) -> Fallible<StapProvider> {
        let mut provider = StapProvider::new(name)?;

        for probe in self.probes {
            provider.add_probe(probe)?;
        }

        Ok(provider)
    }
}

pub struct StapProvider {
    provider: *mut SDTProvider_t,
    probes: HashMap<ProbeDefinition, StapProbe>,
}

impl StapProvider {
    /// Initializes a new stap provider including calling `providerInit`.  This is internal to
    /// this module; the caller should also call `add_probe` for each probe defined on the
    /// provider.
    fn new(name: &str) -> Fallible<StapProvider> {
        let c_name = CString::new(name)?;

        let provider_ptr = unsafe { providerInit(c_name.as_ptr()) };

        if provider_ptr.is_null() {
            return Err(StapError::NativeCallFailed {
                func: "providerInit",
            }
            .into());
        }

        Ok(StapProvider {
            provider: provider_ptr,
            probes: HashMap::new(),
        })
    }

    /// Actually creates a systemtap probe object and associates it with the provider.  This also
    /// keeps a record of the `ProbeDefinition` which will be used at runtime to ensure the
    /// probe'sarg count and types are the same as when the probe was defined.
    fn add_probe(&mut self, definition: ProbeDefinition) -> Fallible<()> {
        let c_name = CString::new(definition.name.clone())?;

        // Unfortunately, the `providerAddProbe` C function is variadic, taking the types of the probe arguments
        // as variadic arguments.  This isn't a very ergonomic API design for either C or Rust wrappers.
        // In this case there is a max number of supported arguments, exposed in the `MAX_ARGUMENTS` constant.  At
        // the time of this writing this is '6', so we must handle from 0 to 6 possible arg counts.
        assert_eq!(6, MAX_ARGUMENTS);

        let arg_types: Vec<ArgType_t> = definition
            .arg_types
            .iter()
            .map(|x| Self::get_arg_type(x.clone()))
            .collect();

        let probe_ptr = unsafe {
            match arg_types.len() {
                0 => providerAddProbe(self.provider, c_name.as_ptr(), 0),
                1 => providerAddProbe(self.provider, c_name.as_ptr(), 1, arg_types[0]),
                2 => providerAddProbe(
                    self.provider,
                    c_name.as_ptr(),
                    2,
                    arg_types[0],
                    arg_types[1],
                ),
                3 => providerAddProbe(
                    self.provider,
                    c_name.as_ptr(),
                    3,
                    arg_types[0],
                    arg_types[1],
                    arg_types[2],
                ),
                4 => providerAddProbe(
                    self.provider,
                    c_name.as_ptr(),
                    4,
                    arg_types[0],
                    arg_types[1],
                    arg_types[2],
                    arg_types[3],
                ),
                5 => providerAddProbe(
                    self.provider,
                    c_name.as_ptr(),
                    5,
                    arg_types[0],
                    arg_types[1],
                    arg_types[2],
                    arg_types[3],
                    arg_types[4],
                ),
                _ => providerAddProbe(
                    self.provider,
                    c_name.as_ptr(),
                    6,
                    arg_types[0],
                    arg_types[1],
                    arg_types[2],
                    arg_types[3],
                    arg_types[4],
                    arg_types[5],
                ),
            }
        };

        if probe_ptr.is_null() {
            return Err(StapError::NativeCallFailed {
                func: "providerAddProbe",
            }
            .into());
        }

        let probe = StapProbe { probe: probe_ptr };

        self.probes.insert(definition, probe);
        Ok(())
    }

    /// Translates from the `probers-core` `CType` enum to the constants used by libstapsdt
    fn get_arg_type(typ: CType) -> ArgType_t {
        match typ {
            CType::NoArg => ArgType_t_noarg,
            CType::Char => ArgType_t_int8,
            CType::UChar => ArgType_t_uint8,
            CType::Short => ArgType_t_int16,
            CType::UShort => ArgType_t_uint16,
            CType::Int => ArgType_t_int32,
            CType::UInt => ArgType_t_uint32,
            CType::Long => ArgType_t_int64,
            CType::ULong => ArgType_t_uint64,
            CType::LongLong => ArgType_t_int64,
            CType::ULongLong => ArgType_t_uint64,
            CType::VoidPtr | CType::CharPtr => ArgType_t_uint64, //we can hard-code this because we only support 64-bit linux
        }
    }
}

impl Provider<StapTracer> for StapProvider {
    /// Look up the probe by its definition (that is, name and arg types)
    fn get_probe_unsafe(&self, definition: &ProbeDefinition) -> Fallible<StapProbe> {
        let probe: Option<StapProbe> = self.probes.get(definition).map(|x| x.clone());

        probe.ok_or_else(|| {
            StapError::ProbeDefinitionNotFound {
                probe: definition.clone(),
            }
            .into()
        })
    }
}

/// Implementation of `Drop` which destroys the stap provider object and frees and memory
/// associated with it.  Note that `providerDestroy` also frees any probes that have been
/// allocatedon a provider.
impl Drop for StapProvider {
    fn drop(&mut self) {
        if !self.provider.is_null() {
            unsafe { providerDestroy(self.provider) };
            self.provider = ptr::null_mut();
        }
    }
}