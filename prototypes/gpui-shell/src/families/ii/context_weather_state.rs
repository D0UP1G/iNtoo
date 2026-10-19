//! Read-only ContextCard weather snapshot backed by Open-Meteo.
//!
//! This adapter deliberately uses only manually configured coordinates. It
//! never attempts GPS, IP geolocation, or geocoding, and has no API secret.

use crate::config::{ShellConfig, WeatherConfig};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const WEATHER_CACHE_TTL_MS: u64 = 3 * 24 * 60 * 60 * 1000;
const MAX_WEATHER_CACHE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextWeatherStatus {
    Loading,
    Ready,
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeatherSnapshot {
    pub condition: String,
    pub temperature: String,
    pub city: Option<String>,
    pub humidity: String,
    pub wind: String,
    pub feels_like: String,
    pub weather_code: u8,
    pub is_night: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextWeatherState {
    pub status: ContextWeatherStatus,
    pub snapshot: Option<WeatherSnapshot>,
    /// Cached readings are shown with an explicit stale indicator.
    pub stale: bool,
    /// QML-compatible epoch timestamp in milliseconds.
    pub updated_at_ms: Option<u64>,
}

impl ContextWeatherState {
    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            status: ContextWeatherStatus::Unavailable(message.into()),
            snapshot: None,
            stale: false,
            updated_at_ms: None,
        }
    }
}

/// Load a recent QML-compatible weather cache without waiting for the network.
pub async fn load_cached_context_weather(
    config: &ShellConfig,
    cache_path: &Path,
) -> ContextWeatherState {
    if !config.sidebar.context_show_weather || !config.bar.weather.enabled {
        return ContextWeatherState::unavailable("Weather is disabled");
    }
    read_cache(cache_path, &config.bar.weather).unwrap_or(ContextWeatherState {
        status: ContextWeatherStatus::Loading,
        snapshot: None,
        stale: false,
        updated_at_ms: None,
    })
}

/// Refresh current conditions and persist successful values. On failure, retain
/// a compatible cache entry for up to three days and mark it stale.
pub async fn load_context_weather(config: &ShellConfig, cache_path: &Path) -> ContextWeatherState {
    if !config.sidebar.context_show_weather || !config.bar.weather.enabled {
        return ContextWeatherState::unavailable("Weather is disabled");
    }
    let cached = read_cache(cache_path, &config.bar.weather);
    match load_weather(&config.bar.weather) {
        Ok(snapshot) => {
            let updated_at_ms = now_unix_ms();
            write_cache(cache_path, &config.bar.weather, &snapshot, updated_at_ms);
            ContextWeatherState {
                status: ContextWeatherStatus::Ready,
                snapshot: Some(snapshot),
                stale: false,
                updated_at_ms: Some(updated_at_ms),
            }
        }
        Err(message) => match cached {
            Some(mut state) => {
                state.status = ContextWeatherStatus::Unavailable(message);
                state.stale = true;
                state
            }
            None => ContextWeatherState::unavailable(message),
        },
    }
}

fn load_weather(config: &WeatherConfig) -> Result<WeatherSnapshot, String> {
    let (Some(lat), Some(lon)) = (config.manual_lat, config.manual_lon) else {
        return Err(if config
            .city
            .as_deref()
            .is_some_and(|city| !city.trim().is_empty())
        {
            "Weather needs manually configured coordinates; city lookup is unavailable"
        } else {
            "Set a city or manual coordinates in Weather settings"
        }
        .to_owned());
    };
    if lat == 0.0 && lon == 0.0 {
        return Err(if config
            .city
            .as_deref()
            .is_some_and(|city| !city.trim().is_empty())
        {
            "Weather city lookup is unavailable; set manual coordinates"
        } else {
            "Set a city or manual coordinates in Weather settings"
        }
        .to_owned());
    }
    if !lat.is_finite()
        || !lon.is_finite()
        || !(-90.0..=90.0).contains(&lat)
        || !(-180.0..=180.0).contains(&lon)
    {
        return Err("Configured weather coordinates are invalid".to_owned());
    }

    let mut url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,relative_humidity_2m,apparent_temperature,is_day,weather_code,wind_speed_10m&temperature_unit={}&wind_speed_unit={}&timezone=auto",
        if config.use_uscs {
            "fahrenheit"
        } else {
            "celsius"
        },
        if config.use_uscs { "mph" } else { "kmh" },
    );
    // Values above are finite, validated numeric configuration, never shell text.
    if url.len() > 2048 {
        return Err("Weather request could not be built".to_owned());
    }
    let output = match Command::new("/usr/bin/curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "12",
            "--",
            &url,
        ])
        .output()
    {
        Ok(output) if output.status.success() => output,
        Ok(_) => return Err("Weather provider request failed".to_owned()),
        Err(_) => {
            return Err("curl is unavailable for weather requests".to_owned());
        }
    };
    // Avoid retaining the URL beyond the request.
    url.clear();
    let parsed: Value = match serde_json::from_slice(&output.stdout) {
        Ok(value) => value,
        Err(_) => {
            return Err("Weather provider returned invalid data".to_owned());
        }
    };
    let Some(current) = parsed.get("current") else {
        return Err("Weather provider returned no current conditions".to_owned());
    };
    let number = |key: &str| current.get(key).and_then(Value::as_f64);
    let (Some(temp), Some(feels), Some(humidity), Some(wind), Some(code), Some(is_day)) = (
        number("temperature_2m"),
        number("apparent_temperature"),
        number("relative_humidity_2m"),
        number("wind_speed_10m"),
        current.get("weather_code").and_then(Value::as_u64),
        current.get("is_day").and_then(Value::as_u64),
    ) else {
        return Err("Weather provider returned incomplete conditions".to_owned());
    };
    let code = code.min(u8::MAX as u64) as u8;
    let unit_temp = if config.use_uscs { "°F" } else { "°C" };
    let unit_wind = if config.use_uscs { "mph" } else { "km/h" };
    let city = config
        .city
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned);
    Ok(WeatherSnapshot {
        condition: describe_code(code).to_owned(),
        temperature: format!("{temp:.0}{unit_temp}"),
        city,
        humidity: format!("{humidity:.0}%"),
        wind: format!("{wind:.0} {unit_wind}"),
        feels_like: format!("{feels:.0}{unit_temp}"),
        weather_code: code,
        is_night: is_day == 0,
    })
}

fn read_cache(path: &Path, config: &WeatherConfig) -> Option<ContextWeatherState> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_WEATHER_CACHE_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let root: Value = serde_json::from_slice(&bytes).ok()?;
    let root = root.as_object()?;
    let saved_at = root.get("savedAt")?.as_u64()?;
    let now = now_unix_ms();
    if saved_at == 0
        || saved_at > now.saturating_add(60_000)
        || now.saturating_sub(saved_at) > WEATHER_CACHE_TTL_MS
        || !cache_source_matches(root.get("source"), config)
    {
        return None;
    }
    let data = root.get("data")?.as_object()?;
    let get_text = |key: &str| data.get(key)?.as_str().map(str::to_owned);
    let temperature = get_text("temp")?;
    let feels_like = get_text("tempFeelsLike")?;
    let humidity = get_text("humidity")?;
    let wind = get_text("wind")?;
    let condition = get_text("description")?;
    let wmo_code = data
        .get("weatherCodeWmo")
        .and_then(Value::as_u64)
        .filter(|code| *code <= u8::MAX as u64)
        .map(|code| code as u8)
        .or_else(|| data.get("wCode")?.as_str().and_then(wmo_from_wttr))?;
    if temperature.trim().is_empty()
        || temperature.starts_with("--")
        || feels_like.trim().is_empty()
        || humidity.trim().is_empty()
        || wind.trim().is_empty()
    {
        return None;
    }
    let city = get_text("city")
        .filter(|city| !city.trim().is_empty() && !city.eq_ignore_ascii_case("unknown"));
    let is_night = data
        .get("isNight")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(ContextWeatherState {
        status: ContextWeatherStatus::Ready,
        snapshot: Some(WeatherSnapshot {
            condition,
            temperature,
            city,
            humidity,
            wind,
            feels_like,
            weather_code: wmo_code,
            is_night,
        }),
        stale: true,
        updated_at_ms: Some(saved_at),
    })
}

fn cache_source_matches(source: Option<&Value>, config: &WeatherConfig) -> bool {
    let Some(source) = source else {
        // Legacy QML cache has no source fingerprint; the QML service also
        // accepts it globally, so preserve that compatibility.
        return true;
    };
    source.get("city").and_then(Value::as_str) == config.city.as_deref()
        && optional_number_matches(source.get("manualLat"), config.manual_lat)
        && optional_number_matches(source.get("manualLon"), config.manual_lon)
        && source.get("useUSCS").and_then(Value::as_bool) == Some(config.use_uscs)
}

fn optional_number_matches(value: Option<&Value>, expected: Option<f64>) -> bool {
    match (value.and_then(Value::as_f64), expected) {
        (Some(actual), Some(expected)) => actual == expected,
        (None, None) => true,
        _ => false,
    }
}

fn write_cache(path: &Path, config: &WeatherConfig, snapshot: &WeatherSnapshot, saved_at: u64) {
    let Some(parent) = path.parent() else { return };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let payload = json!({
        "savedAt": saved_at,
        "source": {
            "city": config.city,
            "manualLat": config.manual_lat,
            "manualLon": config.manual_lon,
            "useUSCS": config.use_uscs,
        },
        "data": {
            "temp": snapshot.temperature,
            "tempFeelsLike": snapshot.feels_like,
            "city": snapshot.city.as_deref().unwrap_or("Unknown"),
            "humidity": snapshot.humidity,
            "wind": snapshot.wind,
            "wCode": wttr_from_wmo(snapshot.weather_code),
            "description": snapshot.condition,
            "weatherCodeWmo": snapshot.weather_code,
            "isNight": snapshot.is_night,
            "forecast": [],
            "hourly": [],
        },
    });
    let Ok(bytes) = serde_json::to_vec(&payload) else {
        return;
    };
    if bytes.len() as u64 > MAX_WEATHER_CACHE_BYTES {
        return;
    }
    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let temporary = path.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let Ok(mut file) = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
    else {
        return;
    };
    if file.write_all(&bytes).is_err() || file.sync_all().is_err() {
        drop(file);
        let _ = fs::remove_file(temporary);
        return;
    }
    drop(file);
    if fs::rename(&temporary, path).is_err() {
        let _ = fs::remove_file(temporary);
    }
}

fn wttr_from_wmo(code: u8) -> &'static str {
    match code {
        0 => "113",
        1 | 2 => "116",
        3 => "122",
        45 => "143",
        48 => "248",
        51 => "263",
        53 => "266",
        55 | 56 => "281",
        57 => "284",
        61 => "293",
        63 => "299",
        65 => "302",
        66 => "311",
        67 => "314",
        71 => "323",
        73 => "329",
        75 => "335",
        77 | 85 => "368",
        80 => "353",
        81 => "356",
        82 => "359",
        86 => "371",
        95 => "200",
        96 => "386",
        99 => "392",
        _ => "113",
    }
}

fn wmo_from_wttr(code: &str) -> Option<u8> {
    Some(match code {
        "113" => 0,
        "116" => 2,
        "119" | "122" => 3,
        "143" | "248" | "260" => 45,
        "176" | "263" | "266" | "281" => 51,
        "284" => 57,
        "293" | "296" => 61,
        "299" => 63,
        "302" => 65,
        "311" => 66,
        "314" => 67,
        "179" | "227" | "323" | "326" => 71,
        "230" | "329" | "332" => 73,
        "335" | "338" => 75,
        "368" => 77,
        "305" | "353" => 80,
        "356" => 81,
        "308" | "359" => 82,
        "371" | "395" => 86,
        "182" | "185" | "362" | "365" => 56,
        "317" | "374" => 66,
        "320" => 73,
        "350" => 57,
        "377" => 67,
        "389" => 96,
        "200" => 95,
        "386" => 96,
        "392" => 99,
        // Unknown/legacy weather-provider icon codes still retain all
        // measured conditions; render a conservative cloudy glyph.
        _ => 3,
    })
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn describe_code(code: u8) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 | 63 | 65 => "Rain",
        66 | 67 => "Freezing rain",
        71 | 73 | 75 | 77 => "Snow",
        80 | 81 | 82 => "Rain showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm with hail",
        _ => "Unknown conditions",
    }
}
