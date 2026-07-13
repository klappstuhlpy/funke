//! Unit conversion: `100 mb in gb`, `72 f in c`, `5 km in miles`.
//!
//! Pure arithmetic over a table — offline, no dependency, and no network request, which is the
//! whole reason **currency is not here**. A rate has to come from somewhere, and SECURITY.md
//! enumerates the four kinds of request Funke makes; a fifth would need its own line, its own
//! opt-in, and a decision about whose exchange rate is the true one. Length and mass do not have
//! that problem: a kilometre was a kilometre yesterday too.
//!
//! The parser is deliberately strict. It is asked about **every keystroke the user types**, so it
//! must say no to prose instantly and without ever guessing: `go to work` and `in the morning`
//! both contain a separator word, and neither is a conversion. Nothing here does fuzzy matching
//! on a unit name — a calculator that shows a wrong answer is worse than one that shows none, so
//! an unrecognized unit is silence.

/// What a unit measures. Converting across two of these is a question with no answer, and the
/// answer to a question with no answer is no row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dimension {
    Length,
    Mass,
    Temperature,
    Data,
    Time,
    Speed,
    Area,
    Volume,
}

/// One unit: how it is spelled, what it measures, and how it relates to its dimension's base.
///
/// `to_base(v) = v * factor + offset`. The offset exists for exactly one dimension —
/// temperature is **affine, not proportional**: 0 °C is not zero heat, so 20 °C is not "twice"
/// 10 °C, and a ratio-only table would confidently return 50 °F for 10 °C. Every other unit
/// leaves it at zero.
struct Unit {
    /// Lowercase, and matched **exactly**. English and German spellings are both always live —
    /// the same reasoning as `alias_score`: muscle memory is not a language, and someone who
    /// learned `kg` does not want `Kilogramm` to be the only thing that works.
    aliases: &'static [&'static str],
    dimension: Dimension,
    /// What the result is written in. A symbol, not a translation — `ft` is `ft` in German too.
    symbol: &'static str,
    factor: f64,
    offset: f64,
}

const fn unit(aliases: &'static [&'static str], dimension: Dimension, symbol: &'static str, factor: f64) -> Unit {
    Unit {
        aliases,
        dimension,
        symbol,
        factor,
        offset: 0.0,
    }
}

use Dimension::{Area, Data, Length, Mass, Speed, Temperature, Time, Volume};

/// Base units: metre, gram, byte, second, metre per second, square metre, litre, and — for the
/// affine one — degrees Celsius.
const UNITS: &[Unit] = &[
    // Length.
    unit(&["mm", "millimeter", "millimetre", "millimeters"], Length, "mm", 0.001),
    unit(&["cm", "centimeter", "centimetre", "zentimeter"], Length, "cm", 0.01),
    unit(&["m", "meter", "metre", "meters"], Length, "m", 1.0),
    unit(&["km", "kilometer", "kilometre", "kilometers"], Length, "km", 1000.0),
    unit(&["in", "inch", "inches", "zoll"], Length, "in", 0.0254),
    unit(&["ft", "foot", "feet", "fuss"], Length, "ft", 0.3048),
    unit(&["yd", "yard", "yards"], Length, "yd", 0.9144),
    unit(&["mi", "mile", "miles", "meile"], Length, "mi", 1609.344),
    unit(&["nmi", "nauticalmile"], Length, "nmi", 1852.0),
    // Mass.
    unit(&["mg", "milligram", "milligramm"], Mass, "mg", 0.001),
    unit(&["g", "gram", "gramm", "grams"], Mass, "g", 1.0),
    unit(&["kg", "kilogram", "kilogramm", "kilo"], Mass, "kg", 1000.0),
    unit(&["t", "tonne", "ton", "tonnes"], Mass, "t", 1_000_000.0),
    unit(&["oz", "ounce", "ounces", "unze"], Mass, "oz", 28.349523125),
    unit(&["lb", "lbs", "pound", "pounds", "pfund"], Mass, "lb", 453.59237),
    unit(&["st", "stone", "stones"], Mass, "st", 6350.29318),
    // Temperature — the affine one.
    Unit {
        aliases: &["c", "°c", "celsius", "centigrade"],
        dimension: Temperature,
        symbol: "°C",
        factor: 1.0,
        offset: 0.0,
    },
    Unit {
        aliases: &["f", "°f", "fahrenheit"],
        dimension: Temperature,
        symbol: "°F",
        factor: 5.0 / 9.0,
        offset: -32.0 * 5.0 / 9.0,
    },
    Unit {
        aliases: &["k", "kelvin"],
        dimension: Temperature,
        symbol: "K",
        factor: 1.0,
        offset: -273.15,
    },
    // Data. Decimal and binary are *different units*, not a rounding difference — that is the
    // whole reason a disk sold as 1 TB shows up as 931 GiB, and a converter that conflated them
    // would be answering the wrong question.
    unit(&["bit", "bits"], Data, "bit", 0.125),
    unit(&["b", "byte", "bytes"], Data, "B", 1.0),
    unit(&["kb", "kilobyte", "kilobytes"], Data, "kB", 1e3),
    unit(&["mb", "megabyte", "megabytes"], Data, "MB", 1e6),
    unit(&["gb", "gigabyte", "gigabytes"], Data, "GB", 1e9),
    unit(&["tb", "terabyte", "terabytes"], Data, "TB", 1e12),
    unit(&["pb", "petabyte", "petabytes"], Data, "PB", 1e15),
    unit(&["kib", "kibibyte", "kibibytes"], Data, "KiB", 1024.0),
    unit(&["mib", "mebibyte", "mebibytes"], Data, "MiB", 1_048_576.0),
    unit(&["gib", "gibibyte", "gibibytes"], Data, "GiB", 1_073_741_824.0),
    unit(&["tib", "tebibyte", "tebibytes"], Data, "TiB", 1_099_511_627_776.0),
    // Time.
    unit(&["ms", "millisecond", "milliseconds"], Time, "ms", 0.001),
    unit(&["s", "sec", "secs", "second", "seconds", "sekunde"], Time, "s", 1.0),
    unit(&["min", "mins", "minute", "minutes", "minuten"], Time, "min", 60.0),
    unit(
        &["h", "hr", "hrs", "hour", "hours", "stunde", "stunden"],
        Time,
        "h",
        3600.0,
    ),
    unit(&["d", "day", "days", "tag", "tage"], Time, "d", 86_400.0),
    unit(&["w", "week", "weeks", "woche", "wochen"], Time, "w", 604_800.0),
    unit(&["y", "yr", "year", "years", "jahr", "jahre"], Time, "y", 31_557_600.0),
    // Speed.
    unit(&["m/s", "mps"], Speed, "m/s", 1.0),
    unit(&["km/h", "kmh", "kph"], Speed, "km/h", 1000.0 / 3600.0),
    unit(&["mph", "mi/h"], Speed, "mph", 1609.344 / 3600.0),
    unit(&["kn", "knot", "knots", "knoten"], Speed, "kn", 1852.0 / 3600.0),
    unit(&["ft/s", "fps"], Speed, "ft/s", 0.3048),
    // Area.
    unit(&["cm2", "cm²", "sqcm"], Area, "cm²", 0.0001),
    unit(&["m2", "m²", "sqm"], Area, "m²", 1.0),
    unit(&["km2", "km²", "sqkm"], Area, "km²", 1e6),
    unit(&["ha", "hectare", "hectares", "hektar"], Area, "ha", 10_000.0),
    unit(&["ft2", "ft²", "sqft"], Area, "ft²", 0.09290304),
    unit(&["in2", "in²", "sqin"], Area, "in²", 0.00064516),
    unit(&["acre", "acres"], Area, "acre", 4046.8564224),
    unit(&["mi2", "mi²", "sqmi"], Area, "mi²", 2_589_988.110336),
    // Volume.
    unit(&["ml", "milliliter", "millilitre"], Volume, "ml", 0.001),
    unit(&["cl", "centiliter", "centilitre"], Volume, "cl", 0.01),
    unit(&["l", "liter", "litre", "liters", "litres"], Volume, "l", 1.0),
    unit(&["m3", "m³", "cbm"], Volume, "m³", 1000.0),
    unit(&["gal", "gallon", "gallons"], Volume, "gal", 3.785411784),
    unit(&["qt", "quart", "quarts"], Volume, "qt", 0.946352946),
    unit(&["pt", "pint", "pints"], Volume, "pt", 0.473176473),
    unit(&["floz", "fl-oz"], Volume, "fl oz", 0.0295735295625),
    unit(&["cup", "cups"], Volume, "cup", 0.2365882365),
    unit(&["tbsp"], Volume, "tbsp", 0.01478676478125),
    unit(&["tsp"], Volume, "tsp", 0.00492892159375),
];

/// The words that mean "convert this into that".
///
/// `in` is on the list **and** is a unit (inches), which is the one genuinely ambiguous thing
/// in this file — see [`parse`] for how `12 in to cm` and `30 cm in in` both come out right.
const SEPARATORS: &[&str] = &["in", "to", "as", "nach", "zu"];

pub struct Conversion {
    /// The converted number, already rendered.
    pub value: String,
    /// The symbol it is in, e.g. `ft`.
    pub symbol: &'static str,
}

impl Conversion {
    /// What the row shows, and what "copy with the unit" copies: `328.08 ft`.
    pub fn labelled(&self) -> String {
        format!("{} {}", self.value, self.symbol)
    }
}

/// `100 m in ft` → `328.08399 ft`, or `None` — for prose, for an unknown unit, and for a
/// question with no answer (`5 kg in litres`).
pub fn convert(text: &str) -> Option<Conversion> {
    let tokens: Vec<String> = text.split_whitespace().map(|t| t.to_lowercase()).collect();
    parse(&tokens)
}

fn parse(tokens: &[String]) -> Option<Conversion> {
    // Right to left, because the separator can also be a unit. `12 in to cm` has two candidates
    // and only the rightmost one splits it into something that parses; `30 cm in in` has two and
    // only the *leftmost* does. Trying them in order and keeping the first that works settles
    // both without either being a special case.
    let candidates = (1..tokens.len().saturating_sub(1))
        .rev()
        .filter(|index| SEPARATORS.contains(&tokens[*index].as_str()));

    for index in candidates {
        let (left, right) = (&tokens[..index], &tokens[index + 1..]);
        if right.len() != 1 {
            continue;
        }
        let Some((amount, from)) = amount(left) else {
            continue;
        };
        let (Some(from), Some(to)) = (lookup(from), lookup(&right[0])) else {
            continue;
        };
        // Kilograms in litres is not a conversion, it is a question about what is in the bottle.
        if from.dimension != to.dimension {
            continue;
        }
        let base = amount * from.factor + from.offset;
        let value = (base - to.offset) / to.factor;
        if !value.is_finite() {
            return None;
        }
        return Some(Conversion {
            value: render(value),
            symbol: to.symbol,
        });
    }
    None
}

/// The left-hand side: `["100", "mb"]`, or `["100mb"]` — people type both.
fn amount(tokens: &[String]) -> Option<(f64, &str)> {
    match tokens {
        [both] => {
            let end = both.find(|c: char| !matches!(c, '0'..='9' | '.' | ',' | '-' | '+'))?;
            let (digits, unit) = both.split_at(end);
            Some((number(digits)?, unit))
        }
        [value, unit] => Some((number(value)?, unit.as_str())),
        _ => None,
    }
}

/// A decimal number, in either of the two ways half of Europe writes one. A comma is read as a
/// decimal point only when there is no point already — `1,5` is one and a half, and `1,500.25`
/// is not something this has to understand.
fn number(text: &str) -> Option<f64> {
    let normalized = if text.contains('.') {
        text.to_string()
    } else {
        text.replace(',', ".")
    };
    normalized.parse().ok()
}

fn lookup(alias: &str) -> Option<&'static Unit> {
    UNITS.iter().find(|unit| unit.aliases.contains(&alias))
}

/// Enough digits to be useful, not so many that the row is a wall of float noise. Trailing
/// zeros go, because `0.100000 GB` reads like a measurement and it is just a tenth.
fn render(value: f64) -> String {
    if value != 0.0 && (value.abs() < 1e-4 || value.abs() >= 1e15) {
        return format!("{value:.4e}");
    }
    let mut text = format!("{value:.6}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert_to(text: &str) -> Option<String> {
        convert(text).map(|c| c.labelled())
    }

    #[test]
    fn every_dimension_converts() {
        assert_eq!(convert_to("100 m in ft").as_deref(), Some("328.08399 ft"));
        assert_eq!(convert_to("5 km in miles").as_deref(), Some("3.106856 mi"));
        assert_eq!(convert_to("2 kg in lb").as_deref(), Some("4.409245 lb"));
        assert_eq!(convert_to("90 min in h").as_deref(), Some("1.5 h"));
        assert_eq!(convert_to("100 km/h in mph").as_deref(), Some("62.137119 mph"));
        assert_eq!(convert_to("1 ha in m2").as_deref(), Some("10000 m²"));
        assert_eq!(convert_to("2 l in ml").as_deref(), Some("2000 ml"));
    }

    /// Temperature is affine, and this is the test that would catch a table which treated it as
    /// a ratio: 0 °C would come out as 0 °F, which is minus eighteen.
    #[test]
    fn temperature_is_affine_not_proportional() {
        assert_eq!(convert_to("0 c in f").as_deref(), Some("32 °F"));
        assert_eq!(convert_to("100 c in f").as_deref(), Some("212 °F"));
        assert_eq!(convert_to("72 f in c").as_deref(), Some("22.222222 °C"));
        assert_eq!(convert_to("0 c in k").as_deref(), Some("273.15 K"));
        assert_eq!(convert_to("0 k in c").as_deref(), Some("-273.15 °C"));
        assert_eq!(
            convert_to("-40 c in f").as_deref(),
            Some("-40 °F"),
            "the one place they meet"
        );
    }

    /// A terabyte and a tebibyte are different units, which is why a 1 TB disk shows up as 931
    /// GiB and everybody thinks they have been robbed.
    #[test]
    fn decimal_and_binary_data_units_are_not_the_same_unit() {
        assert_eq!(convert_to("1 kb in b").as_deref(), Some("1000 B"));
        assert_eq!(convert_to("1 kib in b").as_deref(), Some("1024 B"));
        assert_eq!(convert_to("1 tb in gib").as_deref(), Some("931.322575 GiB"));
        assert_eq!(convert_to("100 mb in gb").as_deref(), Some("0.1 GB"));
        assert_eq!(convert_to("1 byte in bit").as_deref(), Some("8 bit"));
    }

    /// `in` is both a separator and a unit. Both of these have to come out right, and they are
    /// resolved by which split *parses*, not by a rule about which `in` is which.
    #[test]
    fn the_word_in_is_also_a_unit_and_both_readings_work() {
        assert_eq!(convert_to("12 in to cm").as_deref(), Some("30.48 cm"));
        assert_eq!(convert_to("30 cm in in").as_deref(), Some("11.811024 in"));
        assert_eq!(convert_to("100 in in cm").as_deref(), Some("254 cm"));
    }

    #[test]
    fn a_query_that_is_not_a_conversion_is_not_answered() {
        // Prose that happens to contain a separator word.
        assert_eq!(convert_to("go to work"), None);
        assert_eq!(convert_to("in the morning"), None);
        assert_eq!(convert_to("notes to self"), None);
        // A number, but nothing to convert.
        assert_eq!(convert_to("5 to 10"), None);
        // Real units, but the question has no answer.
        assert_eq!(convert_to("5 kg in litres"), None);
        assert_eq!(convert_to("20 c in kg"), None);
        // A unit nobody has heard of. Not guessed at — see the module note.
        assert_eq!(convert_to("5 furlongs in m"), None);
        assert_eq!(convert_to("2+2"), None, "the calculator's job, not this one's");
    }

    #[test]
    fn the_number_and_the_unit_may_be_written_together_and_in_either_decimal_style() {
        assert_eq!(convert_to("100mb in gb").as_deref(), Some("0.1 GB"));
        assert_eq!(convert_to("1.5 kg in g").as_deref(), Some("1500 g"));
        assert_eq!(convert_to("1,5 kg in g").as_deref(), Some("1500 g"), "German decimals");
        assert_eq!(convert_to("-5 c in f").as_deref(), Some("23 °F"));
    }

    #[test]
    fn units_are_case_insensitive_and_speak_both_languages() {
        assert_eq!(convert_to("2 KG in LB").as_deref(), Some("4.409245 lb"));
        assert_eq!(convert_to("2 Kilogramm in Pfund").as_deref(), Some("4.409245 lb"));
        assert_eq!(convert_to("5 stunden in minuten").as_deref(), Some("300 min"));
    }

    /// Two units answering to the same word would make one of them unreachable — and which one
    /// depends on the order of a table, which is not a thing anybody should have to know.
    #[test]
    fn no_alias_belongs_to_two_units() {
        let mut seen: Vec<&str> = Vec::new();
        for unit in UNITS {
            for alias in unit.aliases {
                assert!(!seen.contains(alias), "`{alias}` is claimed by two units");
                seen.push(alias);
            }
        }
    }

    /// A separator that was also a unit *of the same name in the same position* could never be
    /// reached. `in` is the only overlap and it works (above); this pins the rest shut.
    #[test]
    fn no_separator_but_in_is_a_unit() {
        for separator in SEPARATORS {
            let is_unit = lookup(separator).is_some();
            assert_eq!(is_unit, *separator == "in", "`{separator}` overlaps a unit name");
        }
    }

    #[test]
    fn very_large_and_very_small_results_stay_readable() {
        assert_eq!(convert_to("1 b in tb").as_deref(), Some("1.0000e-12 TB"));
        assert_eq!(convert_to("1 y in ms").as_deref(), Some("31557600000 ms"));
    }
}
