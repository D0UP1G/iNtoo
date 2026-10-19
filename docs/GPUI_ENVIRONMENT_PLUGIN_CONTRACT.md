# GPUI Environment Plugin Contract

Status: implementation contract, API version 1 proposal.

## Current prototype status

The GPUI prototype has a statically registered, version-checked selector for
`ii` (Material), `waffle`, and `iris`. Startup and config reload resolve
`panelFamily` through that registry. Missing or unregistered IDs select `ii`
and emit a diagnostic; the source JSON is left unchanged. The four family root
creation paths (ii horizontal/vertical, Waffle, Iris) now call the selected
plugin's composition factory with host-owned runtime adapters. The factory
returns the family-specific composition that `FamilyHost` embeds. The surface
manager continues to own all windows, layers, outputs, and reconciliation.
The current factory boundary returns the closed `FamilyComposition` enum;
`FamilyHost` still maps those variants to GPUI entities. Returning a typed GPUI
content root directly from each plugin remains a follow-up API refinement.

Cross-family lifecycle switching is not implemented. A family change that
requires a different composition needs a restart to activate it. `familyCycle`
is not parsed by the GPUI prototype and there is no GPUI cycle command; cycle
filtering is currently only a registry helper.

This contract separates the shared Material foundation from three environment compositions. The host owns GPUI windows, Wayland layer-shell state, outputs, config watching, and process lifetime. Environment plugins own only their visual root, family-specific composition, style tokens, and environment-local UI state.

## Target model

- `gpui-shell` is the host and shared Material component/theme foundation.
- `ii`, `waffle`, and `iris` are built-in environment plugins registered by the executable. `ii` is the Material environment and consumes the Material foundation; Waffle and Iris can supply their own visual delegates and tokens while reusing compatible shared services.
- The first implementation uses a Rust trait and a statically linked registry. This is an extensibility boundary, not a promise of loading arbitrary shared libraries. A stable dynamic ABI is out of scope for v1.
- The host owns every `WindowHandle`, output selection, layer-shell anchors, exclusive zones, create/update/close order, and GPUI `App` event loop. A plugin never creates or destroys a Wayland/GPUI window.

## Rust API shape

The following is the intended boundary; concrete GPUI types may be adjusted to the version pinned by the prototype, while ownership rules stay fixed.

```rust
pub const ENVIRONMENT_PLUGIN_API_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EnvironmentId(&'static str);

pub struct EnvironmentDescriptor {
    pub id: EnvironmentId,              // "ii", "waffle", or "iris"
    pub api_version: u16,
    pub display_name: &'static str,
    pub capabilities: &'static [EnvironmentCapability],
}

pub trait EnvironmentPlugin: Send + Sync {
    fn descriptor(&self) -> &'static EnvironmentDescriptor;

    /// Resolve environment-owned styling from the shared token source and this
    /// plugin's namespaced settings. Must not mutate host/global config.
    fn resolve_style(&self, input: &EnvironmentStyleInput) -> Result<EnvironmentStyle, PluginError>;

    /// Create a view model/root entity for the requested host-owned surface.
    /// The returned content is embedded into a window that the host created.
    fn create_surface(
        &self,
        request: SurfaceRequest,
        services: EnvironmentServices,
        cx: &mut gpui::App,
    ) -> Result<EnvironmentSurface, PluginError>;
}

pub struct EnvironmentSurface {
    pub role: SurfaceRole,
    pub root: gpui::AnyView,
    pub style: EnvironmentStyle,
}
```

`EnvironmentSurface` is content, not a window. It must not carry `WindowHandle`, `WindowOptions`, layer-shell configuration, output enumeration, or APIs that can open/close global windows. Host surface roles are a closed host-owned set (for example `Background`, `BarHorizontal`, `BarVertical`, `Dock`); a plugin can declare which roles it supports, but cannot invent an unmanaged top-level surface in v1. Transient overlays should be requested through typed host callbacks and opened by the host.

A production implementation may split `create_surface` into typed methods (bar/background/dock) to avoid downcasting. Prefer typed methods if a plugin role needs different state. Do not use a `serde_json::Value` as the service API.

## Contexts and services

`EnvironmentStyleInput` contains immutable, resolved shared theme inputs plus the plugin's namespaced config. Its output is semantic tokens, not raw window lifecycle data.

```rust
pub struct EnvironmentStyleInput<'a> {
    pub material: &'a MaterialTokens,
    pub config: &'a EnvironmentConfig,
    pub color_scheme: ColorScheme,
}

pub struct EnvironmentStyle {
    pub colors: SemanticColors,
    pub typography: TypographyTokens,
    pub shape: ShapeTokens,
    pub spacing: SpacingTokens,
    pub effects: EffectTokens,
    pub style_id: String,
}

pub struct EnvironmentConfig<'a> {
    pub settings: &'a serde_json::Value, // read-only view of one namespace
    pub revision: u64,
}

pub struct EnvironmentServices {
    pub clock: Arc<dyn ClockService>,
    pub windows: Arc<dyn WindowQueryService>,
    pub workspaces: Arc<dyn WorkspaceService>,
    pub notifications: Arc<dyn NotificationService>,
    pub audio: Arc<dyn AudioService>,
    pub actions: Arc<dyn ShellActionService>,
}
```

The service set is implemented incrementally: a capability may be absent, and the corresponding service must report an explicit `Unavailable` state rather than return fabricated data. Use narrower typed service bundles if a surface needs only some services. Services are host-owned and shared across environment switches where their lifetime permits it; a plugin may not start duplicate global providers.

The Material foundation exports reusable GPUI controls, layout primitives, semantic color/shape/spacing tokens, focus/accessibility helpers, and state presentation patterns. Environment plugins may compose those components or provide visual replacements. They must use semantic roles and preserve keyboard/focus behavior when replacing a component.

## Registry and selection

The executable builds an immutable registry at startup:

```rust
pub struct EnvironmentRegistry {
    plugins: BTreeMap<EnvironmentId, Arc<dyn EnvironmentPlugin>>,
}
```

Registration rejects duplicate IDs and descriptors whose `api_version` is not exactly supported. IDs are lowercase stable strings. V1 bundled registrations are `ii`, `waffle`, and `iris`.

`panelFamily` remains the persisted selector for compatibility. On startup or config reload:

1. If the configured ID exists in the registry and passes descriptor validation, select it.
2. If missing, unknown, or incompatible, log a diagnostic and select `ii` (the bundled Material environment). Keep the invalid source value visible to diagnostics; do not rewrite user config silently.
3. If `ii` is unexpectedly unavailable, fail closed: create no family content and report a startup error. The host must not guess another plugin.
4. For future live switching, ask the host to close old family surfaces before creating new composition content. Current cross-family changes require a restart.

When a GPUI config or cycle command is added, it should treat `familyCycle` as an ordered list of IDs, filter it to registered IDs, remove duplicates while preserving order, and fall back to `["ii"]` if no registered entry remains. The GPUI prototype does not currently implement that config/command path. The transition animation setting remains a host concern.

Family plugins do not take over global style keys. `appearance.globalStyle` stays the ii/Material environment's style selector and is interpreted only by `ii`; `bar.appearanceStyle` stays an ii bar layout selector. Iris continues to read its Iris preset/theme namespaces. Waffle continues to read its Waffle namespaces. These selectors cannot override `panelFamily`, and a style ID from one plugin must not be interpreted by another. Config parsing should preserve unknown namespaced keys for forward compatibility.

## Surface and config ownership

- The host owns critical surface policy, output filtering, `enabledPanels`, geometry, exclusivity, config revisioning, and family switching.
- Each plugin declares support for a host-owned role and receives the already-resolved target output/config snapshot. It cannot independently decide to open another output's window.
- Existing panel IDs remain stable (`iiBar`, `wBar`, etc.). The host combines `enabledPanels` with the selected plugin's role support before creating content. An unsupported/disabled role is omitted with a diagnostic where appropriate.
- Plugin settings are read-only, namespaced snapshots from the existing JSON config. Persistence remains through the host's existing config service; plugins request typed setting changes via a host callback, not by writing files.
- Shared panels/services remain host/shared components where their behavior is truly environment-neutral. Environment-specific composition remains inside that environment's plugin.

## Errors and fallback

`PluginError` should distinguish at least `UnsupportedApiVersion`, `DuplicateId`, `UnsupportedSurfaceRole`, `InvalidConfig`, `MissingCapability`, and `CreateViewFailed`. Registry errors are startup/configuration errors. Per-surface errors are logged with environment ID, role, and config revision; the host omits that surface and remains alive. A bad plugin must not close unrelated host surfaces or prevent another environment from starting.

On startup and config reload, a missing or unregistered `panelFamily` resolves to the `ii` registry entry. Current config reload reconciliation remains limited to the existing ii critical surfaces. Cross-family switching, plugin-specific config validation, and registry-driven content construction remain future work. The host must close old family surfaces before a future implementation opens a new composition, and it must not leave two environment families visible simultaneously.

## Repository evidence and migration notes

- `docs/PANEL_FAMILIES.md` defines three current families: Material ii, Waffle, and Island (called `iris` in code). It documents `panelFamily`, ordered `familyCycle`, `familyTransitionAnimation`, `enabledPanels`, and the separate style namespaces.
- `defaults/config.json` currently sets `familyCycle` to `["ii", "waffle", "iris"]` and `panelFamily` to `"ii"`.
- `prototypes/gpui-shell/src/family_host.rs` defines the registry, built-in descriptors, typed runtime adapter inputs, and composition factories. The surface manager calls those factories for all three registered environments before embedding the result in `FamilyHost`.
- `prototypes/gpui-shell/src/surface_manager.rs` creates `WindowHandle`s, layer-shell surfaces, manages per-display maps, and reconciles config. This remains the lifecycle owner and must call plugin factories only after choosing a host surface role/output.
- `prototypes/gpui-shell/src/config.rs` already preserves `panel_family`, `enabled_panels`, `bar.appearance_style`, `appearance.global_style`, and unparsed source sections. Existing `theme::Family` is a closed enum and must not become the plugin registry; use stable IDs at the plugin boundary.
- `prototypes/gpui-shell/src/theme.rs` currently has semantic color/typography/shape/spacing/opacity token scaffolding and explicitly notes that the full source theme resolver is not ported. Plugin API v1 should accept resolved token inputs and must not claim visual parity before that resolver is complete.

## Open implementation choices

1. Whether external third-party Rust plugins are ever required. V1 contract supports statically linked plugins only; a dynamic plugin ABI needs a separate security/versioning design.
2. Exact `SurfaceRole` set and typed factory signatures should be finalized against the GPUI window embedding APIs in the pinned dependency. The ownership rule is fixed: plugin returns content, host owns top-level windows.
3. `ii`/`waffle`/`iris` are based on the repository's existing three family IDs. If “three environments” means three different IDs, update the registry mapping before implementation while keeping `panelFamily` migration explicit.
