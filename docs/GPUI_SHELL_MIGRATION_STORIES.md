# GPUI-shell migration user stories and flows

**Status:** Draft for final docs review
**Date:** 2026-10-19
**Inputs:** [migration PRD](GPUI_SHELL_MIGRATION_PRD.md),
[surface map](GPUI_SURFACE_MAP.md),
[migration acceptance criteria](GPUI_SHELL_MIGRATION_ACCEPTANCE.md),
[visual acceptance criteria](GPUI_ACCEPTANCE.md),
[upstream architecture ADR](GPUI_SHELL_ARCHITECTURE.md)

This document decomposes the approved migration objective into role-based
stories and observable flows. It is a planning artifact; stories describe
target behavior and do not claim that the behavior is implemented or accepted.
The QML shell remains the production reference and fallback throughout the
migration.

## Blocking gate: upstream source licensing

The read-only audit of upstream revision
[`110f0f7126df8df9c738924b94130a0e0c057a06`](https://github.com/andre-brandao/gpui-shell/tree/110f0f7126df8df9c738924b94130a0e0c057a06)
found no root source `LICENSE`/`COPYING` file and no `license` field in the
checked workspace crate manifests. The separate icon-assets license does not
establish a source-code license.

**No upstream source may be copied, vendored, forked, linked, subtree-imported,
or redistributed until the copyright holder provides a verifiable license or
written grant and maintainers record the review.** Documentation and
iNToo-authored contracts may proceed. This gate precedes all source
incorporation and dependency wiring; it is not satisfied by the repository
being public or by iNToo's own GPL license.

The audit also identifies upstream as a standalone application, not a
published embedding library, and does not establish a third-party plugin ABI.
The stories below make no promise to use an upstream plugin mechanism. Any
environment boundary is an iNToo-owned, statically composed design decision
to be finalized only after source clearance and a verified integration spike.

## Roles

- **iNToo user:** selects and operates ii, Waffle, or Iris on a Niri session.
- **Maintainer:** resolves upstream provenance, owns integration and release
  decisions, and keeps compatibility with the installed shell.
- **Environment implementer:** ports one bounded environment/surface against
  the QML reference and migration contracts.
- **Runtime/integration engineer:** owns host lifecycle, compositor/provider
  adapters, configuration, IPC, services, and packaging compatibility.
- **Reviewer/QA:** checks source-to-target coverage, interaction and visual
  evidence, failure states, and rollback; does not infer parity from a build.

## 1. Upstream onboarding, licensing, and integration

### UP-1 — Establish permission and provenance before source use

**As a maintainer,** I need an authoritative source-code license or written
grant for the exact upstream revision, **so that** the team can make a lawful
and attributable integration decision before incorporating upstream code.

**Current state:** the audited revision has no root source license and checked
crate manifests do not declare one; the source-import gate is blocked.

**Target flow:**

1. Identify the copyright holder and request/locate an authoritative source
   license or written grant.
2. Review source, contribution, redistribution, and copied-asset terms; record
   the reviewed evidence, scope, revision, and decision in the repository.
3. If clearance is absent or insufficient, keep the source-import gate closed
   and continue only with iNToo-authored planning/contracts.
4. If clearance is recorded, proceed to the separately reviewed integration
   strategy and provenance checks.

**Acceptance:** no upstream implementation or dependency is incorporated before
clearance. Every later imported revision has traceable upstream provenance and
applicable notices. This story has no implementation shortcut or inferred
license outcome.

### UP-2 — Reproduce and understand the selected upstream base

**As a maintainer,** I need a reproducible build and runtime audit for the
cleared, pinned upstream revision, **so that** iNToo can choose an integration
boundary based on actual ownership and capabilities.

**Current state:** the read-only audit describes a standalone Cargo workspace,
`crates/app` as the default binary member, separate UI/services/assets/story
crates, an upstream-owned app composition, a singleton overlay manager, a Niri
provider, and a moving GPUI dependency. It is not a verified embedding API.

**Target flow:**

1. Record the exact upstream revision, toolchain, native dependencies, build
  commands, and runtime prerequisites.
2. Build and run that revision in an isolated development environment.
3. Trace application entry, event loop, layer-shell/output ownership, service
  startup/shutdown, configuration and IPC ownership, update mechanism, and
  candidate reusable code seams.
4. Compare the observed seams with iNToo's surface, configuration, IPC,
  service, multi-output, and lifecycle contracts.
5. Record an integration ADR stating whether iNToo contributes upstream,
  maintains a fork, imports a subtree, or selects another reviewed method;
  identify update ownership and the boundary for iNToo-only changes.

**Acceptance:** another maintainer can reproduce the pinned build and explain
which responsibilities remain upstream-owned versus iNToo-owned. The ADR does
not describe the binary as a library or claim an unverified extension API.

### UP-3 — Prove an isolated host integration

**As a runtime/integration engineer,** I need a separate test target based on
the cleared and pinned upstream base, **so that** host feasibility is proven
without replacing or disturbing the QML shell.

**Current state:** the existing GPUI prototype is standalone migration research
and is not evidence that upstream can host iNToo's independent per-output
surfaces. The audited upstream application has its own startup, configuration,
IPC, surface, and service ownership.

**Target flow:** start the test target alongside QML; read the existing iNToo
configuration without competing writes; create, update, and close test
layer-shell surfaces for identified Niri outputs; exercise anchors,
reservations, input/focus, output hotplug, diagnostics, service ownership, and
clean shutdown; then stop the test target and confirm QML remains usable.

**Acceptance:** startup and shutdown do not disturb QML, compete for config
writes, exclusive zones, singleton IPC/DBus names, or global service ownership.
All host capability gaps have an owner and resolution plan before environment
surface migration depends on them.

## 2. Material foundation and environment selection/isolation

### ENV-1 — Share Material primitives without making a fourth environment

**As an environment implementer,** I need a shared iNToo Material foundation
for semantic tokens, controls, focus, and accessibility, **so that** genuinely
common UI behavior stays consistent while each environment retains its own
visual identity.

**Current state:** the product model is one shared Material foundation and
three environments. Material ii is one environment; it is not a fourth
environment. The standalone prototype's token scaffolding is incomplete and
does not yet resolve the full QML appearance contract.

**Target flow:** resolve current iNToo theme/configuration into semantic
Material roles; render shared controls and state presentations; allow a family
to compose or deliberately override compatible visuals; verify the result
against QML at matching theme, output scale, and state.

**Acceptance:** the same source configuration produces the expected ii,
Waffle, or Iris appearance; shared controls preserve keyboard/focus behavior
and accessible names/states. A generic Material appearance is not accepted as
a substitute for QML-derived tokens.

### ENV-2 — Select one registered environment compatibly

**As an iNToo user,** I need my existing `panelFamily` choice to select ii,
Waffle, or Iris after startup/reload, **so that** saved settings continue to
identify the same environment.

**Current state:** QML supports three family IDs and an ordered
`familyCycle`. The standalone GPUI prototype has a static selector scaffold,
but cross-family live switching and cycle command are not implemented. The
target gpui-shell integration seam is not yet proven.

**Target flow:** the iNToo host reads the existing family value; it validates
that value against the built-in environment registry agreed for the integration;
it presents the selected composition only after retiring old-family content;
invalid or incompatible values produce a diagnostic and the documented safe
fallback without silently rewriting the user's JSON.

**Acceptance:** `ii`, `waffle`, and `iris` remain stable IDs; configured cycle
order and transition behavior are preserved; duplicate/unknown cycle entries
are handled deterministically; no two family compositions or stale input
regions remain visible after a switch. If `ii` itself cannot be constructed,
the host reports startup failure rather than guessing another composition.

### ENV-3 — Keep environment behavior and state isolated

**As an environment implementer,** I need family-owned composition, style
selectors, and local UI state, **so that** one environment cannot leak its
panels or appearance into another.

**Current state:** QML has separate ii, Waffle, and Iris composition roots,
with shared panels gated by composition. Existing style namespaces differ:
`appearance.globalStyle` and `bar.appearanceStyle` belong to ii, Waffle has
its own looks/settings, and Iris has Iris presets/themes. The target boundary
is iNToo-owned; upstream plugin loading behavior is not established.

**Target flow:** host resolves family, enabled panel IDs, output, configuration
snapshot, and available service capabilities; selected environment composes
only its supported content; host owns surface creation/lifecycle; on family
switch it closes old content and input regions before presenting new content.

**Acceptance:** style IDs are interpreted only by their owning environment;
family-local settings/state remain namespaced; unknown config fields survive;
disabled or unsupported panels are omitted with a useful diagnostic; one
environment's failure does not close unrelated host surfaces. This does not
assume dynamic library plugins.

## 3. Surface migration and state parity

### SURF-1 — Port the first-frame and critical shell surfaces

**As an iNToo user,** I need the selected environment's critical surfaces to
appear at the same lifecycle point and on the same outputs as QML, **so that**
login and workspace interaction remain predictable.

**Current state:** QML composition roots define resident, deferred, and
on-demand surfaces. The surface map prioritizes the shared host/layer
contract, ii background and bars/dock, Waffle background/taskbar, and Iris
background/field/chassis. GPUI implementations are partial; no inventory row
is complete solely because a prototype file exists.

**Target flow:** wait for config/session/output readiness; resolve family and
panel gates; create per-output backgrounds and the family-critical edge
surfaces with matching anchors, exclusive zones, material, input regions, and
readiness states; reconcile output add/remove/resize/rename without stale
surfaces.

**Acceptance:** each critical root is traced from QML source to target
implementation and evidence. Output identity is validated, reservations match,
transparent areas do not intercept unrelated input, lock/session transitions
are safe, and failed/missing providers are visibly unavailable rather than
fabricated.

### SURF-2 — Migrate remaining family surfaces by vertical slice

**As an environment implementer,** I need to port one surface with its state,
configuration, service bindings, and accessibility in a reviewable slice,
**so that** every inventory item progresses toward parity rather than being
replaced by a visual placeholder.

**Current state:** `GPUI_SURFACE_MAP.md` is the source-to-target inventory. It
includes ii bars, dock, sidebars, dashboard, overview and quick surfaces;
Waffle primary panels; Iris content, dock and edge triggers; settings/editors;
widgets and wallpaper tools; shared and family-specific transient/security
surfaces. The acceptance document contains slice-specific requirements and
explicit prototype gaps.

**Target flow:** for a selected inventory row, identify its QML composition and
states; implement target rendering and real state/service/config binding; cover
loading, empty, disabled, unavailable, error, and dismissal states as
applicable; compare paired captures; exercise keyboard, pointer, wheel, drag,
focus, multi-output, and lifecycle behavior; obtain specialist review; link
evidence back to the inventory.

**Acceptance:** every in-scope row has an implementation and evidence record or
an explicitly approved exception. An unavailable capability is represented
honestly. Screenshots, a static build, or another family's visually similar
surface do not independently establish parity.

### SURF-3 — Preserve each surface's interaction and lifecycle contract

**As an iNToo user,** I need the same supported open, update, focus, action,
dismissal, and failure behavior as the QML reference, **so that** migrated
surfaces remain usable rather than merely recognizable.

**Current state:** the acceptance specification covers detailed Waffle Task
View and ii bar behavior and broader family/cross-family gates. The GPUI
prototype has documented partial states and limits; the new upstream base has
not been shown to preserve iNToo lifecycle behavior.

**Target flow:** route typed current service/config state into the active
surface; keep focus and selection semantics stable as data changes; perform an
action through the host/service boundary; handle provider failure or loss;
restore or dismiss focus according to the QML contract; remove surfaces and
input regions during close, disable, lock, family change, and shutdown.

**Acceptance:** use the existing acceptance scenarios for concrete behavior,
including Task View's output scoping/search/focus, bar geometry and output
selection, family transitions, empty/error states, and transient dismissal.
Acceptance evidence is recorded per scenario and supported environment.

## 4. Configuration, IPC, and services

### COMPAT-1 — Preserve existing configuration and safe read-only prototype use

**As an iNToo user,** I need the new runtime to understand existing iNToo
configuration without dropping unknown or unrelated values, **so that** I can
test it without migrating or damaging my current setup.

**Current state:** iNToo stores JSON configuration with defaults, migrations,
legacy paths, family IDs, enabled-panel gates, and namespaced settings.
Upstream uses a separate TOML schema and defaults. The test prototype must not
compete with QML for writes.

**Target flow:** prototype opens the existing supported config path read-only;
it resolves known values through an explicit iNToo adapter while retaining
unknown source fields; diagnostics distinguish absent, invalid, and unsupported
values; a write path is enabled only after single-writer ownership and atomic
compatibility behavior are specified and reviewed.

**Acceptance:** existing paths, defaults, migration semantics, family/style
selectors, panel gates, and unknown fields remain compatible. No upstream
default silently overwrites iNToo data. Side-by-side testing has one config
writer at most.

### COMPAT-2 — Keep command, IPC, and keybind contracts stable

**As a user or automation invoking iNToo,** I need existing commands and IPC
targets, names, arguments, return values, and keybinds to keep their meaning,
**so that** changing the visual runtime does not break established workflows.

**Current state:** these contracts are owned by iNToo documentation and runtime
scripts; upstream has its own application/single-instance IPC and startup
policy. Equivalence has not been established.

**Target flow:** existing entrypoints continue to address the production QML
runtime during migration; a test-target command is isolated and explicit;
eventually a compatibility adapter routes supported requests to the selected
runtime without registering conflicting endpoints; errors retain a defined
return/diagnostic contract.

**Acceptance:** config/IPC fixtures and keybind behavior pass against the
replacement before cutover; prototype mode cannot hijack production IPC or
start duplicate global services. The detailed contract remains
`docs/IPC.md` and related lifecycle documentation.

### COMPAT-3 — Use one service owner and explicit capability states

**As a user,** I need panels to show truthful compositor, audio, media,
network, notification, tray, wallpaper, lock, and session state, **so that** a
missing provider is not presented as a successful or fabricated result.

**Current state:** QML services and iNToo adapters define existing behavior;
upstream has its own service implementations and providers. Provider coverage,
data shape, native dependencies, and ownership need per-service review.

**Target flow:** select one owner per service in each runtime mode; normalize
provider events into iNToo-owned state/actions; surface loading, unavailable,
stale, reconnecting, and error states; route actions through typed host-owned
operations; shut providers down cleanly. Keep optional native dependencies
optional where iNToo's supported behavior allows it.

**Acceptance:** service-specific adapter contracts and acceptance scenarios
pass; no duplicate daemon/DBus name or competing compositor connection is
introduced; missing actions/capabilities are explicit; startup, reconnect,
failure, and shutdown are checked. The Niri provider retains the output,
workspace, window, focus, and capability data required by iNToo.

## 5. Side-by-side rollout, cutover, and rollback

### ROLL-1 — Test alongside QML without production ownership

**As a user testing the replacement,** I need to launch and stop the new
runtime independently, **so that** I can inspect it while keeping my working
desktop and rollback path intact.

**Current state:** Quickshell/QML is production. The migration requirements
prohibit switching the launcher, removing QML, competing for config writes,
exclusive zones, or service ownership during development.

**Target flow:** install/launch a clearly separate test target; use read-only
config and explicitly isolated service/IPC ownership; run representative
surfaces; inspect logs and diagnostics; stop it and confirm surfaces,
reservations, sockets, and owned resources are released; continue using QML.

**Acceptance:** repeated prototype start/stop leaves the QML session
operational and does not change production launcher/service configuration.
Unsupported capabilities are listed rather than masked.

### ROLL-2 — Approve cutover only after complete parity evidence

**As a release owner,** I need every family, surface, compatibility, lifecycle,
packaging, and accessibility gate to have reviewable evidence, **so that** a
production switch does not turn an incomplete prototype into the supported
shell.

**Current state:** the surface map is broad, the acceptance file is a
specification rather than proof, and GPUI/upstream migration work is partial.

**Target flow:** audit each inventory row and acceptance scenario; run paired
QML/replacement checks in supported Niri sessions and output arrangements;
exercise config, IPC, services, keyboard/pointer/accessibility, startup,
shutdown, install/update, and failure paths; resolve all P0/P1 regressions;
record reviewer and release-owner approval.

**Acceptance:** all required criteria pass with evidence, remaining differences
are explicitly approved compatibility changes, and cutover plus rollback has
been rehearsed. Build/test success alone does not pass this story.

### ROLL-3 — Retain and exercise rollback before removing QML

**As a user or operator,** I need a tested path back to the Quickshell runtime,
**so that** an installation, startup, or runtime failure does not leave the
desktop unusable.

**Current state:** QML and Quickshell remain the authoritative production
implementation and fallback; no cutover is authorized by the migration plan.

**Target flow:** install the replacement behind a reversible runtime choice;
verify clean install and upgrade; switch to replacement; exercise status/logs,
restart, and recovery; revert launcher/service/package selection to QML; verify
the prior config and session can start; retain QML for the agreed stabilization
period.

**Acceptance:** rollback passes on supported package paths and systemd/OpenRC
flows before cutover. QML payload and dependencies are removed only after the
new runtime has shipped, stabilization requirements are met, and rollback is
no longer required.

## Cross-story dependencies and review handoff

1. `UP-1` is a hard prerequisite for any upstream source incorporation. `UP-2`
   and `UP-3` must validate the real integration seam before `ENV-*` stories
   depend on it.
2. `ENV-*` relies on the host lifecycle and existing iNToo family/config
   contracts; it does not authorize production launcher or QML changes.
3. `SURF-*` uses `GPUI_SURFACE_MAP.md` as the complete scope and
   `GPUI_ACCEPTANCE.md` as the observable parity reference.
4. `COMPAT-*` applies to all surface slices and both test/production modes;
   provider and config ownership must be explicit.
5. `ROLL-*` follows complete coverage and parity evidence; it cannot be used to
   narrow the migration scope.

### Handoff to acceptance criteria (#09)

Convert each story's target flow and acceptance statement into independently
checkable Given/When/Then scenarios. Preserve the licensing block as a
precondition to source import, not as an implementation detail. Link existing
surface-specific criteria rather than replacing them.

### Handoff to edge-case review (#08)

Review invalid/unknown family IDs, absent `ii`, family changes during open
transients, output hotplug and stale identity, config reload/unknown fields,
service loss and duplicate ownership, upstream revision drift, denied/unclear
license evidence, interrupted upgrades, and rollback after partial startup.
Add cases without assuming an upstream plugin ABI or weakening the source
licensing gate.

## References

- [Migration PRD](GPUI_SHELL_MIGRATION_PRD.md)
- [Upstream architecture and licensing ADR](GPUI_SHELL_ARCHITECTURE.md)
- [Visual surface map](GPUI_SURFACE_MAP.md)
- [Visual acceptance criteria](GPUI_ACCEPTANCE.md)
- [Environment plugin contract](GPUI_ENVIRONMENT_PLUGIN_CONTRACT.md)
- [Configuration system](CONFIG_SYSTEM.md)
- [IPC contract](IPC.md)
- [Runtime lifecycle](RUNTIME.md)
