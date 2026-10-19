//! System-backed adapter for the QML WorldClockWidget.
//!
//! GNU `date` formats each requested IANA zone. Invalid or unavailable zones
//! remain visible as errored rows so the UI can report the actual identifier.

use std::{fs, process::Command};

use crate::config::WorldClockConfig;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldClockSnapshot {
    pub auto: bool,
    pub system_timezone: Option<String>,
    pub system_timezone_error: Option<String>,
    pub show_seconds: bool,
    pub use_24_hour: bool,
    pub show_date: bool,
    pub highlight_local: bool,
    pub rows: Vec<WorldClockRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldClockRow {
    pub timezone: String,
    pub city: String,
    pub is_local: bool,
    /// `None` when the timezone could not be sampled; QML displays day glyph on missing data.
    pub is_daytime: Option<bool>,
    pub time: Option<String>,
    /// GNU date `%:z`, including the sign (for example `-04:00`).
    pub utc_offset: Option<String>,
    /// Locale-formatted GNU date `%a %d %b`.
    pub date: Option<String>,
    /// Relative day offset, matching QML's day-of-year wrap rule. `None` if local time is absent.
    pub day_delta: Option<i32>,
    pub error: Option<String>,
    day_of_year: Option<i32>,
}

/// Builds a fresh snapshot. Call again when the panel opens or on the QML
/// refresh cadence (1s with seconds, otherwise 30s) to account for clock/date rollover.
pub fn build_snapshot(config: &WorldClockConfig) -> WorldClockSnapshot {
    let (system_timezone, system_timezone_error) = detect_system_timezone();
    let auto = config.timezones.is_empty();
    let mut timezones = if auto {
        suggested_timezones(system_timezone.as_deref())
    } else {
        config.timezones.clone()
    };
    if config.highlight_local {
        if let Some(local) = system_timezone.as_ref() {
            if timezones.iter().any(|timezone| timezone == local) {
                timezones.retain(|timezone| timezone != local);
                timezones.insert(0, local.clone());
            }
        }
    }

    let time_format = match (config.use_24_hour, config.show_seconds) {
        (true, true) => "%H:%M:%S",
        (true, false) => "%H:%M",
        (false, true) => "%I:%M:%S %p",
        (false, false) => "%I:%M %p",
    };
    let mut rows = timezones
        .into_iter()
        .map(|timezone| {
            let is_local = system_timezone.as_ref() == Some(&timezone);
            match sample_timezone(&timezone, time_format) {
                Ok(sample) => WorldClockRow {
                    city: city_name(&timezone),
                    timezone,
                    is_local,
                    is_daytime: Some((6..19).contains(&sample.hour)),
                    time: Some(sample.time),
                    utc_offset: Some(sample.utc_offset),
                    date: Some(sample.date),
                    day_delta: None,
                    error: None,
                    day_of_year: Some(sample.day_of_year),
                },
                Err(error) => WorldClockRow {
                    city: city_name(&timezone),
                    timezone,
                    is_local,
                    is_daytime: None,
                    time: None,
                    utc_offset: None,
                    date: None,
                    day_delta: None,
                    error: Some(error),
                    day_of_year: None,
                },
            }
        })
        .collect::<Vec<_>>();

    let local_day = rows
        .iter()
        .find(|row| row.is_local)
        .and_then(|row| row.day_of_year);
    if let Some(local_day) = local_day {
        for row in &mut rows {
            if let Some(day) = row.day_of_year {
                let difference = day - local_day;
                row.day_delta = Some(if difference > 300 {
                    -1
                } else if difference < -300 {
                    1
                } else {
                    difference
                });
            }
        }
    }

    WorldClockSnapshot {
        auto,
        system_timezone,
        system_timezone_error,
        show_seconds: config.show_seconds,
        use_24_hour: config.use_24_hour,
        show_date: config.show_date,
        highlight_local: config.highlight_local,
        rows,
    }
}

struct TimezoneSample {
    time: String,
    utc_offset: String,
    date: String,
    day_of_year: i32,
    hour: u32,
}

fn sample_timezone(timezone: &str, time_format: &str) -> Result<TimezoneSample, String> {
    validate_timezone(timezone)?;
    let format = format!("+{time_format}\t%:z\t%a %d %b\t%j\t%H");
    let output = Command::new("/usr/bin/date")
        .arg(&format)
        .env("TZ", timezone)
        .output()
        .map_err(|error| format!("could not run GNU date: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "GNU date failed for timezone {timezone}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| format!("GNU date returned non-UTF-8 output for timezone {timezone}"))?;
    let fields = stdout.trim_end().split('\t').collect::<Vec<_>>();
    if fields.len() != 5 {
        return Err(format!(
            "GNU date returned malformed output for timezone {timezone}"
        ));
    }
    Ok(TimezoneSample {
        time: fields[0].to_owned(),
        utc_offset: fields[1].to_owned(),
        date: fields[2].to_owned(),
        day_of_year: fields[3].parse().map_err(|_| {
            format!("GNU date returned invalid day-of-year for timezone {timezone}")
        })?,
        hour: fields[4]
            .parse()
            .map_err(|_| format!("GNU date returned invalid hour for timezone {timezone}"))?,
    })
}

fn validate_timezone(timezone: &str) -> Result<(), String> {
    let valid_components = !timezone.is_empty()
        && timezone.split('/').all(|component| {
            !component.is_empty()
                && component != "."
                && component != ".."
                && component
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+'))
        });
    if !valid_components {
        return Err(format!("invalid timezone identifier: {timezone}"));
    }
    let root = fs::canonicalize("/usr/share/zoneinfo")
        .map_err(|error| format!("timezone database is unavailable: {error}"))?;
    let path = root.join(timezone);
    let canonical =
        fs::canonicalize(&path).map_err(|_| format!("timezone is unavailable: {timezone}"))?;
    if !canonical.starts_with(&root) || !canonical.is_file() {
        return Err(format!("timezone is unavailable: {timezone}"));
    }
    Ok(())
}

fn detect_system_timezone() -> (Option<String>, Option<String>) {
    let target = match fs::read_link("/etc/localtime") {
        Ok(target) => target,
        Err(error) => {
            return (
                None,
                Some(format!("could not read /etc/localtime symlink: {error}")),
            );
        }
    };
    let target = target.to_string_lossy();
    let Some((_, timezone)) = target.rsplit_once("/zoneinfo/") else {
        return (
            None,
            Some(format!(
                "/etc/localtime does not identify an IANA timezone: {target}"
            )),
        );
    };
    match validate_timezone(timezone) {
        Ok(()) => (Some(timezone.to_owned()), None),
        Err(error) => (None, Some(format!("system timezone unavailable: {error}"))),
    }
}

fn suggested_timezones(system_timezone: Option<&str>) -> Vec<String> {
    let region = system_timezone
        .and_then(|timezone| timezone.split('/').next())
        .unwrap_or_default();
    let base = system_timezone
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let regional: &[&str] = match region {
        "America" => &["America/Los_Angeles", "Europe/London"],
        "Europe" => &["America/New_York", "Asia/Tokyo"],
        "Asia" => &["Europe/London", "America/New_York"],
        "Australia" | "Pacific" => &["Asia/Tokyo", "Europe/London"],
        "Africa" => &["Europe/London", "Asia/Dubai"],
        _ => &[],
    };
    let mut result = base;
    for timezone in
        regional
            .iter()
            .copied()
            .chain(["America/New_York", "Europe/London", "Asia/Tokyo"])
    {
        if !result.iter().any(|existing| existing == timezone) {
            result.push(timezone.to_owned());
        }
    }
    result.truncate(4);
    result
}

fn city_name(timezone: &str) -> String {
    timezone
        .rsplit('/')
        .next()
        .unwrap_or(timezone)
        .replace('_', " ")
}
