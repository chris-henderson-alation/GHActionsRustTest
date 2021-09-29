use k8s_openapi::api::core::v1::Pod;
pub use kind_derive::*;

/// A type that implements Kind is capable of describing itself to outside systems, typically
/// by simply returning the name of their type.
///
/// This is most easily accomplished by using the Kind derive macro
///
/// ```
/// use kind::Kind;
///
/// #[derive(Kind)]
/// struct MyKind {}
///
/// #[derive(Kind)]
/// enum MyEnum {
///     VariantOne,
///     VariantTwo(u32)
/// }
///
/// assert_eq!("MyKind", MyKind{}.kind());
/// assert_eq!("MyEnum::VariantOne", MyEnum::VariantOne.kind());
/// assert_eq!("MyEnum::VariantTwo", MyEnum::VariantTwo(42).kind());
/// ```
///
/// The Kind derivation macro does not work on Unions. If you wish, you must implement Kind
/// on your target Union yourself.
///
/// A blanket implementation exists for all [Vec<T>](std::vec::Vec) where T implements Kind for
/// which the result is `List[T::kind()]`. If the vector is empty, then the kind is `List[]`.
pub trait Kind {
    fn kind(&self) -> String;
}

macro_rules! impl_kind {
    ($i:ident) => {
        impl Kind for $i {
            fn kind(&self) -> String {
                stringify!($i).to_string()
            }
        }
    };
    (()) => {
        impl Kind for () {
            fn kind(&self) -> String {
                stringify!(()).to_string()
            }
        }
    };
}

impl_kind!(());
impl_kind!(String);
impl_kind!(Pod);
impl_kind!(u8);
impl_kind!(u16);
impl_kind!(u32);
impl_kind!(u64);
impl_kind!(u128);
impl_kind!(i8);
impl_kind!(i16);
impl_kind!(i32);
impl_kind!(i64);
impl_kind!(i128);
impl_kind!(f32);
impl_kind!(f64);

impl<T> Kind for Vec<T>
where
    T: Kind,
{
    fn kind(&self) -> String {
        if self.is_empty() {
            "List[]".to_string()
        } else {
            format!("List[{}]", self.get(0).unwrap().kind())
        }
    }
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use super::*;

    #[test]
    fn simple_struct() {
        #[derive(Kind)]
        struct Lol {}
        assert_eq!(Lol {}.kind(), "Lol")
    }

    #[test]
    fn unit() {
        #[derive(Kind)]
        enum AnEnum {
            Variant,
        }
        assert_eq!(AnEnum::Variant.kind(), "AnEnum::Variant")
    }

    #[test]
    fn unary_unnamed() {
        #[derive(Kind)]
        enum AnEnum {
            Variant(i32),
        }
        assert_eq!(AnEnum::Variant(1).kind(), "AnEnum::Variant")
    }

    #[test]
    fn binary_unnamed() {
        #[derive(Kind)]
        enum AnEnum {
            Variant(i32, i32),
        }
        assert_eq!(AnEnum::Variant(1, 2).kind(), "AnEnum::Variant")
    }

    #[test]
    fn unary_named() {
        #[derive(Kind)]
        enum AnEnum {
            Variant { a: i32 },
        }
        assert_eq!(AnEnum::Variant { a: 1 }.kind(), "AnEnum::Variant")
    }

    #[test]
    fn binary_named() {
        #[derive(Kind)]
        enum AnEnum {
            Variant { a: i32, b: i32 },
        }
        assert_eq!(AnEnum::Variant { a: 1, b: 2 }.kind(), "AnEnum::Variant")
    }

    #[test]
    fn mixed_enum() {
        #[derive(Kind)]
        enum AnEnum {
            Unit,
            UnaryUnnamed(i32),
            BinaryUnnamed(i32, i32),
            UnaryNamed { a: i32 },
            BinaryNamed { a: i32, b: i32 },
        }
        assert_eq!(AnEnum::Unit.kind(), "AnEnum::Unit");
        assert_eq!(AnEnum::UnaryUnnamed(1).kind(), "AnEnum::UnaryUnnamed");
        assert_eq!(AnEnum::BinaryUnnamed(1, 2).kind(), "AnEnum::BinaryUnnamed");
        assert_eq!(AnEnum::UnaryNamed { a: 1 }.kind(), "AnEnum::UnaryNamed");
        assert_eq!(
            AnEnum::BinaryNamed { a: 1, b: 2 }.kind(),
            "AnEnum::BinaryNamed"
        );
    }
}
