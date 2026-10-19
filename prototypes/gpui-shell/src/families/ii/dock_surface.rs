//! Output-scoped host adapter for the ii Dock.

use gpui::popup::{PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupOptions};
use gpui::{
    AnyWindowHandle, Bounds, DisplayId, Entity, IntoElement, Render, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, div, point,
    prelude::*, px, rgba, size,
};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::{
    config::ShellConfig,
    families::{
        ii::dock::{
            self, DockApp, DockDragState, DockDrop, DockEdge, DockGeometry, DockInput, DockStyle,
            DockTask,
        },
        ii::dock_preview::{
            DockPreviewEdge, DockPreviewInput, DockPreviewStyle, DockPreviewWindow, ThumbnailState,
        },
        ii::dock_preview_cache::{
            DockPreviewCache, PreviewFreshness, PreviewState, UnavailableReason,
        },
        ii::dock_preview_host::{DockPreviewHost, EscapeAction, PointerAction},
        waffle::taskbar::start_menu_adapter::desktop_entries::XdgDesktopEntrySource,
    },
    niri,
    theme::ThemeTokens,
};

pub struct IiDockSurface {
    output: Option<String>,
    display_id: DisplayId,
    width: f32,
    height: f32,
    config: ShellConfig,
    theme: ThemeTokens,
    snapshot: niri::Snapshot,
    running_app_order: Vec<String>,
    last_focused: Arc<Mutex<std::collections::HashMap<String, usize>>>,
    catalog: Option<Arc<XdgDesktopEntrySource>>,
    preview_generation: u64,
    preview_app: Option<String>,
    app_hovered: bool,
    dock_hovered: bool,
    popup_hovered: bool,
    escape_latched: bool,
    preview_anchor: Option<(AnyWindowHandle, Bounds<gpui::Pixels>)>,
    preview_popup: Option<WindowHandle<DockPreviewHost>>,
    preview_delay: std::time::Duration,
    close_grace_until: Arc<Mutex<Option<std::time::Instant>>>,
    drag_state: Rc<RefCell<DockDragState>>,
}

static DOCK_CONFIG_WRITE_LOCK: Mutex<()> = Mutex::new(());
static DOCK_CONFIG_WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PinnedWriteOutcome {
    Saved,
    NoChange,
    Superseded,
}

fn begin_pinned_write() -> u64 {
    DOCK_CONFIG_WRITE_SEQUENCE
        .fetch_add(1, Ordering::SeqCst)
        .wrapping_add(1)
        .max(1)
}

fn is_latest_pinned_write(sequence: u64) -> bool {
    DOCK_CONFIG_WRITE_SEQUENCE.load(Ordering::SeqCst) == sequence
}

fn persist_pinned_drop(drop: &DockDrop, sequence: u64) -> Result<PinnedWriteOutcome, String> {
    if !drop.target.pinned {
        return Ok(PinnedWriteOutcome::NoChange);
    }
    let _write_guard = DOCK_CONFIG_WRITE_LOCK
        .lock()
        .map_err(|_| "dock config write lock poisoned".to_owned())?;
    if !is_latest_pinned_write(sequence) {
        return Ok(PinnedWriteOutcome::Superseded);
    }
    let path = crate::config::runtime_path();
    let parent = path
        .parent()
        .ok_or_else(|| "runtime config has no parent directory".to_owned())?;
    for attempt in 0..4 {
        if !is_latest_pinned_write(sequence) {
            return Ok(PinnedWriteOutcome::Superseded);
        }
        let before =
            std::fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
        let mut config: serde_json::Value = serde_json::from_slice(&before)
            .map_err(|error| format!("parse {}: {error}", path.display()))?;
        let Some(dock) = config
            .get_mut("dock")
            .and_then(serde_json::Value::as_object_mut)
        else {
            return Err("config dock section is missing or not an object".to_owned());
        };
        let mut pinned_apps = dock
            .get("pinnedApps")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let original_pinned_apps = pinned_apps.clone();
        let source = &drop.source.id;
        let target = &drop.target.id;
        if drop.source.pinned {
            let Some(source_position) = pinned_apps
                .iter()
                .position(|app| app.eq_ignore_ascii_case(source))
            else {
                return Ok(if is_latest_pinned_write(sequence) {
                    PinnedWriteOutcome::NoChange
                } else {
                    PinnedWriteOutcome::Superseded
                });
            };
            let Some(target_position) = pinned_apps
                .iter()
                .position(|app| app.eq_ignore_ascii_case(target))
            else {
                return Ok(if is_latest_pinned_write(sequence) {
                    PinnedWriteOutcome::NoChange
                } else {
                    PinnedWriteOutcome::Superseded
                });
            };
            if source_position != target_position {
                let moved = pinned_apps.remove(source_position);
                let insertion = if drop.source.display_index < drop.target.display_index {
                    pinned_apps
                        .iter()
                        .position(|app| app.eq_ignore_ascii_case(target))
                        .map(|index| index + 1)
                        .unwrap_or(pinned_apps.len())
                } else {
                    pinned_apps
                        .iter()
                        .position(|app| app.eq_ignore_ascii_case(target))
                        .unwrap_or(0)
                };
                pinned_apps.insert(insertion, moved);
            }
        } else {
            if pinned_apps
                .iter()
                .any(|app| app.eq_ignore_ascii_case(source))
            {
                return Ok(if is_latest_pinned_write(sequence) {
                    PinnedWriteOutcome::NoChange
                } else {
                    PinnedWriteOutcome::Superseded
                });
            }
            let Some(target_position) = pinned_apps
                .iter()
                .position(|app| app.eq_ignore_ascii_case(target))
            else {
                return Ok(if is_latest_pinned_write(sequence) {
                    PinnedWriteOutcome::NoChange
                } else {
                    PinnedWriteOutcome::Superseded
                });
            };
            let insertion = if drop.source.display_index < drop.target.display_index {
                target_position + 1
            } else {
                target_position
            };
            pinned_apps.insert(insertion, source.to_owned());
        }
        if pinned_apps == original_pinned_apps {
            return Ok(if is_latest_pinned_write(sequence) {
                PinnedWriteOutcome::NoChange
            } else {
                PinnedWriteOutcome::Superseded
            });
        }
        dock.insert(
            "pinnedApps".to_owned(),
            serde_json::Value::Array(
                pinned_apps
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
        let serialized = serde_json::to_vec_pretty(&config)
            .map_err(|error| format!("serialize config: {error}"))?;

        // QML Config.setNestedValue rewrites the same JSON file without a shared
        // lock. Compare the bytes before replacing and retry against a fresh
        // merge if another writer was observed. A final compare→rename race
        // remains because the QML process cannot participate in this lock.
        let current =
            std::fs::read(&path).map_err(|error| format!("recheck {}: {error}", path.display()))?;
        if current != before {
            continue;
        }
        if !is_latest_pinned_write(sequence) {
            return Ok(PinnedWriteOutcome::Superseded);
        }
        let temporary = parent.join(format!(
            ".config.json.ii-dock-{}-{attempt}.tmp",
            std::process::id()
        ));
        let permissions = std::fs::metadata(&path)
            .ok()
            .map(|metadata| metadata.permissions());
        let write_result = (|| -> std::io::Result<()> {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&temporary)?;
            if let Some(permissions) = permissions {
                std::fs::set_permissions(&temporary, permissions)?;
            }
            file.write_all(&serialized)?;
            file.sync_all()?;
            std::fs::rename(&temporary, &path)
        })();
        if let Err(error) = write_result {
            let _ = std::fs::remove_file(&temporary);
            return Err(format!("write {}: {error}", path.display()));
        }
        let written =
            std::fs::read(&path).map_err(|error| format!("verify {}: {error}", path.display()))?;
        let observed: serde_json::Value = serde_json::from_slice(&written)
            .map_err(|error| format!("verify config {}: {error}", path.display()))?;
        if observed == config {
            return Ok(if is_latest_pinned_write(sequence) {
                PinnedWriteOutcome::Saved
            } else {
                PinnedWriteOutcome::Superseded
            });
        }
    }
    Err("dock pinned-app update conflicted with concurrent config writes; retry later".to_owned())
}

fn announce_dock_status(
    state: Rc<RefCell<DockDragState>>,
    surface: Entity<IiDockSurface>,
    message: String,
    cx: &mut gpui::App,
) -> u64 {
    let generation = state.borrow_mut().announce_completion(message);
    let _ = surface.update(cx, |_, cx| cx.notify());
    schedule_dock_status_clear(state, surface, generation, cx);
    generation
}

fn announce_dock_status_if_current(
    state: Rc<RefCell<DockDragState>>,
    surface: Entity<IiDockSurface>,
    expected_generation: u64,
    message: String,
    cx: &mut gpui::App,
) {
    let Some(generation) = state
        .borrow_mut()
        .replace_completion_if_current(expected_generation, message)
    else {
        return;
    };
    let _ = surface.update(cx, |_, cx| cx.notify());
    schedule_dock_status_clear(state, surface, generation, cx);
}

fn schedule_dock_status_clear(
    state: Rc<RefCell<DockDragState>>,
    surface: Entity<IiDockSurface>,
    generation: u64,
    cx: &mut gpui::App,
) {
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(std::time::Duration::from_secs(4))
            .await;
        cx.update(|cx| {
            if state.borrow_mut().clear_completion(generation) {
                let _ = surface.update(cx, |_, cx| cx.notify());
            }
        });
    })
    .detach();
}

impl IiDockSurface {
    fn handle_hover(
        &mut self,
        app: Option<String>,
        entered: bool,
        parent: AnyWindowHandle,
        anchor: Bounds<gpui::Pixels>,
        enabled: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let drag_active = self.drag_state.borrow().is_active();
        if drag_active {
            self.preview_generation = self.preview_generation.wrapping_add(1);
            self.app_hovered = false;
            self.popup_hovered = false;
            self.close_preview(cx);
            if app.is_none() {
                self.dock_hovered = entered;
            }
            return;
        }

        if let Some(app_id) = app {
            if entered {
                if !enabled || self.app_windows(&app_id).is_empty() || self.escape_latched {
                    return;
                }
                if self
                    .preview_app
                    .as_deref()
                    .is_some_and(|current| !current.eq_ignore_ascii_case(&app_id))
                {
                    self.close_preview(cx);
                }
                self.app_hovered = true;
                self.dock_hovered = true;
                self.preview_generation = self.preview_generation.wrapping_add(1);
                self.preview_app = Some(app_id.clone());
                self.preview_anchor = Some((parent, anchor));
                let generation = self.preview_generation;
                let delay = self.preview_delay;
                let entity = cx.entity();
                cx.spawn(async move |_, cx| {
                    cx.background_executor().timer(delay).await;
                    cx.update(|cx| {
                        let _ = entity.update(cx, |view, cx| view.open_preview(generation, cx));
                    });
                })
                .detach();
            } else {
                self.app_hovered = false;
                self.schedule_preview_close(cx);
            }
            return;
        }

        let was_hovered = self.dock_hovered;
        self.dock_hovered = entered;
        if !entered {
            self.escape_latched = false;
            if was_hovered {
                self.schedule_preview_close(cx);
            }
        } else {
            if !self.app_hovered {
                self.preview_generation = self.preview_generation.wrapping_add(1);
            }
        }
    }

    fn popup_hover_changed(&mut self, entered: bool, cx: &mut gpui::Context<Self>) {
        if self.drag_state.borrow().is_active() {
            self.popup_hovered = false;
            self.close_preview(cx);
            return;
        }
        self.popup_hovered = entered;
        if entered {
            self.preview_generation = self.preview_generation.wrapping_add(1);
        } else {
            self.schedule_preview_close(cx);
        }
    }

    fn schedule_preview_close(&mut self, cx: &mut gpui::Context<Self>) {
        if self.app_hovered || self.popup_hovered {
            return;
        }
        self.preview_generation = self.preview_generation.wrapping_add(1);
        let generation = self.preview_generation;
        let mut delay = std::time::Duration::from_millis(250);
        if let Ok(grace) = self.close_grace_until.lock() {
            if let Some(until) = *grace {
                delay = delay.max(
                    until.saturating_duration_since(std::time::Instant::now())
                        + std::time::Duration::from_millis(250),
                );
            }
        }
        let entity = cx.entity();
        cx.spawn(async move |_, cx| {
            cx.background_executor().timer(delay).await;
            cx.update(|cx| {
                let _ = entity.update(cx, |view, cx| {
                    if view.preview_generation == generation
                        && !view.app_hovered
                        && !view.popup_hovered
                    {
                        view.close_preview(cx);
                    }
                });
            });
        })
        .detach();
    }

    fn close_preview(&mut self, cx: &mut gpui::Context<Self>) {
        self.preview_app = None;
        self.preview_anchor = None;
        self.popup_hovered = false;
        if let Some(popup) = self.preview_popup.take() {
            let _ = popup.update(cx, |_, window, _| window.remove_window());
        }
    }

    fn open_preview(&mut self, generation: u64, cx: &mut gpui::Context<Self>) {
        if self.drag_state.borrow().is_active() {
            self.close_preview(cx);
            return;
        }
        if generation != self.preview_generation || self.escape_latched {
            return;
        }
        let Some(app_id) = self.preview_app.clone() else {
            return;
        };
        let Some((parent, anchor_rect)) = self.preview_anchor else {
            return;
        };
        let windows = self.app_windows(&app_id);
        if windows.is_empty() {
            self.close_preview(cx);
            return;
        }
        self.close_preview(cx);
        self.preview_app = Some(app_id.clone());
        let mut input = self.preview_input(&app_id, windows);
        let keep_preview_on_click = self
            .config
            .appearance
            .source_sections
            .get("ii.dock")
            .and_then(|options| options.get("keepPreviewOnClick"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if let Some(focus) = input.on_focus.take() {
            let focus_entity = cx.entity();
            input.on_focus = Some(Rc::new(move |id, cx| {
                focus(id, cx);
                if !keep_preview_on_click {
                    let _ = focus_entity.update(cx, |view, cx| view.close_preview(cx));
                }
            }));
        }
        let popup_width = if matches!(input.edge, DockPreviewEdge::Top | DockPreviewEdge::Bottom) {
            (input.windows.len() as f32 * 244.0 + 16.0).min(self.width.max(260.0))
        } else {
            260.0_f32.min(self.width.max(260.0))
        };
        let popup_height = if matches!(input.edge, DockPreviewEdge::Left | DockPreviewEdge::Right) {
            (input.windows.len() as f32 * 180.0 + 16.0).min(self.height.max(200.0))
        } else {
            180.0_f32.min(self.height.max(200.0))
        };
        let (anchor, gravity) = match input.edge {
            DockPreviewEdge::Bottom => (PopupAnchor::Bottom, PopupGravity::Top),
            DockPreviewEdge::Top => (PopupAnchor::Top, PopupGravity::Bottom),
            DockPreviewEdge::Left => (PopupAnchor::Left, PopupGravity::Right),
            DockPreviewEdge::Right => (PopupAnchor::Right, PopupGravity::Left),
        };
        let entity = cx.entity();
        let hover_entity = entity.clone();
        let on_hover: PointerAction = Rc::new(move |entered, cx| {
            let _ = hover_entity.update(cx, |view, cx| view.popup_hover_changed(entered, cx));
        });
        let escape_entity = entity.clone();
        let on_escape: EscapeAction = Rc::new(move |cx| {
            let _ = escape_entity.update(cx, |view, cx| {
                view.escape_latched = true;
                view.preview_generation = view.preview_generation.wrapping_add(1);
                view.close_preview(cx);
            });
        });
        let options = WindowOptions {
            titlebar: None,
            display_id: Some(self.display_id),
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(popup_width), px(popup_height)),
            })),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::AnchoredPopup(PopupOptions {
                parent,
                anchor_rect,
                anchor,
                gravity,
                constraint_adjustment: PopupConstraintAdjustment::SLIDE_X
                    | PopupConstraintAdjustment::SLIDE_Y
                    | PopupConstraintAdjustment::FLIP_X
                    | PopupConstraintAdjustment::FLIP_Y,
                offset: point(px(0.0), px(0.0)),
                grab: false,
            }),
            ..Default::default()
        };
        match cx.open_window(options, move |_, cx| {
            cx.new(|_| DockPreviewHost {
                input,
                on_hover,
                on_escape,
            })
        }) {
            Ok(handle) => self.preview_popup = Some(handle),
            Err(error) => eprintln!("ii Dock preview popup: {error:#}"),
        }
    }

    fn app_windows(&self, app_id: &str) -> Vec<niri::Window> {
        let mut windows = self
            .windows_for_output()
            .into_iter()
            .filter(|window| {
                window
                    .app_id
                    .as_deref()
                    .is_some_and(|id| id.eq_ignore_ascii_case(app_id))
            })
            .collect::<Vec<_>>();
        windows.sort_by_key(|window| {
            let [column, row] = window.pos_in_scrolling_layout.unwrap_or([u64::MAX; 2]);
            (column, row, window.id)
        });
        windows
    }

    fn preview_input(&self, app_id: &str, windows: Vec<niri::Window>) -> DockPreviewInput {
        let edge = match self
            .config
            .appearance
            .source_sections
            .get("ii.dock")
            .and_then(|options| options.get("position"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("bottom")
        {
            "top" => DockPreviewEdge::Top,
            "left" => DockPreviewEdge::Left,
            "right" => DockPreviewEdge::Right,
            _ => DockPreviewEdge::Bottom,
        };
        let app_name = self
            .catalog
            .as_ref()
            .and_then(|catalog| catalog.resolve_app(app_id))
            .map(|entry| entry.0)
            .unwrap_or_else(|| app_id.to_owned());
        let cache = DockPreviewCache::new();
        let now = std::time::SystemTime::now();
        let previews = windows
            .into_iter()
            .map(|window| {
                let state = cache.default_state(window.id, now);
                let (thumbnail_path, thumbnail_state) = match state {
                    PreviewState::Available {
                        path,
                        freshness: PreviewFreshness::Fresh,
                        ..
                    } => (Some(path), ThumbnailState::Ready),
                    PreviewState::Available {
                        path,
                        freshness: PreviewFreshness::Stale,
                        ..
                    } => (Some(path), ThumbnailState::Stale),
                    PreviewState::Loading => (None, ThumbnailState::Loading),
                    PreviewState::Unavailable(
                        UnavailableReason::Missing | UnavailableReason::CacheDirectoryUnavailable,
                    ) => (None, ThumbnailState::Unavailable),
                    PreviewState::Unavailable(_) => (None, ThumbnailState::Error),
                };
                let icon_path = resolve_dock_icon_path(self.catalog.as_deref(), app_id, 24);
                DockPreviewWindow {
                    id: window.id,
                    app_name: app_name.clone(),
                    title: window.title.unwrap_or_default(),
                    icon_path,
                    thumbnail_path,
                    thumbnail_state,
                }
            })
            .collect::<Vec<_>>();
        let style = DockPreviewStyle {
            surface: self
                .theme
                .colors
                .surface
                .unwrap_or_else(|| rgba(0x20242eee).into()),
            card: self
                .theme
                .colors
                .surface_elevated
                .unwrap_or_else(|| rgba(0x343944dd).into()),
            hover: self
                .theme
                .colors
                .selection
                .unwrap_or_else(|| rgba(0x555d70ee).into()),
            foreground: self
                .theme
                .colors
                .foreground
                .unwrap_or_else(|| rgba(0xffffffff).into()),
            muted: self
                .theme
                .colors
                .foreground_muted
                .unwrap_or_else(|| rgba(0xb0b7c3ff).into()),
            accent: self
                .theme
                .colors
                .accent
                .unwrap_or_else(|| rgba(0x8ab4f8ff).into()),
            border: self
                .theme
                .colors
                .border
                .unwrap_or_else(|| rgba(0x687080aa).into()),
            radius: self.theme.shape.large,
            gap: self.theme.spacing.sm,
        };
        let grace = self.close_grace_until.clone();
        let on_focus = Rc::new(|id: u64, _cx: &mut gpui::App| {
            let _ = niri::focus_window(id);
        });
        let on_close = Rc::new(move |id: u64, _cx: &mut gpui::App| {
            let _ = niri::close_window(id);
            if let Ok(mut grace) = grace.lock() {
                *grace = Some(std::time::Instant::now() + std::time::Duration::from_millis(500));
            }
        });
        DockPreviewInput {
            edge,
            style,
            windows: previews,
            on_focus: Some(on_focus),
            on_close: Some(on_close),
        }
    }

    pub fn new(
        output: Option<String>,
        display_id: DisplayId,
        size: gpui::Size<gpui::Pixels>,
        config: ShellConfig,
        theme: ThemeTokens,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let view = Self {
            output: output.clone(),
            display_id,
            width: f32::from(size.width),
            height: f32::from(size.height),
            config,
            theme,
            snapshot: niri::Snapshot::default(),
            running_app_order: Vec::new(),
            last_focused: Arc::new(Mutex::new(std::collections::HashMap::new())),
            catalog: XdgDesktopEntrySource::new().map(Arc::new),
            preview_generation: 0,
            preview_app: None,
            app_hovered: false,
            dock_hovered: false,
            popup_hovered: false,
            escape_latched: false,
            preview_anchor: None,
            preview_popup: None,
            preview_delay: std::time::Duration::from_millis(400),
            close_grace_until: Arc::new(Mutex::new(None)),
            drag_state: Rc::new(RefCell::new(DockDragState::default())),
        };
        cx.spawn(async move |this, cx| {
            loop {
                let output = output.clone();
                let snapshot = cx
                    .background_executor()
                    .spawn(async move { niri::snapshot(output.as_deref()) })
                    .await;
                if this
                    .update(cx, |view, cx| {
                        view.snapshot = snapshot;
                        view.refresh_running_app_order();
                        if let Some(app_id) = view.preview_app.clone() {
                            let grace_active = view
                                .close_grace_until
                                .lock()
                                .ok()
                                .and_then(|grace| *grace)
                                .is_some_and(|until| until > std::time::Instant::now());
                            if view.app_windows(&app_id).is_empty() && !grace_active {
                                view.close_preview(cx);
                            }
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(2))
                    .await;
            }
        })
        .detach();
        view
    }

    pub fn resize(&mut self, size: gpui::Size<gpui::Pixels>, cx: &mut gpui::Context<Self>) {
        self.width = f32::from(size.width);
        self.height = f32::from(size.height);
        cx.notify();
    }
}

impl Render for IiDockSurface {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(options) = self.config.appearance.source_sections.get("ii.dock") else {
            return div().size_full().into_any_element();
        };
        let enabled = options
            .get("enable")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if !enabled {
            return div().size_full().into_any_element();
        }
        let previews_enabled = options
            .get("hoverPreview")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        self.preview_delay = std::time::Duration::from_millis(
            options
                .get("hoverPreviewDelay")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(400)
                .clamp(0, 10_000),
        );
        let configured_edge = options
            .get("position")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("bottom");
        let edge = match configured_edge {
            "top" => DockEdge::Top,
            "left" => DockEdge::Left,
            "right" => DockEdge::Right,
            _ => DockEdge::Bottom,
        };
        let size = options
            .get("iconSize")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(35.0) as f32;
        let separate_pinned_from_running = options
            .get("separatePinnedFromRunning")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        let pinned = options
            .get("pinnedApps")
            .and_then(serde_json::Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut grouped = std::collections::BTreeMap::<String, DockApp>::new();
        let mut app_order = Vec::<String>::new();
        let scoped_windows = self.windows_for_output();
        let mut ordered_windows = scoped_windows.clone();
        ordered_windows.sort_by_key(|window| {
            let [column, row] = window.pos_in_scrolling_layout.unwrap_or([u64::MAX; 2]);
            (column, row, window.id)
        });
        for window in &ordered_windows {
            let Some(id) = window.app_id.as_deref().filter(|id| !id.trim().is_empty()) else {
                continue;
            };
            let name = self
                .catalog
                .as_ref()
                .and_then(|catalog| catalog.resolve_app(id))
                .map(|v| v.0)
                .unwrap_or_else(|| id.to_owned());
            let icon_path =
                resolve_dock_icon_path(self.catalog.as_deref(), id, size.ceil().max(1.0) as u32);
            let key = id.to_lowercase();
            if !grouped.contains_key(&key) {
                app_order.push(key.clone());
            }
            let app = grouped.entry(key).or_insert_with(|| DockApp {
                id: id.to_owned(),
                name,
                glyph: None,
                icon_path,
                pinned: pinned.iter().any(|item| item.eq_ignore_ascii_case(id)),
                tasks: Vec::new(),
            });
            app.tasks.push(DockTask {
                id: window.id,
                title: window.title.clone().unwrap_or_default(),
                focused: window.focused,
                urgent: window.urgent,
            });
        }
        app_order.sort_by_key(|id| {
            self.running_app_order
                .iter()
                .position(|running_id| running_id == id)
                .unwrap_or(usize::MAX)
        });
        let windows = ordered_windows;
        let catalog = self.catalog.clone();
        let last_focused = self.last_focused.clone();
        let surface = cx.entity();
        let activation_surface = surface.clone();
        let activate_app = Rc::new(move |id: String, cx: &mut gpui::App| {
            let _ = activation_surface.update(cx, |view, cx| {
                view.app_hovered = false;
                view.preview_generation = view.preview_generation.wrapping_add(1);
                view.close_preview(cx);
            });
            let app_windows = windows
                .iter()
                .filter(|window| {
                    window
                        .app_id
                        .as_deref()
                        .is_some_and(|app| app.eq_ignore_ascii_case(&id))
                })
                .collect::<Vec<_>>();
            if app_windows.is_empty() {
                if let Some(catalog) = &catalog {
                    if let Err(error) = catalog.spawn_app_id(&id) {
                        eprintln!("ii Dock: {error}");
                    }
                }
                return;
            }
            let app_key = id.to_lowercase();
            let current_focused = app_windows.iter().position(|window| window.focused);
            let next_index = current_focused
                .map(|index| (index + 1) % app_windows.len())
                .or_else(|| {
                    last_focused
                        .lock()
                        .ok()
                        .and_then(|positions| positions.get(&app_key).copied())
                        .map(|index| (index + 1) % app_windows.len())
                })
                .unwrap_or(0);
            if let Ok(mut positions) = last_focused.lock() {
                positions.insert(app_key, next_index);
            }
            if !niri::focus_window(app_windows[next_index].id) {
                eprintln!(
                    "ii Dock: could not focus window {}",
                    app_windows[next_index].id
                );
            }
        });
        let style = DockStyle {
            surface: self
                .theme
                .colors
                .surface
                .unwrap_or_else(|| rgba(0x20242eee).into()),
            item: self
                .theme
                .colors
                .surface_elevated
                .unwrap_or_else(|| rgba(0x343944dd).into()),
            item_hover: self
                .theme
                .colors
                .selection
                .unwrap_or_else(|| rgba(0x555d70ee).into()),
            foreground: self
                .theme
                .colors
                .foreground
                .unwrap_or_else(|| rgba(0xffffffff).into()),
            muted: self
                .theme
                .colors
                .foreground_muted
                .unwrap_or_else(|| rgba(0xb0b7c3ff).into()),
            accent: self
                .theme
                .colors
                .accent
                .unwrap_or_else(|| rgba(0x8ab4f8ff).into()),
            border: self
                .theme
                .colors
                .border
                .unwrap_or_else(|| rgba(0x687080aa).into()),
            radius: self.theme.shape.large,
            spacing: self.theme.spacing.sm,
        };
        let apps = {
            let mut apps = Vec::new();
            let mut remaining = grouped;
            if separate_pinned_from_running {
                for pinned_id in &pinned {
                    let key = pinned_id.to_lowercase();
                    if remaining.contains_key(&key) {
                        continue;
                    }
                    if apps
                        .iter()
                        .any(|app: &DockApp| app.id.eq_ignore_ascii_case(pinned_id))
                    {
                        continue;
                    }
                    let Some((name, _)) = self
                        .catalog
                        .as_ref()
                        .and_then(|catalog| catalog.resolve_app(pinned_id))
                    else {
                        continue;
                    };
                    let icon_path = resolve_dock_icon_path(
                        self.catalog.as_deref(),
                        pinned_id,
                        size.ceil().max(1.0) as u32,
                    );
                    apps.push(DockApp {
                        id: pinned_id.clone(),
                        name,
                        glyph: None,
                        icon_path,
                        pinned: true,
                        tasks: Vec::new(),
                    });
                }
                // Separate mode places every running app after pin-only entries
                // in its stable first-open order, whether pinned or not.
                for key in app_order {
                    if let Some(app) = remaining.remove(&key) {
                        apps.push(app);
                    }
                }
            } else {
                for pinned_id in &pinned {
                    if apps
                        .iter()
                        .any(|app: &DockApp| app.id.eq_ignore_ascii_case(pinned_id))
                    {
                        continue;
                    }
                    let key = pinned_id.to_lowercase();
                    if let Some(app) = remaining.remove(&key) {
                        apps.push(app);
                        continue;
                    }
                    let Some((name, _)) = self
                        .catalog
                        .as_ref()
                        .and_then(|catalog| catalog.resolve_app(pinned_id))
                    else {
                        continue;
                    };
                    let icon_path = resolve_dock_icon_path(
                        self.catalog.as_deref(),
                        pinned_id,
                        size.ceil().max(1.0) as u32,
                    );
                    apps.push(DockApp {
                        id: pinned_id.clone(),
                        name,
                        glyph: None,
                        icon_path,
                        pinned: true,
                        tasks: Vec::new(),
                    });
                }
                for key in app_order {
                    if let Some(app) = remaining.remove(&key) {
                        apps.push(app);
                    }
                }
            }
            apps
        };
        let slot = size.clamp(24.0, 72.0) + 12.0;
        let dock_length = apps.iter().map(|_| slot).sum::<f32>()
            + self.theme.spacing.sm * apps.len().saturating_sub(1) as f32
            + if separate_pinned_from_running
                && apps.iter().any(|app| app.pinned && app.tasks.is_empty())
                && apps.iter().any(|app| !app.tasks.is_empty())
                || !separate_pinned_from_running
                    && apps.iter().any(|app| app.pinned)
                    && apps.iter().any(|app| !app.pinned && !app.tasks.is_empty())
            {
                18.0
            } else {
                0.0
            }
            + 16.0;
        let dock_thickness = slot + 16.0;
        let (x, y, width, height) = match edge {
            DockEdge::Bottom => (
                (self.width - dock_length).max(0.0) / 2.0,
                (self.height - dock_thickness).max(0.0),
                dock_length.min(self.width),
                dock_thickness.min(self.height),
            ),
            DockEdge::Top => (
                (self.width - dock_length).max(0.0) / 2.0,
                0.0,
                dock_length.min(self.width),
                dock_thickness.min(self.height),
            ),
            DockEdge::Left => (
                0.0,
                (self.height - dock_length).max(0.0) / 2.0,
                dock_thickness.min(self.width),
                dock_length.min(self.height),
            ),
            DockEdge::Right => (
                (self.width - dock_thickness).max(0.0),
                (self.height - dock_length).max(0.0) / 2.0,
                dock_thickness.min(self.width),
                dock_length.min(self.height),
            ),
        };
        let input_region = [Bounds {
            origin: point(px(x), px(y)),
            size: gpui::size(px(width), px(height)),
        }];
        window.set_input_region(Some(&input_region));
        let hover_surface = surface.clone();
        let hover = Rc::new(
            move |app: Option<String>, entered: bool, window: &mut Window, cx: &mut gpui::App| {
                let point = window.mouse_position();
                let anchor = Bounds {
                    origin: point,
                    size: gpui::size(px(1.0), px(1.0)),
                };
                let parent = window.window_handle();
                let _ = hover_surface.update(cx, |view, cx| {
                    view.handle_hover(app, entered, parent, anchor, previews_enabled, cx);
                });
            },
        );
        let drag_reorder_enabled = options
            .get("enableDragReorder")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        let reorder_surface = surface.clone();
        let reorder_status_surface = surface.clone();
        let reorder_status_state = self.drag_state.clone();
        let reorder = Rc::new(move |drop: DockDrop, cx: &mut gpui::App| {
            if drop.source.id.eq_ignore_ascii_case(&drop.target.id) {
                announce_dock_status(
                    reorder_status_state.clone(),
                    reorder_status_surface.clone(),
                    "Dock order did not change.".to_owned(),
                    cx,
                );
                return;
            }
            let running_order_branch = drop.source.running
                && drop.target.running
                && (separate_pinned_from_running || (!drop.source.pinned && !drop.target.pinned));
            if running_order_branch {
                let reordered = reorder_surface.update(cx, |view, cx| {
                    let source_id = drop.source.id.to_lowercase();
                    let target_id = drop.target.id.to_lowercase();
                    let source_index = view
                        .running_app_order
                        .iter()
                        .position(|id| id == &source_id);
                    let target_index = view
                        .running_app_order
                        .iter()
                        .position(|id| id == &target_id);
                    if let (Some(source_index), Some(target_index)) = (source_index, target_index)
                        && source_index != target_index
                    {
                        let moved = view.running_app_order.remove(source_index);
                        view.running_app_order
                            .insert(target_index.min(view.running_app_order.len()), moved);
                        cx.notify();
                        true
                    } else {
                        false
                    }
                });
                let message = if reordered {
                    format!("Moved {} in running app order.", drop.source.name)
                } else {
                    "Running app order did not change.".to_owned()
                };
                announce_dock_status(
                    reorder_status_state.clone(),
                    reorder_status_surface.clone(),
                    message,
                    cx,
                );
                return;
            }

            // QML ignores pinned→running and unpinned→unpinned drops. Drops
            // within the pinned group and running→pinned both update pinnedApps.
            if !drop.target.pinned {
                let message = if drop.source.pinned {
                    "No change: pinned apps stay in the pinned section.".to_owned()
                } else {
                    "Dock order did not change.".to_owned()
                };
                announce_dock_status(
                    reorder_status_state.clone(),
                    reorder_status_surface.clone(),
                    message,
                    cx,
                );
                return;
            }
            let source_name = drop.source.name.clone();
            let source_was_pinned = drop.source.pinned;
            let write_sequence = begin_pinned_write();
            let operation_generation = announce_dock_status(
                reorder_status_state.clone(),
                reorder_status_surface.clone(),
                if source_was_pinned {
                    "Saving pinned app order…".to_owned()
                } else {
                    format!("Pinning {source_name}…")
                },
                cx,
            );
            let status_state = reorder_status_state.clone();
            let status_surface = reorder_status_surface.clone();
            cx.spawn(async move |task_cx| {
                let result = task_cx
                    .background_executor()
                    .spawn(async move { persist_pinned_drop(&drop, write_sequence) })
                    .await;
                let message = match result {
                    Ok(PinnedWriteOutcome::Saved) if source_was_pinned => {
                        Some("Pinned app order saved.".to_owned())
                    }
                    Ok(PinnedWriteOutcome::Saved) => {
                        Some(format!("Pinned {source_name} to the dock."))
                    }
                    Ok(PinnedWriteOutcome::NoChange) => {
                        Some("Dock order did not change.".to_owned())
                    }
                    Ok(PinnedWriteOutcome::Superseded) => None,
                    Err(error) => {
                        eprintln!("ii Dock reorder: {error}");
                        Some(format!(
                            "Could not save dock order: {}",
                            error.chars().take(180).collect::<String>()
                        ))
                    }
                };
                if is_latest_pinned_write(write_sequence)
                    && let Some(message) = message
                {
                    task_cx.update(|cx| {
                        announce_dock_status_if_current(
                            status_state,
                            status_surface,
                            operation_generation,
                            message,
                            cx,
                        );
                    });
                }
            })
            .detach();
        });
        let status_surface = surface.clone();
        let drag_status_changed = Rc::new(move |cx: &mut gpui::App| {
            let _ = status_surface.update(cx, |view, cx| {
                if view.drag_state.borrow().is_active() {
                    view.preview_generation = view.preview_generation.wrapping_add(1);
                    view.app_hovered = false;
                    view.popup_hovered = false;
                    view.close_preview(cx);
                }
                cx.notify();
            });
        });
        dock::render(DockInput {
            geometry: DockGeometry {
                width: self.width,
                height: self.height,
                edge,
                icon_size: size,
            },
            style,
            apps,
            separate_pinned_from_running,
            show_all_window_dots: options
                .get("showAllWindowDots")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            smart_indicator: options
                .get("smartIndicator")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            max_indicator_dots: options
                .get("maxIndicatorDots")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(5)
                .clamp(1, 32) as usize,
            drag_reorder_enabled,
            drag_state: self.drag_state.clone(),
            on_reorder: Some(reorder),
            on_drag_status_changed: Some(drag_status_changed),
            on_activate_app: Some(activate_app),
            on_open_launcher: None,
            on_hover: Some(hover),
        })
    }
}

impl IiDockSurface {
    /// Preserve the first observed order of running app ids, like the QML
    /// dock's `_runningAppOrder`, while dropping apps that no longer run.
    fn refresh_running_app_order(&mut self) {
        let windows = self.windows_for_output();
        let present = windows
            .iter()
            .filter_map(|window| window.app_id.as_deref())
            .filter(|id| !id.trim().is_empty())
            .map(str::to_lowercase)
            .collect::<std::collections::HashSet<_>>();
        self.running_app_order.retain(|id| present.contains(id));
        if let Ok(mut positions) = self.last_focused.lock() {
            positions.retain(|id, _| present.contains(id));
        }
        for id in windows
            .iter()
            .filter_map(|window| window.app_id.as_deref())
            .filter(|id| !id.trim().is_empty())
            .map(str::to_lowercase)
        {
            if !self.running_app_order.contains(&id) {
                self.running_app_order.push(id);
            }
        }
    }

    /// Niri's window API identifies a window's workspace, while workspace
    /// snapshots carry output assignment. A named output accepts only windows
    /// mapped to one of its workspaces; unknown and unassigned windows are
    /// omitted so they do not appear on every monitor. Floating windows with a
    /// workspace id are included by the same mapping. When Niri cannot provide
    /// an output name, include the full snapshot as a best-effort fallback.
    fn windows_for_output(&self) -> Vec<niri::Window> {
        let Some(output) = self.output.as_deref() else {
            return self.snapshot.windows.clone();
        };
        if output.trim().is_empty() {
            return self.snapshot.windows.clone();
        }
        let workspace_ids = self
            .snapshot
            .workspaces
            .iter()
            .map(|workspace| workspace.id)
            .collect::<std::collections::HashSet<_>>();
        self.snapshot
            .windows
            .iter()
            .filter(|window| {
                window
                    .workspace_id
                    .is_some_and(|workspace_id| workspace_ids.contains(&workspace_id))
            })
            .cloned()
            .collect()
    }
}

fn resolve_dock_icon_path(
    catalog: Option<&XdgDesktopEntrySource>,
    app_id: &str,
    target_size: u32,
) -> Option<std::path::PathBuf> {
    catalog
        .and_then(|catalog| catalog.resolve_icon_path_for_size(app_id, target_size))
        .or_else(|| {
            catalog.and_then(|catalog| {
                catalog.resolve_icon_path_for_size("image-missing", target_size)
            })
        })
}
