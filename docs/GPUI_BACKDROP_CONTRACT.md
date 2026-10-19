# GPUI ii backdrop contract

Status: mounted on the ii host as a data-driven per-output GPUI surface.
`surface_manager.rs` supplies typed config, output identity, path status, and
theme colors; the component itself remains provider-free.

## Reference and inputs

`modules/background/Backdrop.qml` owns a full-output layer surface and selects
per-monitor wallpaper paths. The GPUI equivalent is
`prototypes/gpui-shell/src/components/desktop_backdrop.rs`, which consumes
`DesktopBackdropInput` supplied by a future host. Inputs include output name,
wallpaper path or host-decoded still, Loading/Ready/Missing status, media kind,
dim, saturation, contrast, blur radius, optional vignette, reduced-effects
flag, and semantic background/foreground colors. The component does not read
configuration, inspect files, enumerate outputs, or start media services.

## Implemented presentation

- Still images use GPUI `ObjectFit::Cover`; per-output identity remains part of
  the supplied component input.
- Loading, missing, decode fallback, and missing-source states are visible and
  carry status semantics.
- Dim is a clamped black overlay. Optional bottom vignette uses the supplied
  intensity/radius and is suppressed when reduced effects are enabled.
- Reduced-effects state is exposed. All image paths, values, and colors remain
  supplied by the host; no wallpaper sample is synthesized.

## Known gaps

- The host creates one `Layer::Background` backdrop surface and one transparent,
  input-transparent `Layer::Bottom` main wallpaper surface per output, matching
  QML's backdrop-behind-wallpaper order. GPUI defines `Background < Bottom`,
  so this avoids undefined same-layer stacking while keeping applications
  above both. The backdrop surface uses namespace
  `intoo-gpui-backdrop`, an empty input region, and exclusive zone zero.
- Config parsing includes `background.backdrop.*` and per-output
  `wallpapersByMonitor.backdropPath`. `useMainWallpaper` selects the main
  wallpaper (including monitor-specific main path); otherwise the host prefers
  the matching monitor backdrop path, then global backdrop path, then main
  wallpaper fallback. `enable` gates creation. When both `enable` and
  `hideWallpaper` are true, the host suppresses the separate main wallpaper
  surface while keeping the configured backdrop visible. The backdrop surface
  retains the resolved theme background as its solid fallback. It is opened
  for ii outputs when enabled, independently of the `iiBackground` panel gate.
- The config's `reducedEffects` has no resolved host source yet, so the host
  does not claim that effects are reduced. `enableAnimatedBlur` is parsed but
  unused. Live Niri compositor stacking/transparency has not been visually
  verified; only GPUI layer-shell ordering and input-region API semantics were
  checked in source.

- GPUI image rendering here has no blur, saturation, or contrast filter. Those
  values are accepted as input and non-neutral requests produce an explicit
  unsupported-effects status; they are not approximated with misleading color
  overlays.
- Video and animated image sources are not decoded or played. The component
  shows an unsupported status and never substitutes a fabricated frame.
- QML wallpaper transition, GIF/video pause policy, first-frame caching,
  aurora adaptive material, and decode-overflow blur compensation are absent.
- The QML bar-edge vignette is separate from the optional bottom backdrop
  vignette and is not represented here.
- Wallpaper decoding is not wired to the image cache yet; Ready still paths are
  passed to GPUI's image element for decode. The live compositor result remains
  to be checked in a Niri session.
