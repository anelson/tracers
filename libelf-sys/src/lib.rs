#![allow(clippy::all)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!("libelf.rs");

#[cfg(test)]
mod tests {
    use super::{elf_version, EV_CURRENT, EV_NONE};
    #[test]
    fn elf_version_is_correct() {
        //Make a token call to a libelf function just to verify all dependent libraries load
        let version = unsafe { elf_version(EV_CURRENT) };
        println!("libelf version: {}", version);

        assert_ne!(version, EV_NONE);
    }
}
