use std::{str::FromStr, time::SystemTime};

use jiff::{tz::TimeZone, Span, Timestamp, Zoned};

use crate::parse::Error;

fn parse_inner(input: &str) -> Option<Result<Span, Error>> {
    let mut split = input.split_whitespace();
    let units = i64::from_str(split.next()?).ok()?;
    let period = split.next()?;
    if split.next()? != "ago" {
        return None;
    }
    span(period, units)
}

pub fn parse(input: &str, now: Option<SystemTime>) -> Option<Result<Zoned, Error>> {
    parse_inner(input).map(|result| {
        let span = result?;
        // This was an error case in a previous version of this code, where
        // it would fail when converting from a negative signed integer
        // to an unsigned integer. This preserves that failure case even
        // though the code below handles it okay.
        if span.is_negative() {
            return Err(Error::RelativeTimeConversion);
        }
        now.ok_or(Error::MissingCurrentTime).and_then(|now| {
            let ts = Timestamp::try_from(now).map_err(|_| Error::RelativeTimeConversion)?;
            // N.B. This matches the behavior of this code when it was
            // written with `time`, but we might consider using the system
            // time zone here. If we did, then it would implement "1 day
            // ago" correctly, even when it crosses DST transitions. Since
            // we're in the UTC time zone here, which has no DST, 1 day is
            // in practice always 24 hours. ---AG
            let zdt = ts.to_zoned(TimeZone::UTC);
            zdt.checked_sub(span).map_err(|_| Error::RelativeTimeConversion)
        })
    })
}

fn span(period: &str, units: i64) -> Option<Result<Span, Error>> {
    let period = period.strip_suffix('s').unwrap_or(period);
    let result = match period {
        "second" => Span::new().try_seconds(units),
        "minute" => Span::new().try_minutes(units),
        "hour" => Span::new().try_hours(units),
        "day" => Span::new().try_days(units),
        "week" => Span::new().try_weeks(units),
        "month" => Span::new().try_months(units),
        "year" => Span::new().try_years(units),
        // Ignore values you don't know, assume seconds then (so does git)
        _anything => Span::new().try_seconds(units),
    };
    Some(result.map_err(|_| Error::RelativeTimeConversion))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_weeks_ago() {
        let actual = parse_inner("2 weeks ago").unwrap().unwrap();
        assert_eq!(actual.fieldwise(), Span::new().weeks(2));
    }
}
