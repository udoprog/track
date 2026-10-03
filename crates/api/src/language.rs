use core::fmt;
use core::str::FromStr;

use musli_core::{Allocator, Context, Decode, Decoder, Encode, Encoder};

#[cfg(feature = "yew")]
use yew::html::IntoPropValue;
#[cfg(feature = "yew")]
use yew::virtual_dom::VNode;

/// A language identified by its 3-letter ISO 639-3 code, stored as four bytes:
/// the three ASCII letters followed by a `0` pad.
///
/// The all-zero value is [`Language::DEFAULT`], a reference-time sentinel
/// meaning "use the media's own default (original) language". It is never a real
/// language and must never be persisted as data in the `strings` table.
///
/// The in-memory representation is the code's bytes directly; the only place
/// bytes are turned into an integer is the SQLite conversion below, which pins
/// the byte order so the stored value is identical regardless of host endianness.
#[derive(Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Language([u8; 4]);

/// Error produced when a string cannot be parsed as a [`Language`].
#[derive(Debug)]
pub struct ParseLanguageErr;

impl fmt::Display for ParseLanguageErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid language code")
    }
}

impl core::error::Error for ParseLanguageErr {}

impl Language {
    /// Sentinel meaning "use the media's own default (original) language".
    pub const DEFAULT: Language = Language([0; 4]);

    /// English (`eng`).
    pub const ENG: Language = Language::new(b"eng");

    /// Build a language from its raw representation.
    ///
    /// If the input is not a valid ISO 639-3 code, returns
    /// [`Language::DEFAULT`].
    ///
    /// ```
    /// use api::Language;
    ///
    /// assert_eq!(Language::new(b"eng"), Language::ENG);
    /// assert_eq!(Language::new(b"ENG"), Language::ENG);
    /// assert_eq!(Language::new(b"zzz"), Language::DEFAULT);
    /// ```
    #[inline]
    pub const fn new(bytes: &[u8]) -> Self {
        if bytes.len() > 4 {
            return Self::DEFAULT;
        }

        let mut out = [0u8; 4];
        let mut n = 0;

        while n < bytes.len() {
            out[n] = bytes[n].to_ascii_lowercase();
            n += 1;
        }

        let mut test = out.as_slice();

        while !test.is_empty() {
            test = match test {
                [prefix @ .., 0] => prefix,
                _ => break,
            };
        }

        if !iso639::is_valid_id(test) {
            return Self::DEFAULT;
        }

        Self(out)
    }

    /// Get the raw byte-wise representation.
    #[inline]
    pub const fn to_raw(&self) -> [u8; 4] {
        self.0
    }

    /// Unwrap the current language or fall back to other if the current
    /// language is `DEFAULT`.
    #[inline]
    pub fn or(self, other: Self) -> Self {
        if self.is_default() { other } else { self }
    }

    /// Filter the current language, returning `DEFAULT` if the predicate
    /// returns false.
    #[inline]
    pub fn filter(self, f: impl FnOnce(Self) -> bool) -> Self {
        if f(self) { self } else { Self::DEFAULT }
    }

    /// The ascii string corresponding to this language code.
    #[inline]
    fn as_repr(&self) -> &str {
        if self.is_default() {
            return "default";
        }

        self.as_raw_code()
    }

    fn as_raw_code(&self) -> &str {
        let end = self.0.iter().position(|&b| b == 0).unwrap_or(self.0.len());

        // SAFETY: The language code is valid through construction.
        unsafe { str::from_utf8_unchecked(&self.0[..end]) }
    }

    /// Build from a 2- or 3-letter ISO 639 code (case-insensitive). A 2-letter
    /// code is resolved to its 3-letter form via the `iso639` data. An empty
    /// string or `"default"` maps to [`Language::DEFAULT`]. Returns `None` for
    /// anything else.
    pub fn from_iso(code: &str) -> Option<Self> {
        let code = code.trim();

        if code.is_empty() || code.eq_ignore_ascii_case("default") {
            return Some(Self::DEFAULT);
        }

        let mut bytes = [0u8; 4];
        let lower = to_lower(&mut bytes, code)?;

        let bytes = match lower.len() {
            2 => {
                let id = iso639::by_part1(lower)?.id;

                let &[a, b, c] = id.as_bytes() else {
                    return None;
                };

                [a, b, c, 0]
            }
            3 if iso639::is_valid_id(lower.as_bytes()) => bytes,
            _ => return None,
        };

        Some(Self(bytes))
    }

    /// Whether this is the [`Language::DEFAULT`] sentinel.
    #[inline]
    pub const fn is_default(self) -> bool {
        matches!(self.0, [0, 0, 0, 0])
    }

    /// The static [`Language`] structure associated with this code, or `None`
    /// for [`Language::DEFAULT`].
    ///
    /// [`Language`]: iso639::Language
    pub fn to_iso(&self) -> Option<&'static iso639::Language> {
        if self.is_default() {
            return None;
        }

        iso639::by_id(self.as_raw_code())
    }

    /// The 3-letter ISO 639-3 code, if one exists or `None` for `DEFAULT`.
    pub fn to_id(&self) -> Option<&'static str> {
        self.to_iso().map(|e| e.id)
    }

    /// The 2-letter ISO 639-1 code, if one exists (`None` for `DEFAULT` or
    /// codes without a 2-letter form). Used for remotes that key on ISO 639-1.
    pub fn to_part1(&self) -> Option<&'static str> {
        self.to_iso()?.part1
    }
}

impl fmt::Display for Language {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_repr())
    }
}

impl fmt::Debug for Language {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_repr())
    }
}

impl FromStr for Language {
    type Err = ParseLanguageErr;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_iso(s).ok_or(ParseLanguageErr)
    }
}

impl serde::Serialize for Language {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.as_repr().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Language {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = Language;

            #[inline]
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a 2- or 3-letter ISO 639 language code or 'default'")
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

impl<M> Encode<M> for Language {
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

impl<'de, M, A> Decode<'de, M, A> for Language
where
    A: Allocator,
{
    #[inline]
    fn decode<D>(decoder: D) -> Result<Self, D::Error>
    where
        D: Decoder<'de, Mode = M, Allocator = A>,
    {
        let cx = decoder.cx();
        decoder.decode_unsized(|s: &str| s.parse::<Language>().map_err(cx.map()))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for Language {
    type Type = ::sqll::ty::Integer;

    #[inline]
    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        let value = i64::from_column(stmt, index)?;

        if let Ok(value) = u32::try_from(value) {
            let bytes = value.to_be_bytes();
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());

            // Only the shape is checked so that a code missing from the current
            // tables still reads; `as_raw_code` relies on it being ASCII.
            if bytes[..end].iter().all(u8::is_ascii_lowercase)
                && bytes[end..].iter().all(|&b| b == 0)
            {
                return Ok(Language(bytes));
            }
        }

        Err(::sqll::Error::new(
            ::sqll::Code::MISMATCH,
            "invalid stored language code",
        ))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for Language {
    #[inline]
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        i64::from(u32::from_be_bytes(self.0)).bind_value(stmt, index)
    }
}

#[cfg(feature = "yew")]
impl IntoPropValue<VNode> for Language {
    #[inline]
    fn into_prop_value(self) -> VNode {
        self.to_string().into()
    }
}

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
