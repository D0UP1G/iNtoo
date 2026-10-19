//! XDG desktop-entry catalog for the Waffle Start Menu.
//!
//! The catalog reads only `.desktop` files in XDG application directories.
//! It launches through `gio launch` with an argument vector, never a shell.

use std::{
    collections::{BTreeMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
};

use super::super::start_menu::{MenuApp, SearchResult, SearchState};
use super::{StartMenuData, StartMenuDataSource};

#[derive(Clone, Debug)]
struct DesktopEntry {
    id: String,
    path: PathBuf,
    name: String,
    comment: String,
    categories: Vec<String>,
    icon: String,
}

#[derive(Debug)]
struct Catalog {
    entries: Vec<DesktopEntry>,
    query: String,
    selected: String,
    show_all_apps: bool,
    pinned_ids: Vec<String>,
    recent_ids: Vec<String>,
}

/// A snapshot-backed real application catalog. Pinning and recent ordering are
/// opt-in inputs because neither a pin store nor recent-history service is
/// available in the GPUI host yet.
pub struct XdgDesktopEntrySource {
    catalog: Mutex<Catalog>,
}

impl XdgDesktopEntrySource {
    /// Scan the current user's XDG application directories.
    pub fn new() -> Option<Self> {
        Self::with_app_ids(Vec::new(), Vec::new())
    }

    /// `pinned_ids` and `recent_ids` must come from actual user/service state.
    pub fn with_app_ids(pinned_ids: Vec<String>, recent_ids: Vec<String>) -> Option<Self> {
        let entries = scan_desktop_entries()?;
        Some(Self {
            catalog: Mutex::new(Catalog {
                entries,
                query: String::new(),
                selected: String::new(),
                show_all_apps: false,
                pinned_ids,
                recent_ids,
            }),
        })
    }

    /// Resolve a desktop id to the localized display name and icon metadata
    /// from the same XDG catalog used by the GPUI Waffle launcher.
    pub fn resolve_app(&self, requested_id: &str) -> Option<(String, Option<String>)> {
        let catalog = self.catalog.lock().ok()?;
        catalog
            .entries
            .iter()
            .find(|entry| id_matches(requested_id, &entry.id))
            .map(|entry| {
                (
                    entry.name.clone(),
                    (!entry.icon.is_empty()).then(|| entry.icon.clone()),
                )
            })
    }

    /// Resolve a desktop entry's actual Icon value to an existing image file.
    /// Resolve using the active desktop icon theme and its declared parents,
    /// followed by the mandatory hicolor theme and legacy pixmaps directory.
    pub fn resolve_icon_path(&self, requested_id: &str) -> Option<PathBuf> {
        self.resolve_icon_path_for_size(requested_id, 32)
    }

    /// Resolve an icon file sized for the current logical taskbar icon size.
    pub fn resolve_icon_path_for_size(
        &self,
        requested_id: &str,
        target_size: u32,
    ) -> Option<PathBuf> {
        let desktop_icon = {
            let catalog = self.catalog.lock().ok()?;
            catalog
                .entries
                .iter()
                .find(|entry| id_matches(requested_id, &entry.id))
                .map(|entry| entry.icon.trim().to_owned())
        };
        let target_size = target_size.max(1);

        // A declared desktop-entry icon is authoritative when it resolves.
        if let Some(icon) = desktop_icon.as_deref()
            && let Some(path) = resolve_icon_file(icon, target_size)
        {
            return Some(path);
        }

        // Kirigami also tries the application id as an icon-theme name when
        // desktop-entry lookup does not provide a usable icon. Keep this
        // fallback to a single safe theme-name component, and allow desktop
        // ids passed by callers to shed only their conventional suffix.
        let app_icon = app_id_icon_name(requested_id)?;
        resolve_icon_file(&app_icon, target_size)
    }

    /// Spawn GIO for a catalog entry and return the child handle so hosts can
    /// observe immediate launch rejection off the UI thread.
    pub fn spawn_app(&self, id: &str) -> Result<std::process::Child, String> {
        let path = self
            .catalog
            .lock()
            .map_err(|_| "Application catalog is unavailable".to_owned())?
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.path.clone())
            .ok_or_else(|| format!("Application {id} is no longer in the desktop catalog"))?;
        Command::new("gio")
            .arg("launch")
            .arg(path)
            .spawn()
            .map_err(|error| format!("Could not launch application: {error}"))
    }

    /// Launch a desktop entry resolved by either its catalog id or configured id.
    pub fn spawn_app_id(&self, id: &str) -> Result<std::process::Child, String> {
        let path = self
            .catalog
            .lock()
            .map_err(|_| "Application catalog is unavailable".to_owned())?
            .entries
            .iter()
            .find(|entry| id_matches(id, &entry.id))
            .map(|entry| entry.path.clone())
            .ok_or_else(|| format!("Application {id} is not in the desktop catalog"))?;
        Command::new("gio")
            .arg("launch")
            .arg(path)
            .spawn()
            .map_err(|error| format!("Could not launch application: {error}"))
    }
}

fn app_id_icon_name(app_id: &str) -> Option<String> {
    let name = app_id.strip_suffix(".desktop").unwrap_or(app_id);
    let path = Path::new(name);
    (path.components().count() == 1
        && !path.is_absolute()
        && path.file_name().and_then(|part| part.to_str()) == Some(name)
        && name != "."
        && name != ".."
        && !name.is_empty())
    .then(|| name.to_owned())
}

fn resolve_icon_file(icon: &str, target_size: u32) -> Option<PathBuf> {
    if icon.is_empty() {
        return None;
    }
    let icon = icon.strip_prefix("file://").unwrap_or(icon);
    let icon_path = Path::new(icon);
    if icon_path.is_absolute() {
        return icon_path.is_file().then(|| icon_path.to_path_buf());
    }
    // Icon theme names are single names. Do not interpret desktop metadata as
    // a relative path (or allow traversal outside the standard icon roots).
    if icon_path.components().count() != 1
        || icon_path.file_name().and_then(|name| name.to_str()) != Some(icon)
    {
        return None;
    }

    let data_dirs = icon_data_dirs();
    let extensions = if icon_path.extension().is_some() {
        vec![String::new()]
    } else {
        ["png", "svg", "xpm", "webp"]
            .into_iter()
            .map(|ext| format!(".{ext}"))
            .collect()
    };
    let themes = icon_theme_chain(&data_dirs);
    for theme in themes {
        let mut themed = Vec::new();
        for data_dir in &data_dirs {
            let theme_root = data_dir.join("icons").join(&theme);
            let dirs = read_theme_directories(&theme_root);
            for directory in dirs {
                let base = theme_root.join(&directory.path);
                for extension in &extensions {
                    let candidate = base.join(format!("{icon}{extension}"));
                    if candidate.is_file() {
                        themed.push((directory.distance_from(target_size), candidate));
                    }
                }
            }
        }
        themed.sort_by_key(|(distance, _)| *distance);
        if let Some((_, path)) = themed.into_iter().next() {
            return Some(path);
        }
    }

    // Pixmaps is outside icon themes and remains a common legacy location.
    data_dirs
        .iter()
        .flat_map(|data_dir| {
            extensions
                .iter()
                .map(|extension| data_dir.join("pixmaps").join(format!("{icon}{extension}")))
        })
        .find(|path| path.is_file())
}

#[derive(Clone, Debug)]
struct ThemeDirectory {
    path: PathBuf,
    size: u32,
    scale: u32,
    scalable: bool,
    min_size: u32,
    max_size: u32,
}

impl ThemeDirectory {
    fn distance_from(&self, requested: u32) -> u32 {
        if self.scalable {
            if requested < self.min_size {
                self.min_size - requested
            } else if requested > self.max_size {
                requested - self.max_size
            } else {
                0
            }
        } else {
            if requested < self.min_size {
                self.min_size - requested
            } else if requested > self.max_size {
                requested - self.max_size
            } else {
                // Threshold directories cover this full range, so they are
                // equivalent matches; declared index.theme order breaks ties.
                0
            }
        }
    }
}

fn icon_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    if let Some(path) = data_home {
        dirs.push(path);
    }
    dirs.extend(
        env::var_os("XDG_DATA_DIRS")
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into())
            .to_string_lossy()
            .split(':')
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from),
    );
    dirs
}

fn active_icon_theme() -> Option<String> {
    if let Some(theme) = env::var("XDG_ICON_THEME")
        .ok()
        .filter(|s| !s.trim().is_empty())
    {
        return Some(theme);
    }
    let desktop = env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if desktop
        .split(':')
        .any(|name| name.contains("gnome") || name.contains("ubuntu"))
    {
        if let Some(theme) = gnome_icon_theme() {
            return Some(theme);
        }
    }
    let home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    let home = home?;
    let kde_first = desktop
        .split(':')
        .any(|name| name.contains("kde") || name.contains("plasma"));
    let (kde_settings, gtk_settings) = (
        vec![
            home.join("kdeglobals"),
            home.join("KDE/share/config/kdeglobals"),
        ],
        vec![
            home.join("gtk-4.0/settings.ini"),
            home.join("gtk-3.0/settings.ini"),
        ],
    );
    let settings = if kde_first {
        kde_settings
            .into_iter()
            .chain(gtk_settings)
            .collect::<Vec<_>>()
    } else {
        gtk_settings
            .into_iter()
            .chain(kde_settings)
            .collect::<Vec<_>>()
    };
    for path in settings {
        if let Ok(content) = fs::read_to_string(path) {
            let theme = ini_value(
                &content,
                &["Settings", "Icons"],
                &["gtk-icon-theme-name", "Theme"],
            );
            if let Some(theme) = theme.filter(|value| !value.trim().is_empty()) {
                return Some(theme);
            }
        }
    }
    None
}

fn gnome_icon_theme() -> Option<String> {
    static THEME: OnceLock<Option<String>> = OnceLock::new();
    THEME
        .get_or_init(|| {
            let output = Command::new("gsettings")
                .args(["get", "org.gnome.desktop.interface", "icon-theme"])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let value = String::from_utf8(output.stdout).ok()?;
            let theme = value.trim().trim_matches(['\'', '"']).trim();
            (!theme.is_empty()).then(|| theme.to_owned())
        })
        .clone()
}

fn icon_theme_chain(data_dirs: &[PathBuf]) -> Vec<String> {
    let mut themes = Vec::new();
    let mut pending = active_icon_theme().into_iter().collect::<Vec<_>>();
    let mut visited = HashSet::new();
    while let Some(theme) = pending.first().cloned() {
        pending.remove(0);
        if theme.is_empty()
            || theme == "."
            || theme == ".."
            || theme.contains('/')
            || !visited.insert(theme.clone())
        {
            continue;
        }
        let roots = data_dirs
            .iter()
            .map(|dir| dir.join("icons").join(&theme))
            .collect::<Vec<_>>();
        // Theme metadata follows XDG data precedence. Lower-priority copies
        // still contribute icon files, but cannot amend the selected theme's
        // inheritance list.
        let parents = roots
            .iter()
            .find(|root| root.join("index.theme").is_file())
            .map(|root| theme_inherits(root))
            .unwrap_or_default();
        themes.push(theme);
        pending.extend(parents);
    }
    if visited.insert("hicolor".to_owned()) {
        themes.push("hicolor".to_owned());
    }
    themes
}

fn theme_inherits(theme_root: &Path) -> Vec<String> {
    let Ok(content) = fs::read_to_string(theme_root.join("index.theme")) else {
        return Vec::new();
    };
    ini_value(&content, &["Icon Theme"], &["Inherits"])
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty() && !name.contains('/') && *name != "." && *name != "..")
        .map(str::to_owned)
        .collect()
}

fn read_theme_directories(theme_root: &Path) -> Vec<ThemeDirectory> {
    let Ok(content) = fs::read_to_string(theme_root.join("index.theme")) else {
        return Vec::new();
    };
    let regular = ini_value(&content, &["Icon Theme"], &["Directories"]).unwrap_or_default();
    let scaled = ini_value(&content, &["Icon Theme"], &["ScaledDirectories"]).unwrap_or_default();
    let mut paths = regular
        .split(',')
        .chain(scaled.split(','))
        .map(str::trim)
        .filter(|path| safe_theme_subdir(path))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    paths.dedup();
    paths
        .into_iter()
        .filter_map(|path| {
            let section = path.as_str();
            let value = |keys: &[&str]| ini_value(&content, &[section], keys);
            let size: u32 = value(&["Size"]).and_then(|v| v.parse().ok()).unwrap_or(32);
            let scale: u32 = value(&["Scale"]).and_then(|v| v.parse().ok()).unwrap_or(1);
            let kind = value(&["Type"]).unwrap_or_else(|| "Threshold".to_owned());
            let threshold: u32 = value(&["Threshold"])
                .and_then(|v| v.parse().ok())
                .unwrap_or(2);
            let scalable = kind == "Scalable";
            let threshold = if kind == "Threshold" {
                threshold.saturating_mul(scale)
            } else {
                0
            };
            let min_size: u32 = value(&["MinSize"])
                .and_then(|v| v.parse().ok())
                .unwrap_or(size.saturating_sub(threshold))
                .saturating_mul(scale);
            let max_size: u32 = value(&["MaxSize"])
                .and_then(|v| v.parse().ok())
                .unwrap_or(size.saturating_add(threshold))
                .saturating_mul(scale);
            Some(ThemeDirectory {
                path: PathBuf::from(path),
                size,
                scale,
                scalable,
                min_size,
                max_size,
            })
        })
        .collect()
}

fn safe_theme_subdir(path: &str) -> bool {
    let path = Path::new(path);
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

fn ini_value(content: &str, sections: &[&str], keys: &[&str]) -> Option<String> {
    let mut section = String::new();
    for line in content.lines().map(str::trim) {
        if line.starts_with('#') || line.starts_with(';') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_owned();
            continue;
        }
        if sections.contains(&section.as_str()) {
            if let Some((key, value)) = line.split_once('=') {
                if keys.contains(&key.trim()) {
                    return Some(value.trim().to_owned());
                }
            }
        }
    }
    None
}

impl StartMenuDataSource for XdgDesktopEntrySource {
    fn snapshot(&self) -> Option<StartMenuData> {
        let catalog = self.catalog.lock().ok()?;
        let query = catalog.query.trim().to_lowercase();
        let matches = catalog
            .entries
            .iter()
            .filter(|entry| {
                query.is_empty()
                    || entry.name.to_lowercase().contains(&query)
                    || entry.comment.to_lowercase().contains(&query)
                    || entry
                        .categories
                        .iter()
                        .any(|category| category.to_lowercase().contains(&query))
            })
            .collect::<Vec<_>>();
        let results = matches
            .iter()
            .map(|entry| SearchResult {
                id: entry.id.clone(),
                name: entry.name.clone(),
                category: entry
                    .categories
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Application".to_owned()),
                detail: entry.comment.clone(),
                icon: entry.icon.clone(),
            })
            .collect::<Vec<_>>();
        let selected_result = results
            .iter()
            .position(|result| result.id == catalog.selected)
            .unwrap_or(0);

        Some(StartMenuData {
            query: catalog.query.clone(),
            results,
            search_state: SearchState::Ready,
            selected_result,
            pinned_apps: apps_for_ids(&catalog.entries, &catalog.pinned_ids),
            recent_apps: apps_for_ids(&catalog.entries, &catalog.recent_ids),
            all_apps: catalog.entries.iter().map(MenuApp::from).collect(),
            show_all_apps: catalog.show_all_apps,
        })
    }

    fn query_changed(&self, value: String) {
        if let Ok(mut catalog) = self.catalog.lock() {
            catalog.query = value;
            catalog.selected.clear();
        }
    }

    fn select_result(&self, id: String) {
        if let Ok(mut catalog) = self.catalog.lock() {
            catalog.selected = id;
        }
    }

    fn launch_app(&self, id: String) -> Result<(), String> {
        self.spawn_app(&id).map(|_| ())
    }

    fn show_all_apps(&self) {
        if let Ok(mut catalog) = self.catalog.lock() {
            catalog.show_all_apps = true;
        }
    }

    fn back(&self) {
        if let Ok(mut catalog) = self.catalog.lock() {
            catalog.show_all_apps = false;
        }
    }

    fn close(&self) {
        if let Ok(mut catalog) = self.catalog.lock() {
            catalog.query.clear();
            catalog.show_all_apps = false;
            catalog.selected.clear();
        }
    }
}

impl From<&DesktopEntry> for MenuApp {
    fn from(entry: &DesktopEntry) -> Self {
        Self {
            id: entry.id.clone(),
            name: entry.name.clone(),
            // GPUI's current menu renders this as text and has no icon theme
            // resolver. Preserve actual Icon metadata when it is a short name.
            icon: entry.icon.clone(),
        }
    }
}

fn apps_for_ids(entries: &[DesktopEntry], ids: &[String]) -> Vec<MenuApp> {
    ids.iter()
        .filter_map(|id| {
            entries
                .iter()
                .find(|entry| id_matches(id, &entry.id))
                .map(MenuApp::from)
        })
        .collect()
}

fn id_matches(requested: &str, desktop_id: &str) -> bool {
    let requested = requested
        .strip_suffix(".desktop")
        .unwrap_or(requested)
        .rsplit('/')
        .next()
        .unwrap_or(requested);
    let desktop_id = desktop_id.strip_suffix(".desktop").unwrap_or(desktop_id);
    requested.eq_ignore_ascii_case(desktop_id)
}

fn scan_desktop_entries() -> Option<Vec<DesktopEntry>> {
    let dirs = application_dirs();
    let mut seen = HashSet::new();
    let mut chosen = BTreeMap::<String, Option<DesktopEntry>>::new();
    let current_desktops = env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    let locale = env::var("LC_ALL")
        .or_else(|_| env::var("LC_MESSAGES"))
        .or_else(|_| env::var("LANG"))
        .unwrap_or_default();
    let locale = locale.split('.').next().unwrap_or(&locale);
    let mut readable_dir = false;

    // Directory order follows XDG precedence: user data first, then the
    // configured system data directories. A higher-priority Hidden entry
    // intentionally shadows a lower-priority record with the same desktop ID.
    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        readable_dir = true;
        let mut files = Vec::new();
        collect_desktop_files(&dir, &dir, &mut files);
        files.sort();
        for file in files {
            let relative = file.strip_prefix(&dir).ok()?;
            let id = relative
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "-");
            if !seen.insert(id.clone()) {
                continue;
            }
            let entry = parse_desktop_file(&id, &file, locale, &current_desktops);
            chosen.insert(id, entry);
        }
    }
    if !readable_dir {
        return None;
    }
    let mut entries = chosen.into_values().flatten().collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    Some(entries)
}

fn application_dirs() -> Vec<PathBuf> {
    let data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let data_dirs = env::var_os("XDG_DATA_DIRS")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        });
    data_home
        .into_iter()
        .map(|path| path.join("applications"))
        .chain(data_dirs.into_iter().map(|path| path.join("applications")))
        .collect()
}

fn collect_desktop_files(root: &Path, current: &Path, output: &mut Vec<PathBuf>) {
    let Ok(items) = fs::read_dir(current) else {
        return;
    };
    for item in items.flatten() {
        let path = item.path();
        let Ok(metadata) = item.file_type() else {
            continue;
        };
        if metadata.is_dir() {
            collect_desktop_files(root, &path, output);
        } else if metadata.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "desktop")
            && path.starts_with(root)
        {
            output.push(path);
        }
    }
}

fn parse_desktop_file(
    id: &str,
    path: &Path,
    locale: &str,
    current_desktops: &HashSet<String>,
) -> Option<DesktopEntry> {
    let text = fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let mut fields = BTreeMap::<String, String>::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            fields.insert(key.trim().to_owned(), unescape(value.trim()));
        }
    }

    let localized_name = localized_field(&fields, "Name", locale);
    let name = localized_name.or_else(|| fields.get("Name").cloned())?;
    let hidden = is_true(fields.get("Hidden")) || is_true(fields.get("NoDisplay"));
    if hidden || fields.get("Type").map(String::as_str) != Some("Application") {
        return None;
    }
    if !desktop_environment_allows(
        fields.get("OnlyShowIn"),
        fields.get("NotShowIn"),
        current_desktops,
    ) {
        return None;
    }
    let exec = fields.get("Exec").map(String::as_str).unwrap_or_default();
    if exec.is_empty() {
        return None;
    }
    if fields
        .get("TryExec")
        .is_some_and(|try_exec| !command_exists(try_exec))
    {
        return None;
    }

    let comment = localized_field(&fields, "Comment", locale)
        .or_else(|| fields.get("Comment").cloned())
        .unwrap_or_default();
    let categories = fields
        .get("Categories")
        .into_iter()
        .flat_map(|value| value.split(';'))
        .filter(|category| !category.is_empty())
        .map(str::to_owned)
        .collect();
    Some(DesktopEntry {
        id: id.to_owned(),
        path: path.to_path_buf(),
        name,
        comment,
        categories,
        icon: fields.get("Icon").cloned().unwrap_or_default(),
    })
}

fn localized_field(fields: &BTreeMap<String, String>, key: &str, locale: &str) -> Option<String> {
    if locale.is_empty() {
        return None;
    }
    let mut candidates = vec![locale.to_owned()];
    if let Some((language, _)) = locale.split_once('_') {
        candidates.push(language.to_owned());
    }
    candidates.dedup();
    candidates
        .into_iter()
        .find_map(|locale| fields.get(&format!("{key}[{locale}]")).cloned())
}

fn desktop_environment_allows(
    only_show_in: Option<&String>,
    not_show_in: Option<&String>,
    current_desktops: &HashSet<String>,
) -> bool {
    let list = |value: Option<&String>| -> Vec<String> {
        value
            .into_iter()
            .flat_map(|value| value.split(';'))
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let only = list(only_show_in);
    if !only.is_empty()
        && !current_desktops.is_empty()
        && !only
            .iter()
            .any(|desktop| current_desktops.contains(desktop))
    {
        return false;
    }
    !list(not_show_in)
        .iter()
        .any(|desktop| current_desktops.contains(desktop))
}

fn command_exists(command: &str) -> bool {
    let path = Path::new(command);
    if path.components().count() > 1 {
        return path.is_file();
    }
    env::var_os("PATH")
        .into_iter()
        .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .any(|dir| dir.join(command).is_file())
}

fn is_true(value: Option<&String>) -> bool {
    value.is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

fn unescape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            Some('s') => output.push(' '),
            Some('n') => output.push('\n'),
            Some('t') => output.push('\t'),
            Some('r') => output.push('\r'),
            Some('\\') => output.push('\\'),
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => output.push('\\'),
        }
    }
    output
}
