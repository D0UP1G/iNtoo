//! Read-only adapter for the window screenshot cache shared with QML.
//!
//! This module never starts captures and never removes cache entries. A host
//! that has requested a capture can pass that fact to [`DockPreviewCache::state`]
//! to distinguish an in-flight capture from a missing preview.

use std::{
    env,
    fs::File,
    io::Read,
    path::PathBuf,
    time::{Duration, SystemTime},
};

const PREVIEW_SUBDIR: &str = "inir/window-previews";
const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
pub const DEFAULT_MAX_AGE: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewFreshness {
    Fresh,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnavailableReason {
    CacheDirectoryUnavailable,
    Missing,
    InvalidPng,
    Unreadable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewState {
    /// A capture was requested by the caller and has not produced a file yet.
    Loading,
    Available {
        path: PathBuf,
        captured_at: SystemTime,
        age: Duration,
        freshness: PreviewFreshness,
    },
    Unavailable(UnavailableReason),
}

#[derive(Debug, Clone)]
pub struct DockPreviewCache {
    directory: Option<PathBuf>,
}

impl Default for DockPreviewCache {
    fn default() -> Self {
        Self::new()
    }
}

impl DockPreviewCache {
    /// Resolve the same cache location used by `WindowPreviewService.qml`:
    /// `$XDG_CACHE_HOME/inir/window-previews`, falling back to `~/.cache`.
    pub fn new() -> Self {
        Self {
            directory: cache_home().map(|home| home.join(PREVIEW_SUBDIR)),
        }
    }

    pub fn with_directory(directory: PathBuf) -> Self {
        Self {
            directory: Some(directory),
        }
    }

    pub fn directory(&self) -> Option<&std::path::Path> {
        self.directory.as_deref()
    }

    /// Inspect one cached preview without changing the filesystem.
    ///
    /// `capture_pending` and `dirty` are supplied by the owner of capture
    /// lifecycle state. A dirty entry remains available but is marked stale.
    pub fn state(
        &self,
        window_id: u64,
        capture_pending: bool,
        dirty: bool,
        max_age: Duration,
        now: SystemTime,
    ) -> PreviewState {
        let Some(directory) = &self.directory else {
            return if capture_pending {
                PreviewState::Loading
            } else {
                PreviewState::Unavailable(UnavailableReason::CacheDirectoryUnavailable)
            };
        };

        let path = directory.join(format!("window-{window_id}.png"));
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() && metadata.len() > PNG_SIGNATURE.len() as u64 => {
                metadata
            }
            Ok(_) => {
                return if capture_pending {
                    PreviewState::Loading
                } else {
                    PreviewState::Unavailable(UnavailableReason::InvalidPng)
                };
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return if capture_pending {
                    PreviewState::Loading
                } else {
                    PreviewState::Unavailable(UnavailableReason::Missing)
                };
            }
            Err(_) => {
                return if capture_pending {
                    PreviewState::Loading
                } else {
                    PreviewState::Unavailable(UnavailableReason::Unreadable)
                };
            }
        };

        if !has_png_structure(&path, metadata.len()) {
            return if capture_pending {
                PreviewState::Loading
            } else {
                PreviewState::Unavailable(UnavailableReason::InvalidPng)
            };
        }

        let captured_at = match metadata.modified() {
            Ok(modified) => modified,
            Err(_) => {
                return if capture_pending {
                    PreviewState::Loading
                } else {
                    PreviewState::Unavailable(UnavailableReason::Unreadable)
                };
            }
        };
        let age = now.duration_since(captured_at).unwrap_or(Duration::ZERO);
        let freshness = if dirty || age > max_age {
            PreviewFreshness::Stale
        } else {
            PreviewFreshness::Fresh
        };

        PreviewState::Available {
            path,
            captured_at,
            age,
            freshness,
        }
    }

    pub fn default_state(&self, window_id: u64, now: SystemTime) -> PreviewState {
        self.state(window_id, false, false, DEFAULT_MAX_AGE, now)
    }
}

fn cache_home() -> Option<PathBuf> {
    if let Some(value) = env::var_os("XDG_CACHE_HOME") {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            return Some(path);
        }
    }

    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .map(|home| home.join(".cache"))
}

/// Check PNG chunk framing and header integrity without decoding pixel data.
/// Metadata supplies a bound so malformed chunk lengths cannot make us read
/// outside the cached file or allocate based on untrusted input.
fn has_png_structure(path: &std::path::Path, file_len: u64) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut signature = [0; PNG_SIGNATURE.len()];
    if file.read_exact(&mut signature).is_err() || signature != PNG_SIGNATURE {
        return false;
    }

    let Some(mut remaining) = file_len.checked_sub(PNG_SIGNATURE.len() as u64) else {
        return false;
    };
    let mut first_chunk = true;
    let mut has_image_data = false;
    let mut image_data_ended = false;
    let mut buffer = [0; 8192];

    loop {
        // Every chunk needs a length, a type, and a CRC.
        if remaining < 12 {
            return false;
        }

        let mut header = [0; 8];
        if file.read_exact(&mut header).is_err() {
            return false;
        }
        let data_len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        let chunk_type = &header[4..8];
        if !valid_chunk_type(chunk_type) {
            return false;
        }
        let chunk_and_crc = u64::from(data_len) + 4;
        if chunk_and_crc > remaining - 8 {
            return false;
        }

        if first_chunk {
            if chunk_type != b"IHDR" || data_len != 13 {
                return false;
            }
            first_chunk = false;
        } else if chunk_type == b"IHDR" {
            return false;
        }
        if chunk_type == b"IDAT" {
            if image_data_ended {
                return false;
            }
            has_image_data = true;
        } else if has_image_data {
            image_data_ended = true;
        }

        let mut crc = crc32_update(u32::MAX, chunk_type);
        let mut ihdr = [0; 13];
        let mut read_total = 0_u64;
        while read_total < u64::from(data_len) {
            let amount =
                usize::try_from((u64::from(data_len) - read_total).min(buffer.len() as u64))
                    .unwrap();
            if file.read_exact(&mut buffer[..amount]).is_err() {
                return false;
            }
            crc = crc32_update(crc, &buffer[..amount]);
            if chunk_type == b"IHDR" {
                let start = read_total as usize;
                ihdr[start..start + amount].copy_from_slice(&buffer[..amount]);
            }
            read_total += amount as u64;
        }

        let mut crc_bytes = [0; 4];
        if file.read_exact(&mut crc_bytes).is_err() {
            return false;
        }
        if u32::from_be_bytes(crc_bytes) != !crc {
            return false;
        }

        if chunk_type == b"IHDR" && !valid_ihdr(&ihdr) {
            return false;
        }
        remaining -= 8 + chunk_and_crc;

        if chunk_type == b"IEND" {
            if data_len != 0 || !has_image_data || remaining != 0 {
                return false;
            }
            // Confirm that the file did not grow after metadata was read.
            let mut trailing = [0];
            return file.read(&mut trailing).is_ok_and(|read| read == 0);
        }
    }
}

fn valid_chunk_type(chunk_type: &[u8]) -> bool {
    chunk_type.len() == 4
        && chunk_type
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_lowercase())
        // The third character's reserved bit must be zero (uppercase).
        && chunk_type[2].is_ascii_uppercase()
}

fn valid_ihdr(ihdr: &[u8; 13]) -> bool {
    let width = u32::from_be_bytes([ihdr[0], ihdr[1], ihdr[2], ihdr[3]]);
    let height = u32::from_be_bytes([ihdr[4], ihdr[5], ihdr[6], ihdr[7]]);
    let bit_depth = ihdr[8];
    let color_type = ihdr[9];
    let valid_depth = match color_type {
        0 => matches!(bit_depth, 1 | 2 | 4 | 8 | 16),
        2 | 4 | 6 => matches!(bit_depth, 8 | 16),
        3 => matches!(bit_depth, 1 | 2 | 4 | 8),
        _ => false,
    };

    width != 0
        && height != 0
        && valid_depth
        && ihdr[10] == 0 // Compression method
        && ihdr[11] == 0 // Filter method
        && ihdr[12] <= 1 // Interlace method
}

fn crc32_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc
}
