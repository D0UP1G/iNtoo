//! Locale-aware data adapter for the QML WeekStrip.
//!
//! Formatting and first-weekday data come from the system locale through GNU
//! `date` and `locale`; no locale or date rules are guessed in Rust.

use std::{
    env,
    io::Write,
    process::{Command, Stdio},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeekStripSnapshot {
    pub week_offset: i32,
    /// Localized `MMMM yyyy` equivalent for the displayed week start.
    pub month_label: String,
    /// Seven cells ordered from the locale's first weekday.
    pub days: Vec<WeekDay>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeekDay {
    pub iso_date: String,
    /// Localized abbreviated weekday truncated to the first two Unicode scalar values.
    pub day_name: String,
    pub day_number: u32,
    pub is_today: bool,
    pub is_weekend: bool,
}

/// Builds the current local calendar week at `week_offset` from today.
///
/// UI controls should use offset -1 for previous week and wheel-up, +1 for
/// next week and wheel-down, and 0 for Today. Rebuild this snapshot after every
/// control action and when refreshing the visible panel to track date rollover.
pub fn build_snapshot(week_offset: i32) -> Result<WeekStripSnapshot, String> {
    let locale = selected_locale();
    let first_weekday = locale_first_weekday(locale.as_deref())?;
    let today_text = run_date(&["+%F"], locale.as_deref(), None)?;
    let (year, month, day) = parse_iso_date(today_text.trim())?;
    let today_days = days_from_civil(year, month, day);
    let weekday = (today_days + 4).rem_euclid(7) as u8; // 1970-01-01 was Thursday.
    let days_since_week_start = (i64::from(weekday) - i64::from(first_weekday)).rem_euclid(7);
    let week_start = today_days - days_since_week_start + i64::from(week_offset) * 7;
    let dates = (0..7)
        .map(|index| iso_date_from_days(week_start + index))
        .collect::<Vec<_>>();
    let input = dates.join("\n") + "\n";
    let output = run_date(
        &["--file=-", "+%B %Y\t%a\t%-d\t%w"],
        locale.as_deref(),
        Some(input.as_bytes()),
    )?;
    let lines = output.lines().collect::<Vec<_>>();
    if lines.len() != 7 {
        return Err(format!(
            "date returned {} calendar rows; expected 7",
            lines.len()
        ));
    }
    let mut month_label = None;
    let mut days = Vec::with_capacity(7);
    for (iso_date, line) in dates.into_iter().zip(lines) {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 4 {
            return Err("date returned an invalid calendar row".into());
        }
        let day_number = fields[2]
            .parse::<u32>()
            .map_err(|_| "date returned an invalid day number")?;
        let weekday = fields[3]
            .parse::<u8>()
            .map_err(|_| "date returned an invalid weekday number")?;
        if weekday > 6 {
            return Err("date returned an out-of-range weekday number".into());
        }
        if month_label.is_none() {
            month_label = Some(fields[0].to_owned());
        }
        days.push(WeekDay {
            is_today: iso_date == today_text.trim(),
            iso_date,
            day_name: fields[1].chars().take(2).collect(),
            day_number,
            is_weekend: weekday == 0 || weekday == 6,
        });
    }
    let month_label = month_label.ok_or_else(|| "date returned no month label".to_owned())?;
    Ok(WeekStripSnapshot {
        week_offset,
        month_label,
        days,
    })
}

fn selected_locale() -> Option<String> {
    ["LC_TIME", "LC_ALL", "LANG"]
        .iter()
        .filter_map(|key| env::var(key).ok())
        .find(|value| !value.is_empty())
        .map(|value| {
            value
                .split('.')
                .next()
                .unwrap_or_default()
                .split('@')
                .next()
                .unwrap_or_default()
                .to_owned()
        })
        .filter(|value| !value.is_empty())
}

fn locale_first_weekday(locale: Option<&str>) -> Result<u8, String> {
    let output = run_locale(locale)?;
    let value = output
        .lines()
        .find_map(|line| line.strip_prefix("first_weekday="))
        .ok_or_else(|| "locale did not report first_weekday".to_owned())?
        .trim()
        .parse::<u8>()
        .map_err(|_| "locale returned an invalid first_weekday".to_owned())?;
    // glibc's first_weekday is a 1-based index into the Sunday-first `abday` list.
    value
        .checked_sub(1)
        .filter(|weekday| *weekday <= 6)
        .ok_or_else(|| "locale first_weekday must be in 1..=7".into())
}

fn run_locale(locale: Option<&str>) -> Result<String, String> {
    let mut command = Command::new("locale");
    command.args(["-k", "first_weekday"]);
    if let Some(locale) = locale {
        command.env_remove("LC_ALL").env("LC_TIME", locale);
    }
    let output = command
        .output()
        .map_err(|error| format!("Could not query locale first_weekday: {error}"))?;
    if !output.status.success() {
        return Err("Could not query locale first_weekday".into());
    }
    String::from_utf8(output.stdout).map_err(|_| "locale returned invalid UTF-8".into())
}

fn run_date(args: &[&str], locale: Option<&str>, input: Option<&[u8]>) -> Result<String, String> {
    let mut command = Command::new("date");
    command.args(args);
    if let Some(locale) = locale {
        command.env_remove("LC_ALL").env("LC_TIME", locale);
    }
    if let Some(input) = input {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("Could not run date: {error}"))?;
        child
            .stdin
            .take()
            .ok_or_else(|| "Could not open date input".to_owned())?
            .write_all(input)
            .map_err(|error| format!("Could not provide calendar dates to date: {error}"))?;
        let output = child
            .wait_with_output()
            .map_err(|error| format!("Could not read date output: {error}"))?;
        if !output.status.success() {
            return Err("date could not format the current locale calendar".into());
        }
        return String::from_utf8(output.stdout).map_err(|_| "date returned invalid UTF-8".into());
    }
    let output = command
        .output()
        .map_err(|error| format!("Could not run date: {error}"))?;
    if !output.status.success() {
        return Err("date could not read the current local date".into());
    }
    String::from_utf8(output.stdout).map_err(|_| "date returned invalid UTF-8".into())
}

fn parse_iso_date(value: &str) -> Result<(i64, u32, u32), String> {
    let mut fields = value.split('-');
    let year = fields
        .next()
        .ok_or_else(|| "date returned an invalid ISO date".to_owned())?
        .parse::<i64>()
        .map_err(|_| "date returned an invalid ISO year".to_owned())?;
    let month = fields
        .next()
        .ok_or_else(|| "date returned an invalid ISO date".to_owned())?
        .parse::<u32>()
        .map_err(|_| "date returned an invalid ISO month".to_owned())?;
    let day = fields
        .next()
        .ok_or_else(|| "date returned an invalid ISO date".to_owned())?
        .parse::<u32>()
        .map_err(|_| "date returned an invalid ISO day".to_owned())?;
    if fields.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err("date returned an out-of-range ISO date".into());
    }
    Ok((year, month, day))
}

// Proleptic Gregorian calendar conversion, using days relative to 1970-01-01.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn iso_date_from_days(days: i64) -> String {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02}")
}
