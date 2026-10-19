//! Bounded, read-only gallery data for the QML QuickWallpaper widget.

use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    env, fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

const QML_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "avif", "bmp", "svg"];
const MAX_DIRECTORY_ENTRIES: usize = 8192;
const MAX_GALLERY_ITEMS: usize = 512;
const MAX_PREVIEW_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallpaperGalleryStatus {
    Loading,
    Empty,
    Unavailable,
    Ready,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WallpaperGallerySnapshot {
    pub status: WallpaperGalleryStatus,
    pub directory: PathBuf,
    /// Configured `background.wallpaperPath`; per-output focused wallpaper state is not available
    /// in this config-only adapter.
    pub current_wallpaper_path: Option<PathBuf>,
    pub items: Vec<WallpaperGalleryItem>,
    pub error: Option<String>,
    /// True when the scan or displayed item list hit its configured bound.
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WallpaperGalleryItem {
    pub path: PathBuf,
    pub file_name: String,
    pub is_current: bool,
    pub change_time_seconds: i64,
    pub size_bytes: u64,
    /// False for AVIF (the decoder feature is disabled) and files over the adapter preview byte
    /// limit. SVG is supported by GPUI's `img` element and remains previewable.
    pub preview_supported: bool,
    pub preview_unsupported_reason: Option<String>,
}

impl WallpaperGallerySnapshot {
    /// Initial value for an async host before it calls `scan_gallery`.
    pub fn loading(directory: PathBuf, current_wallpaper_path: Option<PathBuf>) -> Self {
        Self {
            status: WallpaperGalleryStatus::Loading,
            directory,
            current_wallpaper_path,
            items: Vec::new(),
            error: None,
            truncated: false,
        }
    }
}

/// Scans one directory level, includes only ordinary files with the same
/// extensions as QuickWallpaper.qml, and sorts by filesystem ctime descending.
/// It intentionally does not follow symlink directories or file symlinks.
pub fn scan_gallery(
    configured_directory: Option<&str>,
    current_wallpaper_path: Option<&str>,
) -> WallpaperGallerySnapshot {
    let directory = resolve_wallpapers_directory(configured_directory);
    let current_wallpaper_path = current_wallpaper_path
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(|path| PathBuf::from(path.strip_prefix("file://").unwrap_or(path)))
        .map(|path| fs::canonicalize(&path).unwrap_or_else(|_| absolute_path(&path)));
    let mut result = WallpaperGallerySnapshot::loading(directory.clone(), current_wallpaper_path);

    let directory_metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) => {
            result.status = WallpaperGalleryStatus::Unavailable;
            result.error = Some(format!("wallpaper directory is unavailable: {error}"));
            return result;
        }
    };
    if directory_metadata.file_type().is_symlink() || !directory_metadata.is_dir() {
        result.status = WallpaperGalleryStatus::Unavailable;
        result.error = Some("wallpaper directory must be a real directory, not a symlink".into());
        return result;
    }
    let directory = match fs::canonicalize(&directory) {
        Ok(directory) => directory,
        Err(error) => {
            result.status = WallpaperGalleryStatus::Unavailable;
            result.error = Some(format!("could not resolve wallpaper directory: {error}"));
            return result;
        }
    };
    result.directory = directory.clone();

    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) => {
            result.status = WallpaperGalleryStatus::Unavailable;
            result.error = Some(format!("could not read wallpaper directory: {error}"));
            return result;
        }
    };
    let mut newest = BinaryHeap::<Reverse<(i64, i64, PathBuf, u64)>>::new();
    let mut eligible_entries = 0usize;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                result
                    .error
                    .get_or_insert_with(|| format!("could not read a directory entry: {error}"));
                continue;
            }
        };
        let path = entry.path();
        if !has_supported_extension(&path) {
            continue;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => metadata,
            Ok(_) => continue,
            Err(error) => {
                result.error.get_or_insert_with(|| {
                    format!("could not inspect {}: {error}", path.display())
                });
                continue;
            }
        };
        eligible_entries += 1;
        let key = (
            metadata.ctime(),
            metadata.ctime_nsec(),
            path,
            metadata.len(),
        );
        if newest.len() < MAX_DIRECTORY_ENTRIES {
            newest.push(Reverse(key));
        } else if newest.peek().is_some_and(|Reverse(oldest)| key > *oldest) {
            newest.pop();
            newest.push(Reverse(key));
        }
    }
    if eligible_entries > MAX_DIRECTORY_ENTRIES {
        result.truncated = true;
    }
    let mut candidates = newest
        .into_vec()
        .into_iter()
        .filter_map(|Reverse((ctime, ctime_nsec, path, size_bytes))| {
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                return None;
            };
            let file_name = file_name.to_owned();
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let preview_unsupported_reason = if extension.eq_ignore_ascii_case("avif") {
                Some("AVIF decoding is not enabled in the GPUI image backend".to_owned())
            } else if size_bytes > MAX_PREVIEW_BYTES {
                Some(format!(
                    "image exceeds the {} MiB preview limit",
                    MAX_PREVIEW_BYTES / (1024 * 1024)
                ))
            } else {
                None
            };
            let preview_supported = preview_unsupported_reason.is_none();
            Some((
                ctime,
                ctime_nsec,
                WallpaperGalleryItem {
                    is_current: result.current_wallpaper_path.as_ref() == Some(&path),
                    path,
                    file_name,
                    change_time_seconds: ctime,
                    size_bytes,
                    preview_supported,
                    preview_unsupported_reason,
                },
            ))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        (right.0, right.1)
            .cmp(&(left.0, left.1))
            .then_with(|| left.2.path.cmp(&right.2.path))
    });
    if candidates.len() > MAX_GALLERY_ITEMS {
        candidates.truncate(MAX_GALLERY_ITEMS);
        result.truncated = true;
    }
    result.items = candidates.into_iter().map(|(_, _, item)| item).collect();
    result.status = if result.items.is_empty() {
        WallpaperGalleryStatus::Empty
    } else {
        WallpaperGalleryStatus::Ready
    };
    result
}

/// Resolves QML's `wallpapers.directory || PicturesLocation/Wallpapers` rule.
pub fn resolve_wallpapers_directory(configured_directory: Option<&str>) -> PathBuf {
    if let Some(directory) = configured_directory
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return PathBuf::from(directory);
    }
    pictures_directory().join("Wallpapers")
}

fn pictures_directory() -> PathBuf {
    let home = env::var_os("HOME").map(PathBuf::from);
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".config")));
    if let (Some(home), Some(config_home)) = (home.as_ref(), config_home) {
        if let Some(path) = parse_pictures_user_dir(&config_home.join("user-dirs.dirs"), home) {
            return path;
        }
    }
    home.map(|home| home.join("Pictures"))
        .unwrap_or_else(|| PathBuf::from("Pictures"))
}

fn parse_pictures_user_dir(user_dirs_file: &Path, home: &Path) -> Option<PathBuf> {
    let contents = fs::read_to_string(user_dirs_file).ok()?;
    let line = contents
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("XDG_PICTURES_DIR="))?;
    let value = line.split_once('=')?.1.trim().trim_matches('"');
    let value = value
        .strip_prefix("$HOME/")
        .or_else(|| value.strip_prefix("${HOME}/"))
        .map(|suffix| home.join(suffix))
        .or_else(|| Path::new(value).is_absolute().then(|| PathBuf::from(value)))?;
    Some(value)
}

fn has_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            QML_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        env::current_dir()
            .map(|current| current.join(path))
            .unwrap_or_else(|_| path.to_owned())
    }
}
