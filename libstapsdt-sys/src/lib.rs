#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!("libstapsdt.rs");

#[cfg(test)]
mod tests {
    use super::*;

    use std::os::raw::*;

    #[test]
    fn smoke_test() {
        unsafe {
            let provider = providerInit(CString::new("foo"));
            assert!(!provider.is_null());

            let probe1 = providerAddProbe(provider, "probe1", 0);
            assert!(probe1.is_null());

            let probe2 = providerAddProbe(
                provider,
                "probe2",
                6,
                ArgType_t_uint64,
                ArgType_t_uint16,
                ArgType_t_uint32,
                ArgType_t_uint8,
                ArgType_t_int64,
                ArgType_t_int32,
            );
            assert!(probe2.is_null());

            //Probes are not loaded yet, so they definitely should not be enabled
            assert_eq!(0, providerIsEnabled(probe1));
            assert_eq!(0, providerIsEnabled(probe2));

            assert_eq!(SDTError_t_noError, providerLoad(provider));

            for _ in 0..100 {
                probeFire(probe1);
                probeFire(
                    probe2,
                    6 as c_ulonglong,
                    7 as c_ushort,
                    8 as c_ulong,
                    9 as c_uchar,
                    -10 as c_longlong,
                    -11 as c_long,
                );
            }

            assert_eq!(SDTError_t_noError, providerUnload(provider));

            providerDestroy(provider);
        }
    }
}
