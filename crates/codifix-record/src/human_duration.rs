// SPDX-License-Identifier: AGPL-3.0-or-later

//! The duration string format shared by operator CLI flags, config and API requests.

use std::fmt;
use std::time::Duration;

use serde::{Deserializer, Serializer, de};

const SECOND: u64 = 1_000_000_000;
const MINUTE: u64 = 60 * SECOND;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;
const MONTH: u64 = 2_630_016 * SECOND;
const YEAR: u64 = 31_557_600 * SECOND;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("value was empty")]
    Empty,
    #[error("expected number at {0}")]
    ExpectedNumber(usize),
    #[error("invalid character at {0}")]
    InvalidCharacter(usize),
    #[error("time unit needed, for example {0}sec or {0}ms")]
    MissingUnit(u64),
    #[error(
        "unknown time unit {0:?}, supported units: ns, us/µs, ms, sec, min, hours, days, weeks, months, years (and few variations)"
    )]
    UnknownUnit(String),
    #[error(
        "number is too large or cannot be represented without a lack of precision (values below 1ns are not supported)"
    )]
    Overflow,
}

/// Parse adjacent or space-separated amounts, such as `2h 30m` or `1.5s`.
pub fn parse(source: &str) -> Result<Duration, ParseError> {
    if source == "0" {
        return Ok(Duration::ZERO);
    }
    let mut input = source.trim_start();
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    let offset = |suffix: &str| source.len() - suffix.len();
    let mut total = Duration::ZERO;
    while !input.is_empty() {
        let (whole, digits, rest) = read_digits(input)?;
        if digits == 0 {
            return Err(ParseError::ExpectedNumber(offset(input)));
        }
        let (fraction, rest) = if let Some(decimal) = rest.strip_prefix('.') {
            let (numerator, digits, rest) = read_digits(decimal)?;
            if digits == 0 {
                return Err(ParseError::InvalidCharacter(offset(rest)));
            }
            let digits = u32::try_from(digits).map_err(|_| ParseError::Overflow)?;
            let denominator = 10u64.checked_pow(digits).ok_or(ParseError::Overflow)?;
            (Some((numerator, denominator)), rest)
        } else {
            (None, rest)
        };
        let unit_end = rest
            .find(|character: char| !character.is_ascii_alphabetic() && character != 'µ')
            .unwrap_or(rest.len());
        let (suffix, next) = rest.split_at(unit_end);
        if next.starts_with(|c: char| !c.is_ascii_digit() && !c.is_whitespace()) {
            return Err(ParseError::InvalidCharacter(offset(next)));
        }
        let nanos = match suffix {
            "nanos" | "nsec" | "ns" => 1,
            "usec" | "us" | "µs" => 1_000,
            "millis" | "msec" | "ms" => 1_000_000,
            "seconds" | "second" | "secs" | "sec" | "s" => SECOND,
            "minutes" | "minute" | "min" | "mins" | "m" => MINUTE,
            "hours" | "hour" | "hr" | "hrs" | "h" => HOUR,
            "days" | "day" | "d" => DAY,
            "weeks" | "week" | "wk" | "wks" | "w" => 7 * DAY,
            "months" | "month" | "M" => MONTH,
            "years" | "year" | "yr" | "yrs" | "y" => YEAR,
            "" => return Err(ParseError::MissingUnit(whole)),
            _ => return Err(ParseError::UnknownUnit(suffix.to_owned())),
        };
        total = total
            .checked_add(scale(whole, 1, nanos, nanos >= SECOND)?)
            .ok_or(ParseError::Overflow)?;
        if let Some((numerator, denominator)) = fraction {
            if nanos == 1 {
                return Err(ParseError::Overflow);
            }
            // Existing hour-and-larger fractions use whole-second precision.
            total = total
                .checked_add(scale(numerator, denominator, nanos, nanos >= HOUR)?)
                .ok_or(ParseError::Overflow)?;
        }
        input = next.trim_start();
    }
    Ok(total)
}

fn read_digits(input: &str) -> Result<(u64, usize, &str), ParseError> {
    let mut value = 0u64;
    let mut digits = 0;
    for (offset, character) in input.char_indices() {
        if let Some(digit) = character.to_digit(10) {
            value = value
                .checked_mul(10)
                .and_then(|value| value.checked_add(u64::from(digit)))
                .ok_or(ParseError::Overflow)?;
            digits += 1;
        } else if !character.is_whitespace() {
            return Ok((value, digits, &input[offset..]));
        }
    }
    Ok((value, digits, ""))
}

fn scale(
    value: u64,
    denominator: u64,
    nanos: u64,
    whole_seconds: bool,
) -> Result<Duration, ParseError> {
    let multiplier = if whole_seconds { nanos / SECOND } else { nanos };
    let amount = value.checked_mul(multiplier).ok_or(ParseError::Overflow)?;
    if amount % denominator != 0 {
        return Err(ParseError::Overflow);
    }
    let amount = amount / denominator;
    Ok(if whole_seconds {
        Duration::from_secs(amount)
    } else {
        Duration::from_nanos(amount)
    })
}

struct DisplayDuration(Duration);

impl fmt::Display for DisplayDuration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_zero() {
            return formatter.write_str("0s");
        }
        let mut nanos = self.0.as_nanos();
        let mut first = true;
        for (size, suffix, pluralize) in [
            (YEAR, "year", true),
            (MONTH, "month", true),
            (DAY, "day", true),
            (HOUR, "h", false),
            (MINUTE, "m", false),
            (SECOND, "s", false),
            (1_000_000, "ms", false),
            (1_000, "us", false),
            (1, "ns", false),
        ] {
            let size = u128::from(size);
            let value = nanos / size;
            nanos %= size;
            if value == 0 {
                continue;
            }
            if !first {
                formatter.write_str(" ")?;
            }
            write!(formatter, "{value}{suffix}")?;
            if pluralize && value > 1 {
                formatter.write_str("s")?;
            }
            first = false;
        }
        Ok(())
    }
}

pub fn serialize<S: Serializer>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(&DisplayDuration(*duration))
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
    struct DurationVisitor;
    impl de::Visitor<'_> for DurationVisitor {
        type Value = Duration;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a duration")
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<Duration, E> {
            parse(value).map_err(|_| E::invalid_value(de::Unexpected::Str(value), &self))
        }
    }
    deserializer.deserialize_str(DurationVisitor)
}

pub mod option {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Deserialize, Serialize)]
    struct DurationString(#[serde(with = "super")] Duration);

    pub fn serialize<S: Serializer>(
        value: &Option<Duration>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.map(DurationString).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Duration>, D::Error> {
        Ok(Option::<DurationString>::deserialize(deserializer)?.map(|value| value.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PruneHistoryRequest;

    #[test]
    fn duration_syntax_and_precision() {
        assert_eq!(parse("0"), Ok(Duration::ZERO));
        assert_eq!(
            parse(" 1hour30 min 1.5sec 3µs "),
            Ok(Duration::new(5_401, 500_003_000))
        );
        assert_eq!(parse("1y 1M 1wk"), Ok(Duration::from_secs(34_792_416)));
        assert_eq!(
            parse("18446744073709551615s 999999999ns"),
            Ok(Duration::MAX)
        );
        for text in [
            "",
            "6",
            "1.s",
            "0.5ns",
            "0.00001h",
            "-5s",
            "1dayx",
            "18446744073709551615s 1s",
        ] {
            assert!(parse(text).is_err(), "accepted {text}");
        }
    }

    #[test]
    fn existing_prune_history_request_strings_round_trip() {
        let request: PruneHistoryRequest = serde_json::from_str(
            r#"{"tombstone_after":"1M 2d 3h4m 5.006007008s","superseded_after":null}"#,
        )
        .unwrap();
        assert_eq!(
            request.tombstone_after,
            Some(Duration::new(2_813_861, 6_007_008))
        );
        assert!(request.superseded_after.is_none());
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(
            value["tombstone_after"],
            "1month 2days 3h 4m 5s 6ms 7us 8ns"
        );
        assert!(value["superseded_after"].is_null());
    }
}
