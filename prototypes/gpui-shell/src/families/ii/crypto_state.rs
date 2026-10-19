//! CoinGecko adapter and strict on-disk cache for the Widgets crypto card.
//!
//! These functions perform blocking file/process I/O internally. Call them only
//! from a GPUI background executor, never from the UI thread.

use crate::config::ShellConfig;
use serde_json::{Map, Value, json};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_HISTORY_SAMPLES: usize = 20;
/// The existing QML settings picker exposes 18 selectable coins.
const MAX_HISTORY_REQUESTS: usize = 18;
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

static CACHE_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static NEXT_QUOTE_REQUEST: AtomicU64 = AtomicU64::new(1);
static LAST_QUOTE_COMMIT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoStatus {
    Loading,
    Ready,
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CryptoCoin {
    pub id: String,
    pub symbol: String,
    pub price_usd: Option<f64>,
    pub change_24h: Option<f64>,
    /// Historical 24-hour prices normalized to 0..1. Empty means unavailable.
    pub sparkline: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CryptoState {
    pub status: CryptoStatus,
    /// Rows always follow configured `crypto_settings.coins` order.
    pub coins: Vec<CryptoCoin>,
    /// True when displayed values are retained from cache after a failed refresh.
    pub stale: bool,
    /// Unix timestamp in seconds for the last successful price response.
    pub updated_at: Option<u64>,
    /// IDs whose chart history is being loaded independently of their quotes.
    pub pending_ids: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct CachedData {
    timestamp_ms: u64,
    prices: HashMap<String, CachedPrice>,
    sparklines: HashMap<String, Vec<f32>>,
}

#[derive(Debug, Clone, Copy)]
struct CachedPrice {
    price_usd: Option<f64>,
    change_24h: Option<f64>,
}

/// Load quotes from cache or CoinGecko without waiting for historical charts.
/// `cache_path` should be `${Directories.state}/user/crypto_cache.json`.
pub async fn load_crypto_quotes(config: &ShellConfig, cache_path: &Path) -> CryptoState {
    load_quotes(config, cache_path, false)
}

/// Force a quote refresh; history is returned separately by `load_crypto_history`.
pub async fn refresh_crypto_quotes(config: &ShellConfig, cache_path: &Path) -> CryptoState {
    load_quotes(config, cache_path, true)
}

/// Fetch pending charts, merge successful samples into the latest cache without
/// replacing quotes, and report every requested ID as success or unavailable.
pub async fn load_crypto_history(
    cache_path: &Path,
    configured_ids: &[String],
    pending_ids: &[String],
) -> HashMap<String, Option<Vec<f32>>> {
    load_history(cache_path, configured_ids, pending_ids)
}

fn load_quotes(config: &ShellConfig, cache_path: &Path, force: bool) -> CryptoState {
    let configured_ids = &config.sidebar.crypto_coins;
    if configured_ids.is_empty() {
        return unavailable("Add coins in Widgets settings");
    }
    if !config.sidebar.widgets_crypto_enabled {
        return unavailable("Crypto widget is disabled");
    }

    let cached = read_cache(cache_path, configured_ids);
    let now_ms = now_unix_ms();
    let interval_ms = config
        .sidebar
        .crypto_refresh_interval_seconds
        .clamp(30, 300)
        .saturating_mul(1000);
    let has_prices = configured_ids.iter().any(|id| {
        cached
            .prices
            .get(id)
            .is_some_and(|price| price.price_usd.is_some())
    });
    let has_all_prices = configured_ids.iter().all(|id| {
        cached
            .prices
            .get(id)
            .is_some_and(|price| price.price_usd.is_some())
    });
    let age_ms = now_ms.saturating_sub(cached.timestamp_ms);
    if !force && has_all_prices && age_ms < interval_ms {
        return state_from_cache(configured_ids, &cached, false, Vec::new());
    }

    let request_id = NEXT_QUOTE_REQUEST.fetch_add(1, Ordering::Relaxed);
    let Some(price_payload) = fetch_prices(configured_ids) else {
        return if has_prices {
            state_from_cache(configured_ids, &cached, true, Vec::new())
        } else {
            unavailable("CoinGecko price request failed or returned invalid data")
        };
    };

    let mut quote_updates = HashMap::new();
    let mut got_any_price = false;
    for id in configured_ids {
        let Some(coin) = price_payload.get(id).and_then(Value::as_object) else {
            continue;
        };
        let price = finite_number(coin.get("usd")).filter(|value| *value >= 0.0);
        let change = finite_number(coin.get("usd_24h_change"));
        if price.is_some() {
            got_any_price = true;
        }
        if price.is_some() || change.is_some() {
            quote_updates.insert(
                id.clone(),
                CachedPrice {
                    price_usd: price,
                    change_24h: change,
                },
            );
        }
    }

    if !got_any_price {
        return if has_prices {
            state_from_cache(configured_ids, &cached, true, Vec::new())
        } else {
            unavailable("CoinGecko returned no configured coin prices")
        };
    }

    let write_lock = CACHE_WRITE_LOCK.get_or_init(|| Mutex::new(()));
    let Ok(_guard) = write_lock.lock() else {
        return unavailable("Crypto cache is temporarily unavailable");
    };
    if request_id < LAST_QUOTE_COMMIT.load(Ordering::Acquire) {
        let latest = read_cache(cache_path, configured_ids);
        return if configured_ids
            .iter()
            .any(|id| latest.prices.get(id).is_some_and(|p| p.price_usd.is_some()))
        {
            state_from_cache(configured_ids, &latest, false, Vec::new())
        } else {
            unavailable("A newer quote request superseded this result")
        };
    }
    let mut updated = read_cache(cache_path, configured_ids);
    // CoinGecko's successful simple-price response is the current price map.
    // Drop configured IDs omitted from a partial response instead of making
    // their older cached quotes look current under the new shared timestamp.
    updated.prices = quote_updates;
    updated.timestamp_ms = now_ms;
    write_cache(cache_path, &updated);
    LAST_QUOTE_COMMIT.store(request_id, Ordering::Release);
    let pending_ids = configured_ids
        .iter()
        .take(MAX_HISTORY_REQUESTS)
        .cloned()
        .collect();
    state_from_cache(configured_ids, &updated, false, pending_ids)
}

fn load_history(
    cache_path: &Path,
    configured_ids: &[String],
    pending_ids: &[String],
) -> HashMap<String, Option<Vec<f32>>> {
    let allowed = configured_ids
        .iter()
        .collect::<std::collections::HashSet<_>>();
    let ids = pending_ids
        .iter()
        .filter(|id| allowed.contains(id))
        .take(MAX_HISTORY_REQUESTS)
        .cloned()
        .collect::<Vec<_>>();
    let mut results = HashMap::new();
    for (index, id) in ids.iter().enumerate() {
        if index > 0 {
            // Respect CoinGecko's request pacing while leaving quote refresh free.
            std::thread::sleep(Duration::from_secs(1));
        }
        results.insert(id.clone(), fetch_history(id));
    }
    let successful = results
        .iter()
        .filter_map(|(id, samples)| samples.as_ref().map(|s| (id.clone(), s.clone())))
        .collect::<HashMap<_, _>>();
    if !successful.is_empty() {
        let write_lock = CACHE_WRITE_LOCK.get_or_init(|| Mutex::new(()));
        if let Ok(_guard) = write_lock.lock() {
            // Merge only chart samples into the latest cache. A history worker
            // can never roll back a quote response committed while it was running.
            let mut latest = read_cache(cache_path, configured_ids);
            latest.sparklines.extend(successful);
            write_cache(cache_path, &latest);
        }
    }
    results
}

fn unavailable(message: &str) -> CryptoState {
    CryptoState {
        status: CryptoStatus::Unavailable(message.to_owned()),
        coins: Vec::new(),
        stale: false,
        updated_at: None,
        pending_ids: Vec::new(),
    }
}

fn state_from_cache(
    ids: &[String],
    cache: &CachedData,
    stale: bool,
    pending_ids: Vec<String>,
) -> CryptoState {
    let coins = ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let price = cache.prices.get(id).copied().unwrap_or(CachedPrice {
                price_usd: None,
                change_24h: None,
            });
            let sparkline = if index < MAX_HISTORY_REQUESTS {
                cache.sparklines.get(id).cloned().unwrap_or_default()
            } else {
                Vec::new()
            };
            CryptoCoin {
                id: id.clone(),
                symbol: symbol_for(id),
                price_usd: price.price_usd,
                change_24h: price.change_24h,
                sparkline,
            }
        })
        .collect();
    CryptoState {
        status: CryptoStatus::Ready,
        coins,
        stale,
        updated_at: (cache.timestamp_ms != 0).then_some(cache.timestamp_ms / 1000),
        pending_ids,
    }
}

fn fetch_prices(ids: &[String]) -> Option<Value> {
    let joined = ids.join(",");
    if joined.len() > 4096 {
        return None;
    }
    let url = format!(
        "https://api.coingecko.com/api/v3/simple/price?ids={joined}&vs_currencies=usd&include_24hr_change=true"
    );
    let body = curl_bounded(&url)?;
    let payload: Value = serde_json::from_slice(&body).ok()?;
    payload.as_object()?;
    Some(payload)
}

fn fetch_history(id: &str) -> Option<Vec<f32>> {
    let url =
        format!("https://api.coingecko.com/api/v3/coins/{id}/market_chart?vs_currency=usd&days=1");
    let body = curl_bounded(&url)?;
    let payload: Value = serde_json::from_slice(&body).ok()?;
    let prices = payload.get("prices")?.as_array()?;
    if prices.is_empty() {
        return None;
    }
    let values = prices
        .iter()
        .filter_map(|point| point.as_array()?.get(1).and_then(Value::as_f64))
        .filter(|value| value.is_finite() && *value >= 0.0)
        .collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = (max - min).max(f64::EPSILON);
    let step = values.len().div_ceil(MAX_HISTORY_SAMPLES).max(1);
    let mut normalized = values
        .iter()
        .step_by(step)
        .map(|value| ((value - min) / range) as f32)
        .collect::<Vec<_>>();
    if normalized.len() > MAX_HISTORY_SAMPLES {
        normalized.truncate(MAX_HISTORY_SAMPLES);
    }
    Some(normalized)
}

/// Executes one request with a ten-second timeout and a hard response cap.
fn curl_bounded(url: &str) -> Option<Vec<u8>> {
    if !url.starts_with("https://api.coingecko.com/api/v3/") || url.len() > 8192 {
        return None;
    }
    let mut child = Command::new("/usr/bin/curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--max-time",
            &HTTP_TIMEOUT.as_secs().to_string(),
            "--max-filesize",
            &MAX_RESPONSE_BYTES.to_string(),
            url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let mut body = Vec::new();
    if stdout
        .by_ref()
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .is_err()
        || body.len() > MAX_RESPONSE_BYTES
    {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    let status = child.wait().ok()?;
    status.success().then_some(body)
}

fn read_cache(path: &Path, configured_ids: &[String]) -> CachedData {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return CachedData::default();
    };
    if !metadata.file_type().is_file() || metadata.len() > MAX_RESPONSE_BYTES as u64 {
        return CachedData::default();
    }
    let Ok(bytes) = fs::read(path) else {
        return CachedData::default();
    };
    let Ok(root) = serde_json::from_slice::<Value>(&bytes) else {
        return CachedData::default();
    };
    let Some(root) = root.as_object() else {
        return CachedData::default();
    };
    let Some(timestamp_ms) = root.get("timestamp").and_then(Value::as_u64) else {
        return CachedData::default();
    };
    if timestamp_ms > now_unix_ms().saturating_add(60_000) {
        return CachedData::default();
    }
    let allowed = configured_ids
        .iter()
        .collect::<std::collections::HashSet<_>>();
    let mut cache = CachedData {
        timestamp_ms,
        ..CachedData::default()
    };
    if let Some(prices) = root.get("cryptoData").and_then(Value::as_object) {
        for (id, value) in prices {
            if !allowed.contains(id) {
                continue;
            }
            let Some(price) = value.as_object() else {
                continue;
            };
            let price_usd = finite_number(price.get("usd")).filter(|value| *value >= 0.0);
            let change_24h = finite_number(price.get("usd_24h_change"));
            if price_usd.is_some() || change_24h.is_some() {
                cache.prices.insert(
                    id.clone(),
                    CachedPrice {
                        price_usd,
                        change_24h,
                    },
                );
            }
        }
    }
    if let Some(sparklines) = root.get("sparklineData").and_then(Value::as_object) {
        for (id, samples) in sparklines {
            if !allowed.contains(id) {
                continue;
            }
            if let Some(samples) = valid_samples(samples) {
                cache.sparklines.insert(id.clone(), samples);
            }
        }
    }
    cache
}

fn valid_samples(value: &Value) -> Option<Vec<f32>> {
    let values = value.as_array()?;
    if values.is_empty() || values.len() > MAX_HISTORY_SAMPLES {
        return None;
    }
    let mut samples = Vec::with_capacity(values.len());
    for value in values {
        let sample = value.as_f64()?;
        if !sample.is_finite() || !(0.0..=1.0).contains(&sample) {
            return None;
        }
        samples.push(sample as f32);
    }
    Some(samples)
}

fn write_cache(path: &Path, cache: &CachedData) {
    let Some(parent) = path.parent() else { return };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let prices = cache
        .prices
        .iter()
        .map(|(id, price)| {
            (
                id.clone(),
                json!({ "usd": price.price_usd, "usd_24h_change": price.change_24h }),
            )
        })
        .collect::<Map<_, _>>();
    let sparklines = cache
        .sparklines
        .iter()
        .map(|(id, samples)| (id.clone(), json!(samples)))
        .collect::<Map<_, _>>();
    let payload = json!({
        "cryptoData": prices,
        "sparklineData": sparklines,
        "timestamp": cache.timestamp_ms,
    });
    let Ok(bytes) = serde_json::to_vec(&payload) else {
        return;
    };
    if bytes.len() > MAX_RESPONSE_BYTES {
        return;
    }
    let temporary = path.with_extension("json.tmp");
    let Ok(mut file) = fs::File::create(&temporary) else {
        return;
    };
    if file.write_all(&bytes).is_ok() && file.sync_all().is_ok() {
        let _ = fs::rename(temporary, path);
    } else {
        let _ = fs::remove_file(temporary);
    }
}

fn finite_number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn symbol_for(id: &str) -> String {
    let known = match id {
        "bitcoin" => "BTC",
        "ethereum" => "ETH",
        "solana" => "SOL",
        "cardano" => "ADA",
        "dogecoin" => "DOGE",
        "ripple" => "XRP",
        "polkadot" => "DOT",
        "avalanche-2" => "AVAX",
        "chainlink" => "LINK",
        "polygon" => "MATIC",
        "litecoin" => "LTC",
        "uniswap" => "UNI",
        "stellar" => "XLM",
        "monero" => "XMR",
        "tron" => "TRX",
        "toncoin" => "TON",
        "shiba-inu" => "SHIB",
        "pepe" => "PEPE",
        "binancecoin" => "BNB",
        _ => return id.chars().take(4).flat_map(char::to_uppercase).collect(),
    };
    known.to_owned()
}
