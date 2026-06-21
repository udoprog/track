use core::fmt;
use core::str::FromStr;

use musli_core::{Allocator, Context, Decode, Decoder, Encode, Encoder};

#[cfg(feature = "yew")]
use implicit_clone::unsync::IString;
#[cfg(feature = "yew")]
use yew::html::IntoPropValue;
#[cfg(feature = "yew")]
use yew::virtual_dom::{Key, VNode};

/// A Country identified by its 2-letter ISO 3166-1 alpha-2 code, stored as four
/// bytes: the two ASCII letters followed by `0` padding.
///
/// The all-zero value is [`Country::DEFAULT`], a reference-time sentinel
/// meaning "use the media's own default (original) Country". It is never a real
/// Country and must never be persisted as data in the `strings` table.
///
/// The in-memory representation is the code's bytes directly; the only place
/// bytes are turned into an integer is the SQLite conversion below, which pins
/// the byte order so the stored value is identical regardless of host endianness.
#[derive(Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Country([u8; 4]);

/// Error produced when a string cannot be parsed as a [`Country`].
#[derive(Debug)]
pub struct ParseCountryErr;

impl fmt::Display for ParseCountryErr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Country code")
    }
}

impl core::error::Error for ParseCountryErr {}

impl Country {
    /// Sentinel meaning "use the media's own default (original) Country".
    pub const DEFAULT: Country = Country([0; 4]);

    /// United States (`us`).
    pub const US: Country = Country(*b"us\0\0");

    /// Great Britain (`gb`).
    pub const GB: Country = Country(*b"gb\0\0");

    /// Japan (`jp`).
    pub const JP: Country = Country(*b"jp\0\0");

    /// Unwrap the current Country or fall back to other if the current Country
    /// is `DEFAULT`.
    #[inline]
    pub fn or(self, other: Self) -> Self {
        if self.is_default() { other } else { self }
    }

    /// The ascii string corresponding to this Country code.
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

        // SAFETY: The Country code is valid through construction.
        unsafe { str::from_utf8_unchecked(&self.0[..end]) }
    }

    /// Build from a 2-letter 3166-1 alpha-2 code (case-insensitive).
    pub fn from_iso_3166_1(code: &str) -> Option<Self> {
        fn to_lower<'a>(buf: &'a mut [u8; 4], input: &str) -> Option<&'a str> {
            let bytes = input.as_bytes();

            if bytes.len() > buf.len() {
                return None;
            }

            for (b, o) in bytes.iter().zip(buf.iter_mut()) {
                if !b.is_ascii_alphabetic() {
                    return None;
                }

                *o = b.to_ascii_lowercase();
            }

            Some(unsafe { str::from_utf8_unchecked(&buf[..bytes.len()]) })
        }

        let code = code.trim();

        if code.is_empty() || code.eq_ignore_ascii_case("default") {
            return Some(Self::DEFAULT);
        }

        let mut bytes = [0u8; 4];
        let lower = to_lower(&mut bytes, code)?;

        let bytes = match lower.len() {
            2 => {
                let id = iso3166::by_part1(lower)?.alpha2;

                let &[a, b] = id.as_bytes() else {
                    return None;
                };

                [a, b, 0, 0]
            }
            _ => return None,
        };

        Some(Self(bytes))
    }

    /// Whether this is the [`Country::DEFAULT`] sentinel.
    #[inline]
    pub const fn is_default(self) -> bool {
        matches!(self.0, [0, 0, 0, 0])
    }

    /// The 2-letter ISO 3166-1 alpha-2 code, or `None` for
    /// [`Country::DEFAULT`].
    #[inline]
    pub fn to_iso3166_1(&self) -> Option<&str> {
        if self.is_default() {
            return None;
        }

        Some(self.as_raw_code())
    }
}

impl fmt::Display for Country {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Country {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Country({self})")
    }
}

impl FromStr for Country {
    type Err = ParseCountryErr;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_iso_3166_1(s).ok_or(ParseCountryErr)
    }
}

impl serde::Serialize for Country {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.as_str().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Country {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = Country;

            #[inline]
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a 2-letter ISO 3166-1 Country code or 'default'")
            }

            #[inline]
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                v.parse().map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl<M> Encode<M> for Country {
    type Encode = Self;

    #[inline]
    fn encode<E>(&self, encoder: E) -> Result<(), E::Error>
    where
        E: Encoder<Mode = M>,
    {
        encoder.collect_string(self)
    }

    #[inline]
    fn as_encode(&self) -> &Self::Encode {
        self
    }
}

impl<'de, M, A> Decode<'de, M, A> for Country
where
    A: Allocator,
{
    #[inline]
    fn decode<D>(decoder: D) -> Result<Self, D::Error>
    where
        D: Decoder<'de, Mode = M, Allocator = A>,
    {
        let cx = decoder.cx();
        decoder.decode_unsized(|s: &str| s.parse::<Country>().map_err(cx.map()))
    }

    const IS_BITWISE_DECODE: bool = false;
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for Country {
    type Type = ::sqll::ty::Integer;

    #[inline]
    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        let value = i64::from_column(stmt, index)?;
        Ok(Country((value as u32).to_be_bytes()))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for Country {
    #[inline]
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        i64::from(u32::from_be_bytes(self.0)).bind_value(stmt, index)
    }
}

#[cfg(feature = "yew")]
impl IntoPropValue<VNode> for Country {
    #[inline]
    fn into_prop_value(self) -> VNode {
        self.to_string().into()
    }
}

#[cfg(feature = "yew")]
impl IntoPropValue<Option<IString>> for Country {
    #[inline]
    fn into_prop_value(self) -> Option<IString> {
        self.to_iso3166_1().map(Into::into)
    }
}

impl PartialEq<&'static iso3166::Country> for Country {
    #[inline]
    fn eq(&self, other: &&'static iso3166::Country) -> bool {
        self.to_iso3166_1() == Some(other.alpha2)
    }
}

#[cfg(feature = "yew")]
impl From<Country> for Key {
    #[inline]
    fn from(country: Country) -> Self {
        country.as_str().into()
    }
}
