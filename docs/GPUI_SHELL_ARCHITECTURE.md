# ADR: adopt `gpui-shell` as the iNToo runtime base

- **Status:** Migration direction accepted; importing upstream source is
  blocked pending licensing clearance.
- **Date:** 2026-10-19
- **Decision owners:** iNToo maintainers
- **Scope:** Replace the current standalone GPUI prototype's foundation with
  the public `andre-brandao/gpui-shell` codebase, then migrate iNToo's visual
  layer and shell integrations onto that foundation.

## Decision

Continue the rewrite on `develop` toward the public `gpui-shell` architecture,
but **do not vendor, fork, subtree, link, copy, or redistribute upstream source
yet**. The audited upstream commit has no root license file and its checked
workspace crate manifests do not declare a `license` field. Current source
adoption is **NO-GO**. In the meantime, iNToo-owned documentation, contracts,
interfaces, and migration planning may proceed without copying upstream code.

Reconsider only after the copyright holder provides an identifiable license or
written grant that explicitly covers the intended upstream source revision and
files, modification, incorporation into iNToo, redistribution in source and
binary packages, and any needed sublicensing to iNToo users/downstream
distributors. Preserve required notices and audit code/assets with separate
provenance. Record the grant, covered revision, and approved distribution scope
in the repository before source intake. Revalidate scope for each upstream
update and newly imported asset/component; an upstream change or unclear scope
returns the gate to NO-GO. A repository's public visibility or a Cargo Git
reference is not a grant.

If licensing is cleared, the bounded integration option is an
**iNToo-maintained fork synchronized by Git subtree**, with upstream history
and each imported revision identifiable. Keep the upstream workspace at a
defined boundary; make the iNToo runtime the product entry point and adapt only
reviewed reusable seams (GPUI startup/platform hooks, selected `ui`/`assets`/
`services` interfaces). Keep iNToo-specific host, Material foundation,
environment registry, config, Niri policy, and lifecycle outside upstream's
default product composition. Avoid broad divergence or duplicating all
upstream product modules; if the narrow seams cannot support iNToo without a
permanent invasive fork, stop and re-evaluate before expanding the subtree.
Do not consume the `gpuishell` binary as a Cargo dependency: it is an
application binary, not a published host library, and its `app` crate depends
on its own workspace crates and startup policy. A Cargo link is not an
acceptable workaround for the unresolved source license.

The current production launcher and Quickshell/QML path remain authoritative
until the existing acceptance gates prove parity and rollback. This decision
does not authorize changing setup/startup, deleting QML, or switching the
production launcher.

## Context and audit evidence

The local GPUI prototype is currently an isolated crate at
`prototypes/gpui-shell/`, with direct dependencies on `gpui` and
`gpui_platform` from Zed and local iNToo modules under `src/families/`. The
repository has no root Rust workspace. Existing docs already define a shared
Material foundation plus three environment compositions (`ii`, `waffle`,
`iris`) and explicitly keep QML production until parity; see
[`GPUI_MIGRATION.md`](GPUI_MIGRATION.md),
[`GPUI_ENVIRONMENT_PLUGIN_CONTRACT.md`](GPUI_ENVIRONMENT_PLUGIN_CONTRACT.md),
and [`GPUI_ACCEPTANCE.md`](GPUI_ACCEPTANCE.md).

Read-only audit of public upstream `andre-brandao/gpui-shell` at commit
[`110f0f7126df8df9c738924b94130a0e0c057a06`](https://github.com/andre-brandao/gpui-shell/tree/110f0f7126df8df9c738924b94130a0e0c057a06)
found:

- Its root `Cargo.toml` is a standalone Cargo workspace (`crates/*`), with
  `crates/app` as the default member and separate `ui`, `services`, `assets`,
  and `story` crates. The app package is a binary named `gpuishell`; it is not
  a library API intended for embedding.
- The app brings its own top-level bar, dock, launcher, control center,
  notifications, OSD, configuration, single-instance IPC, startup, and
  compositor selection. Those are upstream product behavior and are not
  equivalent to iNToo's surface inventory or installation/lifecycle contract.
- `crates/app/src/panel.rs` uses one process-global
  `ACTIVE_PANEL: Mutex<Option<(String, AnyWindowHandle)>>`; toggling any panel
  closes the previous one. It opens an on-demand-focus layer-shell overlay.
  That mechanism is useful as a reference for simple popovers, but it cannot
  directly host iNToo's independent, per-output, environment-specific surface
  set and state without changing its ownership and lifecycle model.
- Upstream includes a Niri backend using `niri-ipc` and an event stream, alongside
  Hyprland and Mango backends. It maps compositor data to a generic service
  state; that is a useful starting adapter, not proof of iNToo-specific output,
  workspace, window-preview, action, and configuration parity.
- The upstream README describes the project as early development and notes
  substantial AI-generated code. Treat upstream code as actively changing and
  review it before each update.
- At the audited commit, no root `LICENSE`/`COPYING` file was present, and the
  checked `app`, `ui`, `services`, and `assets` Cargo manifests declare no
  package `license`. A separate icon assets license exists, but it does not
  establish a license for the project source. Confirm source-code licensing
  before importing or redistributing it.

## Integration boundary

### Upstream base

After licensing clearance, retain upstream's GPUI-facing runtime and reusable
crates where they fit: GPUI application/event-loop setup, UI primitives and
focus helpers, asset loading, and compositor/system service foundations. Pin
the imported subtree to an auditable upstream revision. Review upgrades as
explicit subtree merges; do not track a moving branch or unpinned `main` GPUI
dependency in release builds.

Do not treat upstream's default `app` composition, config schema, launcher,
control center, panel singleton, compositor auto-detection, or service
ownership as iNToo requirements. Replace or adapt those at the iNToo host
boundary, preserving only behavior that has been checked against the iNToo
contracts.

### iNToo host and Material foundation

The iNToo host owns the iNToo process contract, config compatibility and
watching, IPC, runtime lifecycle, output-to-surface reconciliation,
`WindowHandle`/layer-shell ownership, accessibility, and shutdown. It creates
surfaces and asks registered composition plugins for content. Environment
plugins return views and typed state/actions; they do not open or remove
top-level windows, claim compositor services, or write global config directly.

The shared iNToo **Material** layer is an iNToo-owned semantic design system
implemented using GPUI and, where useful, upstream UI primitives. It owns
semantic colors, typography, spacing, elevation, shape, focus/accessibility
helpers, and reusable controls. Upstream's existing visual style is not the
default Material specification. iNToo retains visual tokens and behavior from
the QML reference and validates them with paired captures.

The built-in `EnvironmentPlugin` registry contains three compositions:

- `ii` — existing ii panels, bar/dock/sidebar, settings, and module order;
- `waffle` — Waffle surfaces, launcher/task view/notifications, and backdrop;
- `iris` — Iris chassis, dock, palette, and environment-specific style.

Plugins register stable IDs and metadata and receive resolved config, output
identity, platform snapshots, and Material capabilities from the host. They
provide surfaces/content descriptors and typed action requests. The host
remains responsible for choosing output, role, layer, anchor, exclusive zone,
focus policy, and create/update/close behavior. Environment switching is a
host transition over the same registry, not a separate executable or upstream
`panel` toggle.

All asynchronous work is tagged with a monotonically increasing configuration
revision, environment generation, and output generation. On a config update,
environment switch, or output removal, the host invalidates affected work;
results and queued action requests must revalidate all relevant generations
before changing UI, writing config, or issuing compositor/service effects.
Discard stale completions. A stale action must not execute against the newly
selected environment or a replacement output that reused the old identity.

Each environment may compose shared Material controls or supply a visual
override, but it must preserve keyboard navigation, focus restoration, screen
reader names, and observable states. Configuration remains in the existing
iNToo schema unless an explicit migration adds fields; upstream config must not
silently replace or overwrite existing values.

### Niri provider strategy

Make Niri the first and required backend for the iNToo migration. Use one
host-owned Niri service/provider as the source of compositor truth. Prefer the
upstream `niri-ipc` event stream and typed protocol adapter after compatibility
review; normalize events into iNToo-owned immutable snapshots rather than
letting GPUI views depend on upstream service structs. The snapshot must retain
the data required by iNToo surfaces, including connector/output name, logical
geometry and scale when supplied, per-output active workspace, workspace IDs
and ordering, window identity/title/app ID/workspace/output, focused state, and
capability-aware actions.

Apply provider events on the GPUI application executor. Keep command execution
behind typed iNToo actions; do not let panels spawn arbitrary Niri commands.
Handle initial snapshot, event-stream loss/reconnect, missing socket, stale
output identity, and unsupported actions as explicit states. Match GPUI display
IDs to Niri connector names through an explicit validated identity mapping; do
not use assumed enumeration order or geometry alone. Represent a connector as
its name plus a host-maintained hotplug generation. Increment/invalidate that
generation on removal or identity replacement, bind each GPUI display mapping
to the current generation, and reject snapshots/actions captured against an
older generation. If the GPUI platform API cannot provide a trustworthy
connector mapping, expose an unresolved/unavailable output state rather than
guessing. Where the upstream provider lacks a needed iNToo field/action, extend
an adapter or contribute upstream rather than silently dropping the contract.
Other upstream compositor backends stay disabled until iNToo explicitly scopes
and validates them.

## Run isolation, ownership, and fallback

### Dual-run policy and service ownership

Dual-run means validating in a dedicated test session/compositor context, not
starting the candidate alongside production Quickshell on the user's active
desktop. Use separate runtime/config/cache directories and distinct test IPC
endpoints. The test must not register upstream global IPC/single-instance
handlers, overwrite iNToo's production config, or compete for D-Bus well-known
names and shell services. Upstream's global IPC remains disabled; only the
iNToo host may expose compatible IPC, and only under an explicitly isolated
test endpoint until cutover.

| Domain | Production owner before cutover | Isolated candidate-test rule |
| --- | --- | --- |
| Surfaces, lock, session actions | Quickshell/iNToo | Candidate only in test session; security effects inert or mocked |
| Config and settings writes | Existing QML/config path | Isolated test copy only; never production config |
| Process/control IPC | Existing iNToo IPC | Upstream global IPC off; isolated test socket only |
| Notifications and tray | Configured desktop services | Claim only with no competing owner in test session |
| Wallpaper/Awww | Configured wallpaper owner | Disabled unless test session assigns sole ownership |
| Niri events/actions | Active session's Niri | Dedicated test compositor only; no production commands |
| Audio/system controls | Existing production service/UI | Read-only or mocked pending safe sole ownership |

This matrix captures safe defaults; the exact test-session mechanism, service
endpoints, and the final per-service cutover owners remain **unresolved** until
the platform/runtime audit selects them. Never infer exclusive ownership from
two processes using different window titles.

### Configuration ownership

The host owns one canonical iNToo configuration snapshot and serializes all
writes through one coordinator. Environment plugins receive immutable resolved
settings and submit typed updates; they do not watch/write the JSON file
independently. The coordinator validates against the iNToo schema, preserves
unknown fields, writes atomically, increments the configuration revision, and
publishes the accepted snapshot. In isolated tests, this coordinator is pointed
at an isolated test file. Do not run an upstream TOML watcher/writer against
iNToo configuration or allow QML and GPUI to write the same production file
concurrently. The precise writer handoff during eventual cutover is an
unresolved rollout parameter and requires a tested single-writer transition.

### Security-sensitive surfaces

Keep authentication prompts, lock/greeter surfaces, secret entry, and other
security-sensitive UI on the current production implementation. Exercise any
candidate implementation only in an isolated test session with non-sensitive
fixtures and no real session-control effects. Do not include these surfaces in
ordinary dual-run or transfer their ownership until a dedicated threat review,
focus/input isolation review, compositor capability review, and explicit
acceptance gate are complete. The security-surface inventory and review owner
are unresolved; absence from the current prototype is not evidence of parity.

### Stop/go and rollback gates

- **Source intake: NO-GO now.** Go only with documented rights scope, a pinned
  upstream/GPUI revision, provenance/notice audit, dependency review, and a
  bounded subtree plan. Any missing/ambiguous grant or material upstream
  licensing change is a stop condition.
- **Isolated host spike: GO only after source clearance.** Continue only if the
  iNToo entry point can run in the dedicated session without upstream global
  IPC, production config writes, service-name collisions, or production Niri
  actions, and can close/restart cleanly. A failure keeps the source integration
  stopped while iNToo-owned interface work may continue.
- **Surface slices: GO one slice at a time.** A slice advances only with source
  mapping, behavior/config coverage, paired visual review, keyboard/focus and
  accessibility checks, output-scope checks, and independent specialist review
  recorded against the acceptance criteria. Missing runtime evidence keeps the
  slice partial.
- **Cutover: NO-GO until global parity.** Switch production only after all
  supported surfaces, IPC/config, session lifecycle, service ownership,
  multi-output behavior, packaging, security-sensitive surfaces, and rollback
  criteria pass. Any critical regression, ownership collision, failed startup,
  or broken rollback during rollout triggers return to the last known-good QML
  runtime and halts rollout pending repair.

Current product/test parameters still unresolved include the rights grant and
scope, exact isolated-session mechanism, service endpoint/owner choices for
notifications/tray/wallpaper/audio, canonical config-writer handoff timing,
security-surface inventory/reviewer, and the compatibility contract for
upstream versus iNToo IPC. These must be settled with evidence before dependent
implementation; this ADR does not choose values for them.

## Migration sequence and gates

1. **Source and dependency clearance.** Resolve the upstream license, audit
   vendored/copied component attribution, and record the approved upstream
   revision and GPUI/Zed revision. Compare native dependencies with Gentoo,
   Arch, and Nix packaging. This is currently blocked; no source import or
   dependency link may occur before licensing is clear. Continue only with
   iNToo-authored contracts/docs that do not reproduce upstream implementation.
2. **Workspace integration spike.** Bring in the upstream workspace as a
   subtree and add an iNToo host member only after clearance. Verify clean build
   and startup in a dedicated Niri test session, isolated from the active
   production session. Keep upstream history and updates reviewable. Do not
   remove the existing local prototype until its iNToo code has been mapped and
   either ported or intentionally retired.
3. **Host and provider seam.** Move iNToo output/surface lifecycle behind the
   host contract, establish the normalized Niri event/action provider, and
   demonstrate output hotplug, per-output workspace scoping, focus, reconnect,
   and shutdown. The upstream singleton overlay may be used only for eligible
   transient UI after this host contract is met.
4. **Material and plugin base.** Implement shared iNToo Material tokens and
   controls, then register `ii`, `waffle`, and `iris` composition plugins. Add
   plugin contract tests and config compatibility checks. Do not equate three
   registry entries with complete family parity.
5. **Surface migration.** Use `GPUI_SURFACE_MAP.md` and
   `GPUI_ACCEPTANCE.md` as the inventory. Port always-visible and critical
   surfaces first, then transient surfaces and settings. For each slice, pair
   QML/GPUI screenshots at matching scale/output/config, test keyboard and
   pointer interactions, accessibility, multi-output behavior, and state/data
   adapters. Record partial/unavailable behavior explicitly.
6. **Runtime/packaging cutover.** Only after all acceptance criteria and
   rollback checks pass, update launcher, install/update scripts, systemd and
   OpenRC paths, package payloads/dependencies, and user documentation. Keep
   QML and Quickshell rollback available until the new runtime has shipped and
   upgrade/rollback paths are verified.

## Compatibility and dependency risks

- **License and provenance:** upstream source license was not established by
  the read-only audit. This is a hard import/redistribution gate. Audit icon
  assets and copied component code separately and preserve their notices.
- **Workspace shape:** upstream's default build targets its own binary. An
  iNToo integration must define workspace membership and startup explicitly;
  do not imply that adding `gpuishell` as a dependency reuses the shell host.
- **Moving GPUI dependency:** upstream currently selects Zed GPUI via the
  `main` branch in workspace dependencies. Pin both upstream and GPUI to tested
  revisions and upgrade them in reviewed changes. The local prototype pins a
  different Zed Git resolution in `Cargo.lock`, so API/dependency unification
  may require code changes and lockfile regeneration.
- **Native/system libraries:** upstream `services` includes PipeWire, PulseAudio,
  udev, zbus and sysinfo integrations plus Niri/Hyprland APIs. Check Gentoo
  dependency names, USE options, and optional service behavior; do not make
  upstream's dependencies unconditional where iNToo does not need them.
- **Configuration:** upstream uses its own TOML configuration and defaults;
  iNToo reads JSON and has legacy paths/migrations. Keep a translation/adapter
  boundary and preserve unknown iNToo fields until a separately reviewed
  schema migration.
- **Lifecycle and service ownership:** upstream single-instance IPC, startup,
  notifications, tray, and control-center behavior can conflict with iNToo's
  scripts or current Quickshell services in side-by-side testing. Each service
  must have one declared owner per mode; prototype startup must not register
  competing DBus names or launch duplicate daemons.
- **Parity:** upstream's advertised features and Niri support do not prove
  iNToo visual, accessibility, output, IPC, packaging, or rollback parity.
  Acceptance evidence remains per-surface and per-runtime as defined by the
  existing migration docs.

## Consequences

Once cleared, the migration has a stronger maintained starting point for GPUI
shell-specific services and reusable UI, while retaining iNToo-specific
Material, three environments, config compatibility, and visual contracts. For
now, source adoption is not authorized by the evidence available. The
architecture boundary keeps iNToo-owned work moving while the license question
is resolved. The existing GPUI prototype is migration material, not the chosen
foundation. No production behavior changes as a result of this ADR.

## References

- Upstream repository and audited revision:
  <https://github.com/andre-brandao/gpui-shell/tree/110f0f7126df8df9c738924b94130a0e0c057a06>
- Upstream workspace manifest:
  <https://github.com/andre-brandao/gpui-shell/blob/110f0f7126df8df9c738924b94130a0e0c057a06/Cargo.toml>
- Upstream app manifest:
  <https://github.com/andre-brandao/gpui-shell/blob/110f0f7126df8df9c738924b94130a0e0c057a06/crates/app/Cargo.toml>
- Upstream single-overlay panel manager:
  <https://github.com/andre-brandao/gpui-shell/blob/110f0f7126df8df9c738924b94130a0e0c057a06/crates/app/src/panel.rs>
- Upstream Niri provider:
  <https://github.com/andre-brandao/gpui-shell/blob/110f0f7126df8df9c738924b94130a0e0c057a06/crates/services/src/compositor/niri.rs>
- Upstream README (including early-development notice):
  <https://github.com/andre-brandao/gpui-shell/blob/110f0f7126df8df9c738924b94130a0e0c057a06/README.md>
- Local GPUI migration and acceptance contracts:
  [`GPUI_MIGRATION.md`](GPUI_MIGRATION.md),
  [`GPUI_SURFACE_MAP.md`](GPUI_SURFACE_MAP.md),
  [`GPUI_ENVIRONMENT_PLUGIN_CONTRACT.md`](GPUI_ENVIRONMENT_PLUGIN_CONTRACT.md),
  [`GPUI_ACCEPTANCE.md`](GPUI_ACCEPTANCE.md)
