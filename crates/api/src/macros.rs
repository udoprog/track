macro_rules! __define_id {
    ($name:ident) => {
        #[derive(
            Copy,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            Encode,
            Decode,
            serde::Serialize,
            serde::Deserialize,
        )]
        #[serde(transparent)]
        #[musli(crate = musli_core, transparent)]
        pub struct $name(u64);

        impl $name {
            #[inline]
            pub const fn new(id: u64) -> Self {
                Self(id)
            }

            #[inline]
            pub fn get(self) -> u64 {
                self.0
            }

            #[cfg(feature = "rand")]
            #[inline]
            pub fn random() -> Self {
                Self(rand::random())
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                let bytes = self.0.to_be_bytes();

                let d = ::base64::display::Base64Display::new(
                    &bytes,
                    &::base64::engine::general_purpose::URL_SAFE_NO_PAD,
                );

                d.fmt(f)
            }
        }

        impl ::core::fmt::Debug for $name {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::fmt::Display::fmt(self, f)
            }
        }

        impl ::core::str::FromStr for $name {
            type Err = ::base64::DecodeSliceError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                use ::base64::Engine as _;
                let mut bytes = [0u8; 8];

                ::base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode_slice(s.as_bytes(), &mut bytes)?;

                Ok($name(u64::from_be_bytes(bytes)))
            }
        }

        #[cfg(feature = "sqll")]
        impl ::sqll::FromColumn<'_> for $name {
            type Type = ::sqll::ty::Integer;

            #[inline]
            fn from_column(
                stmt: &::sqll::Statement,
                index: ::sqll::ty::Integer,
            ) -> ::sqll::Result<Self> {
                let value = i64::from_column(stmt, index)?;
                Ok($name(value.cast_unsigned()))
            }
        }

        #[cfg(feature = "sqll")]
        impl ::sqll::BindValue for $name {
            #[inline]
            fn bind_value(
                &self,
                stmt: &mut ::sqll::Statement,
                index: ::sqll::Index,
            ) -> ::sqll::Result<()> {
                self.0.cast_signed().bind_value(stmt, index)
            }
        }
    };
}

pub(crate) use __define_id as define_id;
