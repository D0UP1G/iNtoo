//! Pure, output-scoped policy for Iris desktop backgrounds and overview backdrop.
//!
//! This module only resolves inputs that are present in the typed config or
//! explicitly supplied by a runtime adapter. It does not select a display from
//! `bar.screen_list`, infer the focused output, or invent compositor providers.

use crate::config::{BackdropConfig, ShellConfig, WallpaperOutputConfig};

/// Which Iris desktop surface QML selects with `iris.modules.desktopWidgets`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopBackgroundMode {
    /// `Background {}`: wallpaper plus desktop widget canvas.
    WidgetCanvas,
    /// `IrisBackground.qml`: wallpaper and the bare-desktop menu only.
    WallpaperOnly,
}

/// Where an output's selected wallpaper path came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallpaperPathSource {
    PerMonitorMain,
    GlobalMain,
    PerMonitorBackdrop,
    GlobalBackdrop,
    InternalPreview,
    /// The output identity is absent. A display-scoped resolver must not guess
    /// which output is focused and borrow that output's custom wallpaper.
    MissingOutputIdentity,
}

/// A wallpaper choice keeps the requested output identity even when config has
/// no matching per-output entry. `path` is empty in the QML-equivalent case of
/// no usable configured path; the view can then render its background color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWallpaper {
    pub output_name: String,
    pub path: String,
    pub source: WallpaperPathSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallpaperMediaKind {
    /// GPUI's static wallpaper image path accepts this common raster set.
    Still,
    Gif,
    Video,
    /// No configured path; the surface should render its opaque fallback color.
    Empty,
    /// Path extension is outside the static GPUI image path's supported set.
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationRuntimeInput {
    OutputIdentity,
    InternalPreviewState,
    InternalPreviewTarget,
    InternalPreviewPath,
    AwwwOwnership,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallpaperPresentationDecision {
    RenderInternally,
    ExternalAwwwOwnsWallpaper,
    Awaiting(PresentationRuntimeInput),
}

/// Runtime values supplied by Quickshell's WallpaperListener/Wallpapers and
/// AwwwBackend. The GPUI host currently has no adapter for these providers.
/// An empty `preview_output_name` means QML's preview applies to all outputs;
/// `None` means that target information has not been read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WallpaperPresentationInputs {
    pub internal_preview_active: Option<bool>,
    pub internal_preview_path: Option<String>,
    pub internal_preview_output_name: Option<String>,
    /// Effective QML `externalWallpaper` eligibility for this output.
    pub awww_owns_wallpaper: Option<bool>,
}

/// External, per-output animation state. `None` means the GPUI host has not
/// integrated that provider yet, and is intentionally not treated as `false`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MotionRuntimeInputs {
    pub screen_locked: Option<bool>,
    pub game_mode_active: Option<bool>,
    pub battery_pause_active: Option<bool>,
    pub overview_active: Option<bool>,
    /// QML `Wallpapers.videoMotionAllowedOn(outputName)` for that same output.
    pub output_motion_allowed: Option<bool>,
    pub is_niri: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionRuntimeInput {
    ScreenLock,
    GameMode,
    BatteryPause,
    Overview,
    OutputMotionAllowance,
    NiriCompositor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionPauseReason {
    AnimationDisabled,
    ScreenLocked,
    GameMode,
    BatteryPause,
    OutputPolicy,
    NotNiri,
    OutsideOverview,
}

/// A three-way result prevents an absent host provider from silently granting
/// or denying motion in a way that would be mistaken for runtime parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionDecision {
    Allowed,
    Paused(MotionPauseReason),
    Awaiting(MotionRuntimeInput),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputBackgroundPolicy {
    /// Exact compositor output name supplied by the host, even if unmatched.
    pub output_name: String,
    pub mode: DesktopBackgroundMode,
    pub wallpaper: ResolvedWallpaper,
    pub media: WallpaperMediaKind,
    pub motion: MotionDecision,
    pub presentation: WallpaperPresentationDecision,
}

/// Inputs for the independent Iris backdrop surface in `Backdrop.qml`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BackdropGateInputs {
    pub config_ready: bool,
    pub iris_official: bool,
    pub deferred_panels_ready: bool,
    pub is_niri: bool,
}

/// Lifecycle inputs for the two Iris desktop background loaders. The widget
/// canvas is deferred; the lightweight wallpaper-only panel is critical.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DesktopBackgroundGateInputs {
    pub config_ready: bool,
    pub iris_official: bool,
    pub deferred_panels_ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackdropPolicy {
    pub output_name: String,
    pub gate_open: bool,
    pub wallpaper: ResolvedWallpaper,
    pub media: WallpaperMediaKind,
    pub motion: MotionDecision,
}

/// Mirrors the split loader conditions in `ShellIrisCriticalPanels.qml` and
/// `ShellIrisPanelsImpl.qml` while keeping the policy independent of bar output
/// selection. Wallpaper-only is critical; widget canvas is deferred.
pub fn desktop_background_gate_open(
    config: &ShellConfig,
    inputs: DesktopBackgroundGateInputs,
) -> bool {
    let panel_enabled = config
        .enabled_panels
        .iter()
        .any(|panel| panel == "irisBackground");
    inputs.config_ready
        && inputs.iris_official
        && panel_enabled
        && (!config.iris_background.desktop_widgets_enabled || inputs.deferred_panels_ready)
}

/// Resolve the selected Iris desktop policy for one concrete output.
///
/// `output_name` comes from the output being hosted, not Iris bar configuration.
/// When it is missing, no arbitrary/focused output's wallpaper is substituted.
pub fn resolve_output_background(
    config: &ShellConfig,
    output_name: &str,
    runtime: MotionRuntimeInputs,
) -> OutputBackgroundPolicy {
    resolve_output_background_with_presentation(
        config,
        output_name,
        runtime,
        &WallpaperPresentationInputs::default(),
    )
}

/// Resolve output wallpaper and motion policy with explicit preview/Awww
/// ownership inputs. `Awaiting` is retained when the host lacks a provider.
pub fn resolve_output_background_with_presentation(
    config: &ShellConfig,
    output_name: &str,
    runtime: MotionRuntimeInputs,
    presentation_inputs: &WallpaperPresentationInputs,
) -> OutputBackgroundPolicy {
    let wallpaper = resolve_main_wallpaper(config, output_name);
    let (wallpaper, presentation) = resolve_wallpaper_presentation(wallpaper, presentation_inputs);
    let media = classify_media(&wallpaper.path);
    let animation_enabled = config.background.enable_animation.unwrap_or(true);
    let motion = resolve_desktop_motion(animation_enabled, runtime);
    OutputBackgroundPolicy {
        output_name: output_name.to_owned(),
        mode: if config.iris_background.desktop_widgets_enabled {
            DesktopBackgroundMode::WidgetCanvas
        } else {
            DesktopBackgroundMode::WallpaperOnly
        },
        wallpaper,
        media,
        motion,
        presentation,
    }
}

fn resolve_wallpaper_presentation(
    mut wallpaper: ResolvedWallpaper,
    inputs: &WallpaperPresentationInputs,
) -> (ResolvedWallpaper, WallpaperPresentationDecision) {
    if wallpaper.output_name.is_empty() {
        return (
            wallpaper,
            WallpaperPresentationDecision::Awaiting(PresentationRuntimeInput::OutputIdentity),
        );
    }
    let Some(preview_active) = inputs.internal_preview_active else {
        return (
            wallpaper,
            WallpaperPresentationDecision::Awaiting(PresentationRuntimeInput::InternalPreviewState),
        );
    };
    if preview_active {
        let Some(preview_output) = inputs.internal_preview_output_name.as_deref() else {
            return (
                wallpaper,
                WallpaperPresentationDecision::Awaiting(
                    PresentationRuntimeInput::InternalPreviewTarget,
                ),
            );
        };
        if preview_output.is_empty() || preview_output == wallpaper.output_name {
            let Some(preview_path) = inputs.internal_preview_path.as_ref() else {
                return (
                    wallpaper,
                    WallpaperPresentationDecision::Awaiting(
                        PresentationRuntimeInput::InternalPreviewPath,
                    ),
                );
            };
            wallpaper.path = preview_path.clone();
            wallpaper.source = WallpaperPathSource::InternalPreview;
        }
        // QML disables Awww ownership globally while any internal preview is active.
        return (wallpaper, WallpaperPresentationDecision::RenderInternally);
    }
    match inputs.awww_owns_wallpaper {
        Some(true) => (
            wallpaper,
            WallpaperPresentationDecision::ExternalAwwwOwnsWallpaper,
        ),
        Some(false) => (wallpaper, WallpaperPresentationDecision::RenderInternally),
        None => (
            wallpaper,
            WallpaperPresentationDecision::Awaiting(PresentationRuntimeInput::AwwwOwnership),
        ),
    }
}

/// QML `Wallpapers.currentMainWallpaperPath(output)` and `Background.qml` main
/// path selection: per-output path when multi-monitor is on and non-empty,
/// otherwise the global main wallpaper. A missing output name remains unresolved
/// rather than borrowing the compositor-focused monitor's override.
pub fn resolve_main_wallpaper(config: &ShellConfig, output_name: &str) -> ResolvedWallpaper {
    if output_name.is_empty() {
        return ResolvedWallpaper {
            output_name: String::new(),
            path: String::new(),
            source: WallpaperPathSource::MissingOutputIdentity,
        };
    }

    if config.background.multi_monitor_enabled.unwrap_or(false) {
        if let Some(path) =
            per_monitor_path(&config.background.wallpapers_by_monitor, output_name, false)
        {
            return ResolvedWallpaper {
                output_name: output_name.to_owned(),
                path,
                source: WallpaperPathSource::PerMonitorMain,
            };
        }
    }

    ResolvedWallpaper {
        output_name: output_name.to_owned(),
        path: config.background.wallpaper_path.clone().unwrap_or_default(),
        source: WallpaperPathSource::GlobalMain,
    }
}

/// QML `Backdrop.qml` policy: main wallpaper by default; if configured to use
/// a custom backdrop, choose per-output backdrop, then global backdrop, then
/// main wallpaper. Main wallpaper itself follows the per-output/global rule.
pub fn resolve_backdrop_wallpaper(config: &ShellConfig, output_name: &str) -> ResolvedWallpaper {
    let backdrop = &config.background.backdrop;
    let main = resolve_main_wallpaper(config, output_name);
    if backdrop.use_main_wallpaper.unwrap_or(true) {
        return main;
    }

    if !output_name.is_empty()
        && config.background.multi_monitor_enabled.unwrap_or(false)
        && let Some(path) =
            per_monitor_path(&config.background.wallpapers_by_monitor, output_name, true)
    {
        return ResolvedWallpaper {
            output_name: output_name.to_owned(),
            path,
            source: WallpaperPathSource::PerMonitorBackdrop,
        };
    }

    if let Some(path) = non_empty(backdrop.wallpaper_path.as_deref()) {
        return ResolvedWallpaper {
            output_name: output_name.to_owned(),
            path: path.to_owned(),
            source: WallpaperPathSource::GlobalBackdrop,
        };
    }

    main
}

/// Full QML gate from `ShellIrisPanelsImpl.qml` for `Backdrop.qml`.
pub fn backdrop_gate_open(inputs: BackdropGateInputs, backdrop: &BackdropConfig) -> bool {
    inputs.config_ready
        && inputs.iris_official
        && inputs.deferred_panels_ready
        && inputs.is_niri
        && backdrop.enabled.unwrap_or(false)
}

/// Resolve the separate full-output backdrop from the typed config and its
/// compositor lifecycle inputs. Wallpaper choice remains display-scoped even
/// when the gate is closed, which lets hosts prepare a stable policy without
/// accidentally selecting another output's wallpaper.
pub fn resolve_backdrop_policy(
    config: &ShellConfig,
    output_name: &str,
    gate: BackdropGateInputs,
    runtime: MotionRuntimeInputs,
) -> BackdropPolicy {
    let wallpaper = resolve_backdrop_wallpaper(config, output_name);
    let media = classify_media(&wallpaper.path);
    let motion = resolve_backdrop_motion(
        config.background.backdrop.enable_animation.unwrap_or(false),
        runtime,
    );
    BackdropPolicy {
        output_name: output_name.to_owned(),
        gate_open: backdrop_gate_open(gate, &config.background.backdrop),
        wallpaper,
        media,
        motion,
    }
}

/// Desktop wallpaper movement gate from `IrisBackground.qml`.
/// Overview is deliberately not consulted by this QML surface.
pub fn resolve_desktop_motion(
    animation_enabled: bool,
    runtime: MotionRuntimeInputs,
) -> MotionDecision {
    if !animation_enabled {
        return MotionDecision::Paused(MotionPauseReason::AnimationDisabled);
    }
    match runtime.screen_locked {
        Some(true) => return MotionDecision::Paused(MotionPauseReason::ScreenLocked),
        Some(false) => {}
        None => return MotionDecision::Awaiting(MotionRuntimeInput::ScreenLock),
    }
    match runtime.game_mode_active {
        Some(true) => return MotionDecision::Paused(MotionPauseReason::GameMode),
        Some(false) => {}
        None => return MotionDecision::Awaiting(MotionRuntimeInput::GameMode),
    }
    match runtime.battery_pause_active {
        Some(true) => return MotionDecision::Paused(MotionPauseReason::BatteryPause),
        Some(false) => {}
        None => return MotionDecision::Awaiting(MotionRuntimeInput::BatteryPause),
    }
    match runtime.output_motion_allowed {
        Some(true) => MotionDecision::Allowed,
        Some(false) => MotionDecision::Paused(MotionPauseReason::OutputPolicy),
        None => MotionDecision::Awaiting(MotionRuntimeInput::OutputMotionAllowance),
    }
}

/// Backdrop animation gate from `Backdrop.qml`: on Niri it animates only while
/// the overview is open. This is separate from desktop video-pause policy.
pub fn resolve_backdrop_motion(
    animation_enabled: bool,
    runtime: MotionRuntimeInputs,
) -> MotionDecision {
    if !animation_enabled {
        return MotionDecision::Paused(MotionPauseReason::AnimationDisabled);
    }
    match runtime.screen_locked {
        Some(true) => return MotionDecision::Paused(MotionPauseReason::ScreenLocked),
        Some(false) => {}
        None => return MotionDecision::Awaiting(MotionRuntimeInput::ScreenLock),
    }
    match runtime.game_mode_active {
        Some(true) => return MotionDecision::Paused(MotionPauseReason::GameMode),
        Some(false) => {}
        None => return MotionDecision::Awaiting(MotionRuntimeInput::GameMode),
    }
    match runtime.battery_pause_active {
        Some(true) => return MotionDecision::Paused(MotionPauseReason::BatteryPause),
        Some(false) => {}
        None => return MotionDecision::Awaiting(MotionRuntimeInput::BatteryPause),
    }
    match runtime.is_niri {
        Some(false) => return MotionDecision::Paused(MotionPauseReason::NotNiri),
        Some(true) => {}
        None => return MotionDecision::Awaiting(MotionRuntimeInput::NiriCompositor),
    }
    match runtime.overview_active {
        Some(true) => MotionDecision::Allowed,
        Some(false) => MotionDecision::Paused(MotionPauseReason::OutsideOverview),
        None => MotionDecision::Awaiting(MotionRuntimeInput::Overview),
    }
}

pub fn classify_media(path: &str) -> WallpaperMediaKind {
    if path.is_empty() {
        return WallpaperMediaKind::Empty;
    }
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".gif") {
        WallpaperMediaKind::Gif
    } else if [".mp4", ".webm", ".mkv", ".avi", ".mov"]
        .iter()
        .any(|extension| lower.ends_with(extension))
    {
        WallpaperMediaKind::Video
    } else if [".png", ".jpg", ".jpeg", ".webp", ".bmp"]
        .iter()
        .any(|extension| lower.ends_with(extension))
    {
        WallpaperMediaKind::Still
    } else {
        WallpaperMediaKind::Unsupported
    }
}

fn per_monitor_path(
    entries: &[WallpaperOutputConfig],
    output_name: &str,
    backdrop: bool,
) -> Option<String> {
    // WallpaperListener builds a JS object by assignment, so the last duplicate
    // monitor entry wins. Empty strings are false in QML and fall through.
    entries
        .iter()
        .rev()
        .find(|entry| entry.monitor.as_deref() == Some(output_name))
        .and_then(|entry| {
            non_empty(if backdrop {
                entry.backdrop_path.as_deref()
            } else {
                entry.path.as_deref()
            })
        })
        .map(str::to_owned)
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WallpaperOutputConfig;

    fn config() -> ShellConfig {
        ShellConfig::default()
    }

    fn motion_inputs() -> MotionRuntimeInputs {
        MotionRuntimeInputs {
            screen_locked: Some(false),
            game_mode_active: Some(false),
            battery_pause_active: Some(false),
            overview_active: Some(false),
            output_motion_allowed: Some(true),
            is_niri: Some(true),
        }
    }

    #[test]
    fn desktop_modes_are_selected_from_iris_desktop_widgets_setting() {
        let mut config = config();
        assert_eq!(
            resolve_output_background(&config, "DP-1", motion_inputs()).mode,
            DesktopBackgroundMode::WidgetCanvas
        );
        config.iris_background.desktop_widgets_enabled = false;
        let policy = resolve_output_background(&config, "DP-1", motion_inputs());
        assert_eq!(policy.mode, DesktopBackgroundMode::WallpaperOnly);
        assert_eq!(policy.output_name, "DP-1");
    }

    #[test]
    fn desktop_surface_gate_matches_critical_and_deferred_qml_loaders() {
        let mut config = config();
        config.enabled_panels = vec!["irisBackground".into()];
        let ready = DesktopBackgroundGateInputs {
            config_ready: true,
            iris_official: true,
            deferred_panels_ready: true,
        };
        assert!(desktop_background_gate_open(&config, ready));
        assert!(!desktop_background_gate_open(
            &config,
            DesktopBackgroundGateInputs {
                deferred_panels_ready: false,
                ..ready
            }
        ));
        config.iris_background.desktop_widgets_enabled = false;
        assert!(desktop_background_gate_open(
            &config,
            DesktopBackgroundGateInputs {
                deferred_panels_ready: false,
                ..ready
            }
        ));
        config.enabled_panels.clear();
        assert!(!desktop_background_gate_open(&config, ready));
        config.enabled_panels = vec!["irisBackground".into()];
        for inputs in [
            DesktopBackgroundGateInputs {
                config_ready: false,
                ..ready
            },
            DesktopBackgroundGateInputs {
                iris_official: false,
                ..ready
            },
        ] {
            assert!(!desktop_background_gate_open(&config, inputs));
        }
    }

    #[test]
    fn path_resolution_uses_matching_output_then_global_main_without_bar_list() {
        let mut config = config();
        config.background.multi_monitor_enabled = Some(true);
        config.background.wallpaper_path = Some("/global.jpg".into());
        config.background.wallpapers_by_monitor = vec![WallpaperOutputConfig {
            monitor: Some("DP-1".into()),
            path: Some("/dp1.gif".into()),
            backdrop_path: None,
        }];
        let first = resolve_main_wallpaper(&config, "DP-1");
        assert_eq!(first.path, "/dp1.gif");
        assert_eq!(first.source, WallpaperPathSource::PerMonitorMain);
        assert_eq!(first.output_name, "DP-1");

        let unmatched = resolve_main_wallpaper(&config, "HDMI-A-1");
        assert_eq!(unmatched.path, "/global.jpg");
        assert_eq!(unmatched.source, WallpaperPathSource::GlobalMain);
        assert_eq!(unmatched.output_name, "HDMI-A-1");
    }

    #[test]
    fn disabled_multi_monitor_ignores_output_override_and_duplicate_uses_last() {
        let mut config = config();
        config.background.wallpaper_path = Some("/global.jpg".into());
        config.background.wallpapers_by_monitor = vec![
            WallpaperOutputConfig {
                monitor: Some("DP-1".into()),
                path: Some("/first.jpg".into()),
                backdrop_path: None,
            },
            WallpaperOutputConfig {
                monitor: Some("DP-1".into()),
                path: Some("/last.jpg".into()),
                backdrop_path: None,
            },
        ];
        assert_eq!(resolve_main_wallpaper(&config, "DP-1").path, "/global.jpg");
        config.background.multi_monitor_enabled = Some(true);
        assert_eq!(resolve_main_wallpaper(&config, "DP-1").path, "/last.jpg");
    }

    #[test]
    fn missing_output_identity_is_not_replaced_with_arbitrary_output_path() {
        let mut config = config();
        config.background.multi_monitor_enabled = Some(true);
        config.background.wallpaper_path = Some("/global.jpg".into());
        config.background.wallpapers_by_monitor = vec![WallpaperOutputConfig {
            monitor: Some("DP-1".into()),
            path: Some("/dp1.jpg".into()),
            backdrop_path: None,
        }];
        let unresolved = resolve_main_wallpaper(&config, "");
        assert_eq!(unresolved.output_name, "");
        assert_eq!(unresolved.path, "");
        assert_eq!(
            unresolved.source,
            WallpaperPathSource::MissingOutputIdentity
        );
    }

    #[test]
    fn backdrop_path_priority_matches_qml_main_then_per_monitor_global_main() {
        let mut config = config();
        config.background.multi_monitor_enabled = Some(true);
        config.background.wallpaper_path = Some("/global-main.jpg".into());
        config.background.wallpapers_by_monitor = vec![WallpaperOutputConfig {
            monitor: Some("DP-1".into()),
            path: Some("/per-monitor-main.jpg".into()),
            backdrop_path: Some("/per-monitor-backdrop.jpg".into()),
        }];
        config.background.backdrop.enabled = Some(true);
        config.background.backdrop.use_main_wallpaper = Some(true);
        config.background.backdrop.wallpaper_path = Some("/global-backdrop.jpg".into());
        let main = resolve_backdrop_wallpaper(&config, "DP-1");
        assert_eq!(main.path, "/per-monitor-main.jpg");
        assert_eq!(main.source, WallpaperPathSource::PerMonitorMain);

        config.background.backdrop.use_main_wallpaper = Some(false);
        let per_monitor = resolve_backdrop_wallpaper(&config, "DP-1");
        assert_eq!(per_monitor.path, "/per-monitor-backdrop.jpg");
        assert_eq!(per_monitor.source, WallpaperPathSource::PerMonitorBackdrop);

        let global_backdrop = resolve_backdrop_wallpaper(&config, "HDMI-A-1");
        assert_eq!(global_backdrop.path, "/global-backdrop.jpg");
        assert_eq!(global_backdrop.source, WallpaperPathSource::GlobalBackdrop);

        config.background.backdrop.wallpaper_path = Some(String::new());
        let fallback = resolve_backdrop_wallpaper(&config, "HDMI-A-1");
        assert_eq!(fallback.path, "/global-main.jpg");
        assert_eq!(fallback.source, WallpaperPathSource::GlobalMain);
    }

    #[test]
    fn backdrop_gate_requires_every_qml_condition() {
        let backdrop = BackdropConfig {
            enabled: Some(true),
            ..BackdropConfig::default()
        };
        let all_ready = BackdropGateInputs {
            config_ready: true,
            iris_official: true,
            deferred_panels_ready: true,
            is_niri: true,
        };
        assert!(backdrop_gate_open(all_ready, &backdrop));
        for inputs in [
            BackdropGateInputs {
                config_ready: false,
                ..all_ready
            },
            BackdropGateInputs {
                iris_official: false,
                ..all_ready
            },
            BackdropGateInputs {
                deferred_panels_ready: false,
                ..all_ready
            },
            BackdropGateInputs {
                is_niri: false,
                ..all_ready
            },
        ] {
            assert!(!backdrop_gate_open(inputs, &backdrop));
        }
        assert!(!backdrop_gate_open(all_ready, &BackdropConfig::default()));
    }

    #[test]
    fn backdrop_policy_keeps_output_and_uses_backdrop_animation_default() {
        let config = config();
        let policy = resolve_backdrop_policy(
            &config,
            "HDMI-A-1",
            BackdropGateInputs {
                config_ready: true,
                iris_official: true,
                deferred_panels_ready: true,
                is_niri: true,
            },
            motion_inputs(),
        );
        assert_eq!(policy.output_name, "HDMI-A-1");
        assert!(!policy.gate_open);
        assert_eq!(policy.media, WallpaperMediaKind::Empty);
        assert_eq!(
            policy.motion,
            MotionDecision::Paused(MotionPauseReason::AnimationDisabled)
        );
    }

    #[test]
    fn media_classification_matches_qml_video_and_gif_suffixes() {
        assert_eq!(classify_media("/wall.JPG"), WallpaperMediaKind::Still);
        assert_eq!(classify_media("/wall.GiF"), WallpaperMediaKind::Gif);
        for path in [
            "/wall.mp4",
            "/wall.WEBM",
            "/wall.mkv",
            "/wall.avi",
            "/wall.mov",
        ] {
            assert_eq!(classify_media(path), WallpaperMediaKind::Video, "{path}");
        }
        assert_eq!(classify_media(""), WallpaperMediaKind::Empty);
        assert_eq!(classify_media("/wall.svg"), WallpaperMediaKind::Unsupported);
    }

    #[test]
    fn presentation_waits_for_output_preview_and_awww_providers() {
        let mut config = config();
        config.background.wallpaper_path = Some("/global.jpg".into());
        let pending = resolve_output_background(&config, "DP-1", motion_inputs());
        assert_eq!(
            pending.presentation,
            WallpaperPresentationDecision::Awaiting(PresentationRuntimeInput::InternalPreviewState)
        );
        assert_eq!(pending.wallpaper.path, "/global.jpg");

        let preview_inactive = WallpaperPresentationInputs {
            internal_preview_active: Some(false),
            ..WallpaperPresentationInputs::default()
        };
        let still_pending = resolve_output_background_with_presentation(
            &config,
            "DP-1",
            motion_inputs(),
            &preview_inactive,
        );
        assert_eq!(
            still_pending.presentation,
            WallpaperPresentationDecision::Awaiting(PresentationRuntimeInput::AwwwOwnership)
        );

        let awww = WallpaperPresentationInputs {
            awww_owns_wallpaper: Some(true),
            ..preview_inactive.clone()
        };
        assert_eq!(
            resolve_output_background_with_presentation(&config, "DP-1", motion_inputs(), &awww)
                .presentation,
            WallpaperPresentationDecision::ExternalAwwwOwnsWallpaper
        );
        let gpui_owner = WallpaperPresentationInputs {
            awww_owns_wallpaper: Some(false),
            ..preview_inactive
        };
        assert_eq!(
            resolve_output_background_with_presentation(
                &config,
                "DP-1",
                motion_inputs(),
                &gpui_owner
            )
            .presentation,
            WallpaperPresentationDecision::RenderInternally
        );
    }

    #[test]
    fn active_preview_uses_only_matching_or_global_target_and_disables_awww() {
        let mut config = config();
        config.background.wallpaper_path = Some("/configured.jpg".into());
        let preview = WallpaperPresentationInputs {
            internal_preview_active: Some(true),
            internal_preview_path: Some("/preview.gif".into()),
            internal_preview_output_name: Some("DP-1".into()),
            awww_owns_wallpaper: None,
        };
        let matching =
            resolve_output_background_with_presentation(&config, "DP-1", motion_inputs(), &preview);
        assert_eq!(matching.wallpaper.path, "/preview.gif");
        assert_eq!(
            matching.wallpaper.source,
            WallpaperPathSource::InternalPreview
        );
        assert_eq!(matching.media, WallpaperMediaKind::Gif);
        assert_eq!(
            matching.presentation,
            WallpaperPresentationDecision::RenderInternally
        );

        let other_output = resolve_output_background_with_presentation(
            &config,
            "HDMI-A-1",
            motion_inputs(),
            &preview,
        );
        assert_eq!(other_output.wallpaper.path, "/configured.jpg");
        assert_eq!(
            other_output.presentation,
            WallpaperPresentationDecision::RenderInternally
        );

        let global_preview = WallpaperPresentationInputs {
            internal_preview_output_name: Some(String::new()),
            ..preview
        };
        assert_eq!(
            resolve_output_background_with_presentation(
                &config,
                "HDMI-A-1",
                motion_inputs(),
                &global_preview
            )
            .wallpaper
            .path,
            "/preview.gif"
        );
    }

    #[test]
    fn desktop_motion_has_lock_game_battery_output_gates_and_awaits_missing_provider() {
        let mut input = motion_inputs();
        assert_eq!(resolve_desktop_motion(true, input), MotionDecision::Allowed);
        input.screen_locked = Some(true);
        assert_eq!(
            resolve_desktop_motion(true, input),
            MotionDecision::Paused(MotionPauseReason::ScreenLocked)
        );
        input.screen_locked = Some(false);
        input.game_mode_active = Some(true);
        assert_eq!(
            resolve_desktop_motion(true, input),
            MotionDecision::Paused(MotionPauseReason::GameMode)
        );
        input.game_mode_active = Some(false);
        input.battery_pause_active = Some(true);
        assert_eq!(
            resolve_desktop_motion(true, input),
            MotionDecision::Paused(MotionPauseReason::BatteryPause)
        );
        input.battery_pause_active = Some(false);
        input.output_motion_allowed = Some(false);
        assert_eq!(
            resolve_desktop_motion(true, input),
            MotionDecision::Paused(MotionPauseReason::OutputPolicy)
        );
        input.output_motion_allowed = None;
        assert_eq!(
            resolve_desktop_motion(true, input),
            MotionDecision::Awaiting(MotionRuntimeInput::OutputMotionAllowance)
        );
        input.output_motion_allowed = Some(true);
        input.overview_active = Some(false);
        assert_eq!(resolve_desktop_motion(true, input), MotionDecision::Allowed);
        assert_eq!(
            resolve_desktop_motion(false, input),
            MotionDecision::Paused(MotionPauseReason::AnimationDisabled)
        );
    }

    #[test]
    fn backdrop_motion_requires_overview_and_niri_after_common_safety_gates() {
        let mut input = motion_inputs();
        assert_eq!(
            resolve_backdrop_motion(true, input),
            MotionDecision::Paused(MotionPauseReason::OutsideOverview)
        );
        input.overview_active = Some(true);
        assert_eq!(
            resolve_backdrop_motion(true, input),
            MotionDecision::Allowed
        );
        input.screen_locked = Some(true);
        assert_eq!(
            resolve_backdrop_motion(true, input),
            MotionDecision::Paused(MotionPauseReason::ScreenLocked)
        );
        input.screen_locked = Some(false);
        input.game_mode_active = Some(true);
        assert_eq!(
            resolve_backdrop_motion(true, input),
            MotionDecision::Paused(MotionPauseReason::GameMode)
        );
        input.game_mode_active = Some(false);
        input.battery_pause_active = Some(true);
        assert_eq!(
            resolve_backdrop_motion(true, input),
            MotionDecision::Paused(MotionPauseReason::BatteryPause)
        );
        input.battery_pause_active = Some(false);
        input.is_niri = Some(false);
        assert_eq!(
            resolve_backdrop_motion(true, input),
            MotionDecision::Paused(MotionPauseReason::NotNiri)
        );
        input.is_niri = Some(true);
        input.overview_active = None;
        assert_eq!(
            resolve_backdrop_motion(true, input),
            MotionDecision::Awaiting(MotionRuntimeInput::Overview)
        );
    }
}
