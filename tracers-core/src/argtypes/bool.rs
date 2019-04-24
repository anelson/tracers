use super::{ProbeArgType, ProbeArgWrapper};

impl ProbeArgType<bool> for bool {
    type WrapperType = bool;
    fn wrap(arg: bool) -> Self::WrapperType {
        arg
    }
}

impl ProbeArgWrapper for bool {
    type CType = i32;

    fn as_c_type(&self) -> Self::CType {
        i32::from(*self)
    }
}

#[cfg(test)]
mod tests {
    use crate::{wrap, ProbeArgWrapper};

    #[test]
    fn as_c_type() {
        assert_eq!(0i32, wrap(false).as_c_type());
        assert_eq!(1i32, wrap(true).as_c_type());
    }
}
