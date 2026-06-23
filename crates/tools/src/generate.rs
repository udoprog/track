use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

const URL: &str = "https://iso639-3.sil.org/sites/iso639-3/files/downloads/iso-639-3.tab";

const COUNTRIES_URL: &str = "https://raw.githubusercontent.com/lukes/ISO-3166-Countries-with-Regional-Codes/refs/heads/master/all/all.csv";

const LOCALES_URL: &str = "https://cdn.simplelocalize.io/public/v1/locales";

#[derive(Serialize)]
pub(super) struct Mapping {
    #[serde(rename = "iso-639-3")]
    id: String,
    #[serde(default, rename = "iso-639-1")]
    part1: Option<String>,
    #[serde(default, rename = "iso-3166-1")]
    flag: Option<String>,
}

#[derive(Debug)]
struct LanguageRow {
    id: String,
    part2b: Option<String>,
    part2t: Option<String>,
    part1: Option<String>,
    scope: Scope,
    ty: Type,
    name: String,
    comment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Scope {
    Individual,
    Macrolanguage,
    Special,
}

impl Scope {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "I" => Ok(Self::Individual),
            "M" => Ok(Self::Macrolanguage),
            "S" => Ok(Self::Special),
            _ => bail!("Unknown scope code: {value}"),
        }
    }

    fn rust_variant(self) -> &'static str {
        match self {
            Self::Individual => "Individual",
            Self::Macrolanguage => "Macrolanguage",
            Self::Special => "Special",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Type {
    Living,
    Extinct,
    Historical,
    Constructed,
    Special,
}

impl Type {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "L" => Ok(Self::Living),
            "E" => Ok(Self::Extinct),
            "H" => Ok(Self::Historical),
            "C" => Ok(Self::Constructed),
            "S" => Ok(Self::Special),
            _ => bail!("Unknown language type code: {value}"),
        }
    }

    fn rust_variant(self) -> &'static str {
        match self {
            Self::Living => "Living",
            Self::Extinct => "Extinct",
            Self::Historical => "Historical",
            Self::Constructed => "Constructed",
            Self::Special => "Special",
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawLanguageRow {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Part2b", deserialize_with = "deserialize_optional")]
    part2b: Option<String>,
    #[serde(rename = "Part2t", deserialize_with = "deserialize_optional")]
    part2t: Option<String>,
    #[serde(rename = "Part1", deserialize_with = "deserialize_optional")]
    part1: Option<String>,
    #[serde(rename = "Scope")]
    scope: String,
    #[serde(rename = "Language_Type")]
    ty: String,
    #[serde(rename = "Ref_Name")]
    name: String,
    #[serde(rename = "Comment", deserialize_with = "deserialize_optional")]
    comment: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawCountryRow {
    #[serde(rename = "name")]
    name: String,
    #[serde(rename = "alpha-2")]
    alpha2: String,
}

/// Generate the single `iso639` source module: the `ENTRIES` table. Each entry's `flag` is
/// the ISO 3166-1 flag code from the `to_3166_1` mapping, kept only when a matching flag SVG
/// exists in `flags_dir` (mirroring `has_flag` in the `iso3166` crate).
pub fn iso639_module(tab: &str, to_3166: &str, flags_dir: &Path) -> Result<(String, Vec<Mapping>)> {
    let entries = parse_rows(tab)?;

    let (flags1, flags3) = flag_map(to_3166, flags_dir)?;

    let mut o = String::new();
    write_header(&mut o, "crates/iso639/data/{iso-639-3.tab, to-3166-1.txt}")?;
    writeln!(o, "use super::{{Language, Type, Scope}};")?;
    writeln!(o)?;

    writeln!(o, "pub(super) const fn is_valid_id(id: &[u8]) -> bool {{")?;
    writeln!(o, "    match id {{")?;

    for e in &entries {
        let id = &e.id;
        writeln!(o, "        b\"{id}\" => true,")?;
    }

    writeln!(o, "        _ => false,")?;
    writeln!(o, "    }}")?;
    writeln!(o, "}}")?;

    let mapping = write_entries(&mut o, &entries, &flags1, &flags3)?;

    iso639_maps(&mut o, &entries)?;
    Ok((o, mapping))
}

/// Build the ISO 639-1 -> ISO 3166-1 flag-code map, keeping only entries whose target country
/// has a flag asset in `flags_dir`.
fn flag_map(
    csv: &str,
    flags_dir: &Path,
) -> Result<(BTreeMap<String, String>, BTreeMap<String, String>)> {
    #[derive(Debug, Deserialize)]
    struct Row {
        #[serde(rename = "iso-639-3")]
        iso_639_3: String,
        #[serde(rename = "iso-639-1", default)]
        iso_639_1: Option<String>,
        #[serde(rename = "iso-3166-1")]
        iso_3166_1: String,
    }

    let mut map1 = BTreeMap::new();
    let mut map3 = BTreeMap::new();

    let mut reader = csv::Reader::from_reader(csv.as_bytes());

    for (index, row) in reader.deserialize::<Row>().enumerate() {
        let row = row.with_context(|| anyhow!("Reading entry #{index}"))?;

        let iso3166_1 = row.iso_3166_1.trim();

        if !flags_dir
            .join(format!("{}.svg", iso3166_1.to_ascii_lowercase()))
            .is_file()
        {
            continue;
        }

        if let Some(part1) = &row.iso_639_1 {
            map1.insert(part1.to_ascii_lowercase(), iso3166_1.to_ascii_uppercase());
        }

        map3.insert(
            row.iso_639_3.to_ascii_lowercase(),
            iso3166_1.to_ascii_uppercase(),
        );
    }

    Ok((map1, map3))
}

fn write_entries(
    out: &mut String,
    entries: &[LanguageRow],
    flags1: &BTreeMap<String, String>,
    flags3: &BTreeMap<String, String>,
) -> Result<Vec<Mapping>> {
    let mut mapping = Vec::new();

    writeln!(out, "pub const ENTRIES: &[Language] = &[")?;

    for row in entries {
        let flag1 = row
            .part1
            .as_deref()
            .and_then(|id| flags1.get(id))
            .map(String::as_str);

        let flag3 = flags3.get(&row.id).map(String::as_str);

        let m = if let Some(flag) = flag1.or(flag3)
            && !flag.eq_ignore_ascii_case("UN")
        {
            Mapping {
                id: row.id.clone(),
                part1: row.part1.clone(),
                flag: Some(flag.to_owned()),
            }
        } else {
            Mapping {
                id: row.id.clone(),
                part1: row.part1.clone(),
                flag: None,
            }
        };

        mapping.push(m);

        writeln!(out, "    Language {{")?;
        writeln!(out, "        id: {:?},", row.id)?;
        write!(out, "        part2b: ")?;
        write_optional(out, row.part2b.as_deref())?;
        writeln!(out, ",")?;
        write!(out, "        part2t: ")?;
        write_optional(out, row.part2t.as_deref())?;
        writeln!(out, ",")?;
        write!(out, "        part1: ")?;
        write_optional(out, row.part1.as_deref())?;
        writeln!(out, ",")?;
        write!(out, "        flag: ")?;
        write_optional(out, flag3)?;
        writeln!(out, ",")?;
        writeln!(out, "        scope: Scope::{},", row.scope.rust_variant())?;
        writeln!(out, "        ty: Type::{},", row.ty.rust_variant())?;
        write!(out, "        name: ")?;
        write_string_literal(out, &row.name)?;
        writeln!(out, ",")?;
        write!(out, "        comment: ")?;
        write_optional(out, row.comment.as_deref())?;
        writeln!(out, ",")?;
        writeln!(out, "    }},")?;
    }

    writeln!(out, "];\n")?;
    Ok(mapping)
}

fn iso639_maps(out: &mut String, rows: &[LanguageRow]) -> Result<()> {
    let mut by_id = phf_codegen::Map::new();
    let mut by_part1 = phf_codegen::Map::new();

    for (i, row) in rows.iter().enumerate() {
        by_id.entry(&row.id, i.to_string());

        let Some(part1) = &row.part1 else {
            continue;
        };

        by_part1.entry(part1, i.to_string());
    }

    writeln!(
        out,
        "pub static BY_PART1: phf::Map<&'static str, usize> = {};",
        by_part1.build()
    )?;

    writeln!(
        out,
        "pub static BY_ID: phf::Map<&'static str, usize> = {};",
        by_id.build()
    )?;
    Ok(())
}

/// Generate the `iso3166` country table (`ENTRIES`) from the ISO 3166-1 CSV. `flags_dir` is
/// the directory of `4x3` flag SVGs; a country's `has_flag` is set when a `{alpha2}.svg`
/// exists there.
pub fn iso3166_module(csv: &str, flags_dir: &Path) -> Result<String> {
    if csv.trim().is_empty() {
        bail!("Dataset is empty");
    }

    let mut reader = csv::Reader::from_reader(csv.as_bytes());

    // Keyed by alpha-2 so output is sorted and de-duplicated, like the language maps.
    let mut entries: BTreeMap<String, (String, bool)> = BTreeMap::new();

    for (index, row) in reader.deserialize::<RawCountryRow>().enumerate() {
        let line_no = index + 2;
        let row = row.with_context(|| anyhow!("Parsing row at line {line_no}"))?;

        let alpha2 = row.alpha2.trim();
        let name = row.name.trim().to_owned();

        if alpha2.len() != 2 {
            bail!("Invalid ISO 3166-1 alpha-2 code at line {line_no}: {alpha2:?}");
        }

        if name.is_empty() {
            bail!("Empty country name at line {line_no}");
        }

        let has_flag = flags_dir
            .join(format!("{}.svg", alpha2.to_ascii_lowercase()))
            .is_file();

        entries.insert(alpha2.to_owned(), (name, has_flag));
    }

    if entries.is_empty() {
        bail!("Dataset has no data rows");
    }

    let mut o = String::new();

    write_header(&mut o, COUNTRIES_URL)?;

    writeln!(o, "use super::Country;")?;
    writeln!(o)?;
    writeln!(
        o,
        "pub(super) const fn is_valid_alpha2(alpha2: &[u8]) -> bool {{"
    )?;
    writeln!(o, "    match alpha2 {{")?;

    for alpha2 in entries.keys() {
        writeln!(o, "        b\"{alpha2}\" => true,")?;
    }

    writeln!(o, "        _ => false,")?;
    writeln!(o, "    }}")?;
    writeln!(o, "}}")?;
    writeln!(o)?;
    writeln!(o, "pub(super) const ENTRIES: &[Country] = &[")?;

    for (alpha2, (name, has_flag)) in &entries {
        write!(o, "    Country {{ alpha2: {alpha2:?}, name: ")?;
        write_string_literal(&mut o, name)?;
        writeln!(o, ", has_flag: {has_flag} }},")?;
    }

    writeln!(o, "];\n")?;

    let mut by_alpha2 = phf_codegen::Map::new();

    for (i, alpha2) in entries.keys().enumerate() {
        by_alpha2.entry(alpha2, i.to_string());
    }

    writeln!(
        o,
        "pub static BY_ALPHA2: phf::Map<&'static str, usize> = {};",
        by_alpha2.build()
    )?;

    Ok(o)
}

/// Generate a Sass partial exposing a `$names` list built from every `<name>.svg` in
/// `svg_dir`. Names are sorted for deterministic output so regeneration produces no diff
/// when the set is unchanged. A hand-written partial iterates the list to build the
/// per-name rules (e.g. `_flags.scss`, `_icons.scss`).
pub fn svg_names_scss(svg_dir: &Path, with_uppercase: bool) -> Result<String> {
    if !svg_dir.is_dir() {
        bail!("SVG directory does not exist: {}", svg_dir.display());
    }

    // BTreeSet keeps the output sorted and de-duplicated.
    let mut names: BTreeSet<String> = BTreeSet::new();

    for entry in fs::read_dir(svg_dir)
        .with_context(|| anyhow!("Reading SVG directory {}", svg_dir.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();

        let Some(name) = name.to_str() else {
            continue;
        };

        if let Some(stem) = name.strip_suffix(".svg") {
            names.insert(stem.to_owned());
        }
    }

    if names.is_empty() {
        bail!("No SVGs found in {}", svg_dir.display());
    }

    let mut out = String::new();
    writeln!(
        out,
        "// @generated by `cargo run -p tools`; do not edit by hand."
    )?;
    writeln!(out)?;
    writeln!(out, "$names: (")?;

    for name in &names {
        if with_uppercase {
            writeln!(out, "    ('{}', '{name}'),", name.to_uppercase())?;
        } else {
            writeln!(out, "    '{name}',")?;
        }
    }

    writeln!(out, ");")?;
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct RawLocale {
    language: RawLocaleLanguage,
    country: RawLocaleCountry,
}

#[derive(Debug, Deserialize)]
struct RawLocaleLanguage {
    iso_639_3: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawLocaleCountry {
    code: Option<String>,
}

/// The set of valid ISO 639-3 ids (3-letter, lowercase) from the language dataset.
pub fn language_ids(tab: &str) -> Result<BTreeSet<String>> {
    Ok(parse_rows(tab)?
        .into_iter()
        .map(|r| r.id.to_ascii_lowercase())
        .collect())
}

/// The set of valid ISO 3166-1 alpha-2 codes (uppercase) from the country dataset.
pub fn country_codes(csv: &str) -> Result<BTreeSet<String>> {
    if csv.trim().is_empty() {
        bail!("Dataset is empty");
    }

    let mut reader = csv::Reader::from_reader(csv.as_bytes());
    let mut out = BTreeSet::new();

    for (index, row) in reader.deserialize::<RawCountryRow>().enumerate() {
        let row = row.with_context(|| anyhow!("Parsing country row #{index}"))?;
        let alpha2 = row.alpha2.trim();

        if alpha2.len() == 2 {
            out.insert(alpha2.to_ascii_uppercase());
        }
    }

    Ok(out)
}

/// Generate the `locales` source module (`ENTRIES` + `BY_KEY`) from the SimpleLocalize
/// payload, keeping only combinations whose language resolves in `valid_languages` and
/// whose country resolves in `valid_countries`.
pub fn locales_module(
    json: &str,
    valid_languages: &BTreeSet<String>,
    valid_countries: &BTreeSet<String>,
) -> Result<String> {
    let raw: Vec<RawLocale> =
        serde_json::from_str(json).context("Parsing SimpleLocalize locales JSON")?;

    // BTreeSet keeps the output sorted and de-duplicated.
    let mut entries: BTreeSet<(String, String)> = BTreeSet::new();

    for entry in raw {
        let Some(language) = entry.language.iso_639_3 else {
            continue;
        };
        let Some(country) = entry.country.code else {
            continue;
        };

        let language = language.trim().to_ascii_lowercase();
        let country = country.trim().to_ascii_uppercase();

        if !valid_languages.contains(&language) || !valid_countries.contains(&country) {
            continue;
        }

        entries.insert((language, country));
    }

    if entries.is_empty() {
        bail!("No valid locale combinations found");
    }

    let mut out = String::new();
    write_header(&mut out, LOCALES_URL)?;
    writeln!(out, "use super::Locale;")?;
    writeln!(out)?;
    writeln!(out, "pub const ENTRIES: &[Locale] = &[")?;

    for (language, country) in &entries {
        writeln!(
            out,
            "    Locale {{ language: {language:?}, country: {country:?} }},"
        )?;
    }

    writeln!(out, "];\n")?;

    let mut by_key = phf_codegen::Map::new();
    let keys: Vec<String> = entries
        .iter()
        .map(|(language, country)| format!("{language}-{country}"))
        .collect();

    for (i, k) in keys.iter().enumerate() {
        by_key.entry(k.as_str(), i.to_string());
    }

    writeln!(
        out,
        "pub static BY_KEY: phf::Map<&'static str, usize> = {};",
        by_key.build()
    )?;

    Ok(out)
}

pub fn download_locales() -> Result<String> {
    reqwest::blocking::get(LOCALES_URL)
        .and_then(|response| response.error_for_status())
        .context("Downloading SimpleLocalize locales dataset")?
        .text()
        .context("Reading SimpleLocalize locales response body")
}

pub fn download_table() -> Result<String> {
    reqwest::blocking::get(URL)
        .and_then(|response| response.error_for_status())
        .context("Downloading ISO 639-3 dataset")?
        .text()
        .context("Reading ISO 639-3 response body")
}

pub fn download_countries() -> Result<String> {
    reqwest::blocking::get(COUNTRIES_URL)
        .and_then(|response| response.error_for_status())
        .context("Downloading ISO 3166 dataset")?
        .text()
        .context("Reading ISO 3166 response body")
}

fn parse_rows(input: &str) -> Result<Vec<LanguageRow>> {
    if input.trim().is_empty() {
        bail!("Dataset is empty");
    }

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_reader(input.trim_start_matches('\u{feff}').as_bytes());

    let mut rows = Vec::new();

    for (index, row) in reader.deserialize::<RawLanguageRow>().enumerate() {
        let line_no = index + 2;
        let row = row.with_context(|| anyhow!("Parsing row at line {line_no}"))?;

        let id = row.id.trim();
        let scope = row.scope.trim();
        let ty = row.ty.trim();
        let name = row.name.trim();

        if id.len() != 3 {
            bail!("Invalid language id at line {line_no}: {id:?}");
        }

        if scope.is_empty() {
            bail!("Empty scope at line {line_no}");
        }

        if ty.is_empty() {
            bail!("Empty language type at line {line_no}");
        }

        if name.is_empty() {
            bail!("Empty ref name at line {line_no}");
        }

        let scope =
            Scope::parse(scope).with_context(|| anyhow!("Invalid scope at line {line_no}"))?;
        let ty =
            Type::parse(ty).with_context(|| anyhow!("Invalid language type at line {line_no}"))?;

        rows.push(LanguageRow {
            id: id.to_owned(),
            part2b: row.part2b,
            part2t: row.part2t,
            part1: row.part1,
            scope,
            ty,
            name: name.to_owned(),
            comment: row.comment,
        });
    }

    if rows.is_empty() {
        bail!("Dataset has no data rows");
    }

    rows.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(rows)
}

fn parse_optional(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn deserialize_optional<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    Ok(value.and_then(|v| parse_optional(&v)))
}

fn write_header(out: &mut String, source: &str) -> Result<()> {
    writeln!(
        out,
        "// @generated by `cargo run -p tools`; do not edit by hand."
    )?;
    writeln!(out, "// Source: {source}")?;
    writeln!(out)?;
    Ok(())
}

fn write_optional(out: &mut String, value: Option<&str>) -> Result<()> {
    match value {
        Some(value) => {
            write!(out, "Some(")?;
            write_string_literal(out, value)?;
            write!(out, ")")?;
        }
        None => write!(out, "None")?,
    }

    Ok(())
}

fn write_string_literal(out: &mut String, value: &str) -> Result<()> {
    write!(out, "{value:?}")?;
    Ok(())
}
