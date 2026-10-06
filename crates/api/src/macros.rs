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

/// Define a code type stored as up to four ASCII letters padded with `0`,
/// where the all-zero value is the `DEFAULT` sentinel.
///
/// The defining module provides `from_iso`, which `FromStr`, serde and musli
/// decoding parse through, and a `Debug` implementation.
macro_rules! __define_code {
    (
        $(#[$meta:meta])*
        pub struct $name:ident;
        $(#[$err_meta:meta])*
        pub struct $err:ident($err_msg:literal);
        $(#[$new_meta:meta])*
        const fn new valid by $valid:path;
        case $to_case:ident, $is_case:ident;
        expecting $expecting:literal;
        stored error $stored_msg:literal;
    ) => {
        $(#[$meta])*
        ///
        /// The in-memory representation is the code's bytes directly; the only
        /// place bytes are turned into an integer is the SQLite conversion,
        /// which pins the byte order so the stored value is identical
        /// regardless of host endianness.
        #[derive(Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name([u8; 4]);

        $(#[$err_meta])*
        #[derive(Debug)]
        pub struct $err;

        impl ::core::fmt::Display for $err {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str($err_msg)
            }
        }

        impl ::core::error::Error for $err {}

        impl $name {
            /// Sentinel meaning "use the media's own default (original) value".
            pub const DEFAULT: $name = $name([0; 4]);

            $(#[$new_meta])*
            #[inline]
            pub const fn new(bytes: &[u8]) -> Self {
                if bytes.len() > 4 {
                    return Self::DEFAULT;
                }

                let mut out = [0u8; 4];
                let mut n = 0;

                while n < bytes.len() {
                    out[n] = bytes[n].$to_case();
                    n += 1;
                }

                let mut test = out.as_slice();

                while !test.is_empty() {
                    test = match test {
                        [prefix @ .., 0] => prefix,
                        _ => break,
                    };
                }

                if !$valid(test) {
                    return Self::DEFAULT;
                }

                Self(out)
            }

            /// Get the raw byte-wise representation.
            #[inline]
            pub const fn to_raw(&self) -> [u8; 4] {
                self.0
            }

            /// Unwrap the current code or fall back to other if the current
            /// code is `DEFAULT`.
            #[inline]
            pub fn or(self, other: Self) -> Self {
                if self.is_default() { other } else { self }
            }

            /// The ascii string corresponding to this code, `"default"` for
            /// `DEFAULT`.
            #[inline]
            pub fn as_str(&self) -> &str {
                if self.is_default() {
                    return "default";
                }

                self.as_raw_code()
            }

            #[inline]
            fn as_raw_code(&self) -> &str {
                let end = self.0.iter().position(|&b| b == 0).unwrap_or(self.0.len());

                // SAFETY: The code is ASCII through construction.
                unsafe { str::from_utf8_unchecked(&self.0[..end]) }
            }

            /// Whether this is the `DEFAULT` sentinel.
            #[inline]
            pub const fn is_default(self) -> bool {
                matches!(self.0, [0, 0, 0, 0])
            }

            /// Copy `input` into `buf` with normalized case, or `None` if it
            /// is too long or not alphabetic.
            fn normalize<'a>(buf: &'a mut [u8; 4], input: &str) -> Option<&'a str> {
                let bytes = input.as_bytes();

                if bytes.len() > buf.len() {
                    return None;
                }

                for (b, o) in bytes.iter().zip(buf.iter_mut()) {
                    if !b.is_ascii_alphabetic() {
                        return None;
                    }

                    *o = b.$to_case();
                }

                Some(unsafe { str::from_utf8_unchecked(&buf[..bytes.len()]) })
            }
        }

        impl ::core::fmt::Display for $name {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::core::str::FromStr for $name {
            type Err = $err;

            #[inline]
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::from_iso(s).ok_or($err)
            }
        }

        impl ::serde::Serialize for $name {
            #[inline]
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: ::serde::Serializer,
            {
                ::serde::Serialize::serialize(self.as_str(), serializer)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            #[inline]
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: ::serde::Deserializer<'de>,
            {
                struct Visitor;

                impl ::serde::de::Visitor<'_> for Visitor {
                    type Value = $name;

                    #[inline]
                    fn expecting(
                        &self,
                        formatter: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        formatter.write_str($expecting)
                    }

                    #[inline]
                    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                    where
                        E: ::serde::de::Error,
                    {
                        v.parse().map_err(::serde::de::Error::custom)
                    }
                }

                deserializer.deserialize_str(Visitor)
            }
        }

        impl<M> ::musli_core::Encode<M> for $name {
            type Encode = Self;

            #[inline]
            fn encode<E>(&self, encoder: E) -> Result<(), E::Error>
            where
                E: ::musli_core::Encoder<Mode = M>,
            {
                encoder.collect_string(self)
            }

            #[inline]
            fn as_encode(&self) -> &Self::Encode {
                self
            }
        }

        impl<'de, M, A> ::musli_core::Decode<'de, M, A> for $name
        where
            A: ::musli_core::Allocator,
        {
            #[inline]
            fn decode<D>(decoder: D) -> Result<Self, D::Error>
            where
                D: ::musli_core::Decoder<'de, Mode = M, Allocator = A>,
            {
                use ::musli_core::Context as _;
                let cx = decoder.cx();
                decoder.decode_unsized(|s: &str| s.parse::<$name>().map_err(cx.map()))
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

                if let Ok(value) = u32::try_from(value) {
                    let bytes = value.to_be_bytes();
                    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());

                    // Only the shape is checked so that a code missing from the
                    // current tables still reads; `as_raw_code` relies on it
                    // being ASCII.
                    if bytes[..end].iter().all(u8::$is_case)
                        && bytes[end..].iter().all(|&b| b == 0)
                    {
                        return Ok($name(bytes));
                    }
                }

                Err(::sqll::Error::new(::sqll::Code::MISMATCH, $stored_msg))
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
                i64::from(u32::from_be_bytes(self.0)).bind_value(stmt, index)
            }
        }

        #[cfg(feature = "yew")]
        impl ::yew::html::IntoPropValue<::yew::virtual_dom::VNode> for $name {
            #[inline]
            fn into_prop_value(self) -> ::yew::virtual_dom::VNode {
                self.to_string().into()
            }
        }
    };
}

pub(crate) use __define_code as define_code;
