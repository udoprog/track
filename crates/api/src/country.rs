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

    /// United States (`US`).
    pub const US: Country = Country::new(b"US");

    /// Great Britain (`GB`).
    pub const GB: Country = Country::new(b"GB");

    /// Japan (`JP`).
    pub const JP: Country = Country::new(b"JP");

    /// Build a country from its raw representation.
    ///
    /// If the input is not a valid ISO 3166-1 alpha-2 code, returns
    /// [`Country::DEFAULT`].
    ///
    /// ```
    /// use api::Country;
    ///
    /// assert_eq!(Country::new(b"US"), Country::US);
    /// assert_eq!(Country::new(b"us"), Country::US);
    /// assert_eq!(Country::new(b"ZZ"), Country::DEFAULT);
    /// ```
    #[inline]
    pub const fn new(bytes: &[u8]) -> Self {
        if bytes.len() > 4 {
            return Self::DEFAULT;
        }

        let mut out = [0u8; 4];
        let mut n = 0;

        while n < bytes.len() {
            out[n] = bytes[n].to_ascii_uppercase();
            n += 1;
        }

        let mut test = out.as_slice();

        while !test.is_empty() {
            test = match test {
                [prefix @ .., 0] => prefix,
                _ => break,
            };
        }

        if !iso3166::is_valid_alpha2(test) {
            return Self::DEFAULT;
        }

        Self(out)
    }

    /// Get the raw byte-wise representation.
    #[inline]
    pub const fn to_raw(&self) -> [u8; 4] {
        self.0
    }

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
    pub fn from_iso(code: &str) -> Option<Self> {
        let code = code.trim();

        if code.is_empty() || code.eq_ignore_ascii_case("default") {
            return Some(Self::DEFAULT);
        }

        let mut bytes = [0u8; 4];
        let upper = to_upper(&mut bytes, code)?;

        let bytes = match upper.len() {
            2 if iso3166::is_valid_alpha2(upper.as_bytes()) => {
                let &[a, b] = upper.as_bytes() else {
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

    /// Get a reference to a static ISO 3166 country, or `None` for
    /// [`Country::DEFAULT`].
    #[inline]
    pub fn to_iso(&self) -> Option<&'static iso3166::Country> {
        if self.is_default() {
            return None;
        }

        iso3166::by_alpha2(self.as_raw_code())
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
        Self::from_iso(s).ok_or(ParseCountryErr)
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

        if let Ok(value) = u32::try_from(value) {
            let bytes = value.to_be_bytes();
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());

            // Only the shape is checked so that a code missing from the current
            // tables still reads; `as_raw_code` relies on it being ASCII.
            if bytes[..end].iter().all(u8::is_ascii_uppercase)
                && bytes[end..].iter().all(|&b| b == 0)
            {
                return Ok(Country(bytes));
            }
        }

        Err(::sqll::Error::new(
            ::sqll::Code::MISMATCH,
            "invalid stored country code",
        ))
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
        self.to_iso().map(|c| IString::Static(c.name))
    }
}

impl PartialEq<&'static iso3166::Country> for Country {
    #[inline]
    fn eq(&self, other: &&'static iso3166::Country) -> bool {
        self.as_raw_code() == other.alpha2
    }
}

#[cfg(feature = "yew")]
impl From<Country> for Key {
    #[inline]
    fn from(country: Country) -> Self {
        country.as_str().into()
    }
}

fn to_upper<'a>(buf: &'a mut [u8; 4], input: &str) -> Option<&'a str> {
    let bytes = input.as_bytes();

    if bytes.len() > buf.len() {
        return None;
    }

    for (b, o) in bytes.iter().zip(buf.iter_mut()) {
        if !b.is_ascii_alphabetic() {
            return None;
        }

        *o = b.to_ascii_uppercase();
    }

    Some(unsafe { str::from_utf8_unchecked(&buf[..bytes.len()]) })
}
