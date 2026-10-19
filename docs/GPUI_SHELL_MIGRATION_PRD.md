# GPUI-shell migration PRD

**Status:** Draft for decomposition<br>
**Date:** 2026-10-19<br>
**Owner:** iNtoo migration team<br>
**Direction:** Target the public
[`andre-brandao/gpui-shell`](https://github.com/andre-brandao/gpui-shell)
project as the eventual runtime base, subject to source clearance. Source
adoption is currently **BLOCKED**.

## 1. Summary

Migrate the complete iNtoo visual shell from Quickshell/QML to an implementation
based on GPUI-shell after source adoption is cleared. The migration must preserve iNtoo's visible surfaces,
interactions, configuration, service behavior, and supported session lifecycle.
The intended product model is one shared Material foundation with three
environment plugins: Material ii, Waffle, and Iris (the existing `iris` family
ID, also called Island in user documentation).

This PRD changes the intended implementation direction, not the migration
scope or the compatibility bar. Quickshell/QML remains the production
implementation and rollback path until the replacement has complete surface
coverage and verified
behavioral and visual parity. The current standalone `prototypes/gpui-shell/`
implementation is migration research and reusable work; it is not by itself
the destination architecture.

### Blocking source-adoption gate

The read-only audit found no authoritative source-code license for the audited
upstream revision. Do not copy, adapt, vendor, fork, link, subtree-import, or
otherwise incorporate GPUI-shell source, or add a dependency on that source,
until an authoritative license or written grant from the copyright holder is
obtained and maintainers record a provenance/licensing review covering the
revision, scope, and intended use. Public visibility and iNToo's own license do
not clear this gate. See [the architecture ADR](GPUI_SHELL_ARCHITECTURE.md)
and [migration stories](GPUI_SHELL_MIGRATION_STORIES.md) for the audit and
current blocked status.

While the gate is blocked, a read-only inspection of public upstream source
files is permitted solely to establish factual architecture findings (for
example, manifests and host/service ownership code). Do not copy, adapt, build,
execute, link, or otherwise incorporate that source. Audit findings describe
observed facts only; they do not grant permission to use the source or create
derivative work. Other work is limited to iNToo-authored documentation and
contracts plus independently written code that is not copied, adapted, or
otherwise derived from GPUI-shell implementation. Such code must be based on
iNToo's own requirements and contracts. Source adoption and
source-dependent integration spikes remain blocked.

## 2. Evidence and certainty

### Confirmed project facts

- iNtoo is currently a Quickshell/QML shell for Niri, with three selectable
  families: Material ii, Waffle, and Island/Iris. Their compositions, feature
  gates, styles, and surface inventory are documented in
  [ARCHITECTURE.md](../ARCHITECTURE.md), [PANEL_FAMILIES.md](PANEL_FAMILIES.md),
  and [GPUI_SURFACE_MAP.md](GPUI_SURFACE_MAP.md).
- The current migration contract requires preserving configuration, IPC,
  services, multi-output behavior, startup/lifecycle, packaging, and rollback;
  see [GPUI_MIGRATION.md](GPUI_MIGRATION.md) and
  [GPUI_ACCEPTANCE.md](GPUI_ACCEPTANCE.md).
- The prior implementation direction has a GPUI prototype in
  `prototypes/gpui-shell/` with a Material component foundation and partial
  family implementations. Its README explicitly records incomplete coverage
  and lack of full parity.
- The read-only upstream audit supplied for this work identifies
  `andre-brandao/gpui-shell` as an application/repository, not a library that
  iNtoo can assume it can import as a stable crate API. Integration details
  must therefore be verified from upstream sources before implementation.

### Assumptions pending verification

- The existing ii, Waffle, and Iris family IDs and their persisted config
  values remain the plugin identities. No family rename or config migration is
  intended unless a later compatibility review proves it necessary.
- “Material base” means shared Material design tokens, components, and common
  interaction/accessibility primitives. Material ii remains one of the three
  environment plugins and may compose that base; the base is not a fourth
  environment.
- After source clearance, GPUI-shell can serve as the application/runtime
  foundation if the reviewed integration strategy confirms the fit. This PRD
  does not assume its internal module layout, public extension API, license
  terms, release stability, or ability to load third-party plugins.
- Unless the upstream audit is supplemented with a confirmed extension
  mechanism, “plugins” means built-in, separately bounded Rust environment
  implementations. It does not promise dynamic shared-library loading.

## 3. Problem and opportunity

iNToo's complete visual product is spread over three QML family trees, shared
panels, settings, background widgets, transient/security surfaces, services,
and shell lifecycle code. A partial renderer or a new shell that only resembles
the current UI would break established user workflows and configuration.
Building atop GPUI-shell may provide a maintained shell foundation, but the
upstream project is not an established importable library contract. The team
must first determine a sustainable integration boundary, then port the full
inventory incrementally without disrupting the installed shell.

## 4. Goals

1. Replace every supported iNtoo visual surface and state with a GPUI-shell
   based implementation, retaining the observable behavior documented by the
   QML implementation.
2. Use a shared Material foundation and three isolated environment plugins:
   `ii`, `waffle`, and `iris`.
3. Preserve the user-facing configuration and IPC contracts, Niri behavior,
   multi-output placement, input/focus/accessibility, session services, and
   install/update/rollback behavior.
4. Keep the migration reviewable in vertical slices, with source-to-target
   inventory, independent specialist review, and explicit acceptance evidence.
5. Keep the current production launcher, QML files, and Quickshell dependency
   available until all parity and rollout gates pass.

## 5. Non-goals

- Switching the production launcher to GPUI-shell during the migration or
  removing QML before the final cutover gate.
- Replacing Niri or broadening the supported compositor set as part of this
  migration. Existing compatibility behavior remains governed by current
  documentation.
- Redesigning the three families, deleting established settings, changing
  config/IPC names, or reducing the supported visual inventory to the surfaces
  already present in the prototype.
- Assuming upstream GPUI-shell is a library, inventing an upstream API, or
  deciding its license on the basis of repository name or visibility.
- Promising arbitrary dynamic plugins in the first complete migration.
- Rewriting unrelated mascot-assistant behavior or other project features.

## 6. Users and key use cases

- A Niri user selects ii, Waffle, or Iris and sees the expected family on every
  configured display after login and restart.
- A user opens bars, docks, launchers, sidebars, settings, notifications,
  wallpaper tools, widgets, and transient/system surfaces and can complete the
  same actions as in the current shell.
- A user changes an existing option in the supported config/settings UI and
  sees the correct result without losing unrelated or unknown config fields.
- A user or installer updates, diagnoses, restarts, or rolls back the shell
  using the existing supported commands and service managers.
- A maintainer ports one surface at a time, can identify the source QML
  contract, and can verify the corresponding GPUI-shell surface in isolation.

## 7. Product and architecture requirements

### 7.1 Upstream integration boundary

- **REQ-UP-1:** Before selecting a code integration method, inspect the
  upstream repository's current structure, entry point, build, runtime,
  extension seams, dependency ownership, update model, and licensing files.
  Before source clearance, inspection of public source files is read-only and
  solely for factual architecture findings; it does not authorize source use
  or derivative work.
- **REQ-UP-1a:** Source adoption is **BLOCKED** until an authoritative source
  license or written grant is obtained and maintainers record a review of
  provenance, revision, scope, and intended use. While blocked, only
  iNToo-authored documentation/contracts and independently written,
  non-derived code may proceed; no GPUI-shell source or source dependency may
  be incorporated.
- **REQ-UP-2:** Record a reviewed integration decision: upstream contribution,
  maintained fork, vendored source, submodule, or another justified approach.
  The decision must state who owns updates and how iNtoo-only changes are
  isolated. It must not masquerade as use of a published crate API unless such
  an API is verified upstream.
- **REQ-UP-3:** Pin a reproducible upstream revision or release and document
  supported toolchain/build prerequisites. Define an update and compatibility
  review process before relying on upstream internals.
- **REQ-UP-4:** Keep the GPUI-shell host responsible for top-level windows,
  Wayland layer-shell ownership, outputs, event loop, and lifecycle. Environment
  implementations contribute bounded content and behavior through the
  integration seam established by the architecture decision.
- **REQ-UP-5:** Reuse an upstream crate only after its license is cleared and
  its responsibilities, dependencies, lifecycle ownership, and API are shown
  to match an iNToo contract. If no suitable crate boundary exists, stop and
  record a go/no-go decision before adapting the upstream host; host changes
  must have bounded ownership and a reviewable upgrade path.
- **REQ-UP-6:** Assign one owner to each process-global service, IPC/DBus name,
  notification source, tray, compositor connection, config writer, and shell
  lifecycle integration in each run mode. Do not start upstream singleton
  integrations that compete with iNToo or the active QML shell.

### 7.2 Shared foundation and environments

- **REQ-ENV-1:** Provide a shared Material foundation for semantic color,
  typography, shape, spacing, motion/effects, reusable controls, focus, and
  accessibility. Resolve source iNToo themes/configuration rather than
  substituting a generic Material look.
- **REQ-ENV-2:** Implement `ii`, `waffle`, and `iris` as isolated environment
  modules/plugins with family-owned composition, style tokens, and local UI
  state. Shared behavior belongs in the foundation/host only where it is
  genuinely common.
- **REQ-ENV-3:** Preserve existing family selection (`panelFamily`), family
  cycle order, transitions, config namespaces, enabled-panel gates, and
  unknown config data. Invalid plugin/family selection must produce a defined
  diagnostic and safe fallback without silently rewriting user config.
- **REQ-ENV-4:** Prevent one family's style selectors or panels from leaking
  into another family. Switching families must dispose old surfaces and input
  regions before presenting the new family.
- **REQ-ENV-5:** A delayed action or async result created by one family must not
  mutate or activate surfaces after a family transition. Tag work with a host
  family/session generation and reject stale callbacks/actions after that
  generation changes.
- **REQ-ENV-6:** Unknown, invalid, or unavailable `panelFamily` values resolve
  deterministically to the Material `ii` environment, emit a diagnostic, and
  leave the persisted source value unchanged. This fallback must not depend on
  registry iteration order or which plugin was registered first.

### 7.3 Visual and behavioral coverage

- **REQ-COV-1:** Trace every current surface and state in
  [GPUI_SURFACE_MAP.md](GPUI_SURFACE_MAP.md) to an implementation and evidence
  record. A list of files or a running demo is not proof of coverage.
- **REQ-COV-2:** Match supported QML behavior for geometry, materials, visible
  content, transitions, interaction regions, open/close lifecycle, and states
  such as loading, empty, unavailable, error, disabled, and failure.
- **REQ-COV-3:** Preserve input semantics: keyboard shortcuts, focus order,
  pointer, wheel, drag/drop, activation, dismissal, and accessibility names and
  states, where currently supported.
- **REQ-COV-4:** Preserve output targeting, hotplug/resize/rename behavior,
  layer anchors, exclusive zones, input regions, transparency, and lock/session
  behavior. Unsupported capabilities must be explicitly surfaced and must not
  be represented with fabricated data or unsafe placeholders.
- **REQ-COV-5:** Provide visual comparison evidence against QML for each family
  and representative supported styles, output scales, and window/data states.
  Screenshots supplement interaction/lifecycle checks; they do not replace
  them.
- **REQ-COV-6:** Preserve translations and existing theme/config controls, and
  ensure settings changes update the relevant rendered surface and persist
  without dropping unrelated configuration.

### 7.4 Compatibility and rollout

- **REQ-COMP-1:** Keep `intoo` commands and IPC targets, names, argument and
  return contracts, keybinds, configuration paths/defaults, and runtime reload
  behavior compatible, except for an explicitly approved and documented
  migration.
- **REQ-COMP-2:** Preserve service behavior for compositor state, audio, media,
  network, notifications, tray, wallpaper, lock/session actions, and all
  documented modules. UI code must consume explicit service states and must
  not invent successful results for unavailable providers.
- **REQ-COMP-3:** Preserve systemd and OpenRC startup, status/logging, graceful
  restart, cleanup, supported package/install/update flows, and rollback.
- **REQ-COMP-4:** During development, GPUI-shell based implementation must run
  side-by-side as a test prototype without taking over the user's production
  shell or competing for config writes, exclusive zones, or service ownership.
- **REQ-COMP-5:** Production cutover requires a tested rollback path. Retain the
  QML payload and Quickshell dependencies until the new runtime has passed the
  complete family, lifecycle, packaging, and parity gates.
- **REQ-COMP-6:** Prototype dual-run is permitted only in an isolated Niri test
  session with an explicit environment/config boundary. Do not run lock,
  authentication, PolicyKit, session shutdown, or other security-sensitive
  replacement surfaces alongside an active user's production session.
- **REQ-COMP-7:** Config handling must distinguish missing, malformed,
  unsupported, and valid snapshots. A malformed reload must keep the last
  known-good state and report the error; a user edit during a pending write
  must not be overwritten. Define a single writer and explicit write handoff
  before enabling replacement settings persistence.
- **REQ-COMP-8:** Bind Niri output/window/workspace identity to validated
  connector identity and a provider generation. On reconnect, output rename,
  or stale snapshot, reconcile surfaces and actions against the current
  generation; never apply an action to an output/window based only on stale
  enumeration order or geometry.

## 8. Staged delivery and acceptance gates

### Stage 0 — Read-only audit and separate adoption gate

Complete a read-only upstream architecture and integration-feasibility audit.
Public source files may be inspected only to record factual findings about
architecture, manifests, and ownership; do not copy, adapt, build, execute,
link, or otherwise incorporate source. Findings do not authorize use or
derivative work. Record the audit in an explicit integration ADR and update
the iNToo source-to-target surface inventory. Obtain authoritative source
licensing or written permission and record maintainer review of provenance,
revision, scope, and intended use before source adoption.

If no authoritative license or written grant is available, keep adoption
blocked. Continue only with iNToo-authored docs/contracts and independently
written, non-derived code; do not start source-dependent integration spikes.

**Gate:** the read-only audit, ADR, and inventory are reviewable regardless of
licensing status. Source adoption, dependency wiring, and source-dependent
integration spikes remain **BLOCKED** until an authoritative license/grant and
recorded review clear that separate gate. The integration plan must not assume
upstream is a library or supports a plugin ABI.

### Stage 1 — Isolated host feasibility

After Stage 0 source clearance, reproduce the pinned upstream build and
integrate the chosen base in a separate development target. Prove Niri/Wayland
layer surfaces, anchors/reservations, focus/input, multiple outputs,
configuration read without competing writes, diagnostics, and clean shutdown
in an isolated Niri test session. Do not exercise security-sensitive
replacement surfaces in an active production session.

**Gate:** isolated dual-run can start and stop without disturbing QML, taking
over security/session behavior, or competing for global service ownership; all
unsupported host capabilities are listed with owners and resolution plans.

### Stage 2 — Material foundation and plugin scaffold

Establish host-owned lifecycle APIs and the shared Material foundation. Add
three built-in environment implementations and resolve persisted family IDs.
No family is considered ported merely because a placeholder plugin exists.

**Gate:** plugin isolation, family selection/fallback, config namespaces, and
surface ownership pass review; each family can render its first representative
surface without taking lifecycle ownership from the host. Invalid family IDs
always resolve to `ii` with a diagnostic and no config rewrite; stale actions
from the prior family generation are rejected.

### Stage 3 — Complete visual migration by vertical slices

Migrate all P0–P3 surfaces from the surface map, grouped by family and shared
services. Each slice includes UI, state, configuration/service binding,
accessibility, error/empty states, regression checks, and specialist review.
Track partial/unavailable behaviors explicitly until parity is demonstrated.

**Gate:** every inventory item has a passing implementation/evidence link or a
documented, explicitly approved exception. No surface is omitted because the
prototype currently lacks an adapter.

### Stage 4 — Parity and runtime acceptance

Run paired QML and replacement checks in a real supported Niri session, across
all three families, configured output arrangements, representative styles,
inputs, service states, and startup/shutdown/failure paths. Exercise automated
config/IPC/payload/lifecycle checks and manual visual, keyboard, pointer,
accessibility, and multi-output checks.

**Gate:** all acceptance criteria in `docs/GPUI_ACCEPTANCE.md` pass with
surface-specific evidence, and all runtime, lifecycle, side-by-side, and
rollout criteria in `docs/GPUI_SHELL_MIGRATION_ACCEPTANCE.md` pass with
reviewable evidence. Remaining differences are either fixed or formally
approved as compatibility changes. A failed runtime check cannot be waived by
a static build/test result.

### Stage 5 — Controlled production cutover

Update launcher, service ownership, packaging, dependencies, install/update,
and migrations only after Stage 4. Roll out with a reversible switch and retain
the QML runtime as rollback through the agreed stabilization period. Remove
QML/Quickshell only after rollback is no longer required and all supported
distribution paths have been verified.

**Gate:** all required surface evidence in `docs/GPUI_ACCEPTANCE.md` and
runtime/lifecycle/rollout criteria in
`docs/GPUI_SHELL_MIGRATION_ACCEPTANCE.md` pass. Clean install, upgrade,
restart, diagnostics, and rollback pass on supported service managers and
package paths; release owner confirms cutover evidence and compatibility notes.

Rollback triggers and the post-cutover stabilization period must be set before
production rollout. They must be measurable from runtime/release evidence and
include an owner and a tested return path; this PRD does not invent numeric
thresholds or a duration.

## 9. Success measures

- **Coverage:** 100% of in-scope surfaces/states in the source inventory have a
  target implementation and a recorded acceptance result. “Not yet ported” is
  visible in the tracker and does not count as completion.
- **Parity:** all critical behavior and visual acceptance criteria pass for
  ii, Waffle, and Iris in paired runtime checks; no unresolved P0/P1 regression
  remains at cutover.
- **Compatibility:** existing config and IPC fixtures remain compatible, and
  supported startup, update, restart, and rollback workflows complete.
- **Isolation:** during migration, the production QML launcher remains
  unchanged and remains usable as fallback.
- **Maintainability:** upstream base revision, local patch set, plugin
  boundaries, and update procedure are documented and reproducible.

## 10. Risks and dependencies

| Risk/dependency | Effect | Required response |
|---|---|---|
| Upstream is an application rather than a published library | May require fork, patching, or restructuring instead of a normal dependency | Complete Stage 0 ADR; do not infer API or plugin support |
| Authoritative source license/grant and recorded review are absent | Source adoption, dependency wiring, and source-dependent integration are BLOCKED | Keep the gate closed; proceed only with iNToo-authored docs/contracts and independently written non-derived code |
| Upstream internals or APIs change | Local integration can break on updates | Pin revision, isolate adapters, define update checks and regression gate |
| Surface inventory is large and spans shell/security/system behavior | Visual-only port could miss critical workflows | Use `GPUI_SURFACE_MAP.md` and `GPUI_ACCEPTANCE.md` as complete inventories; specialist review per slice |
| Existing standalone GPUI work may not map cleanly to upstream architecture | Reuse could add parallel lifecycle or duplicated host code | Reassess each module after Stage 0; port semantics, not structure, where needed |
| GPUI/Wayland host capabilities may differ for lock, tray, screenshots, or input | Some QML behaviors may need host extensions or explicit platform adapters | Prototype capability seams early and keep unsupported state explicit |
| Config is shared with the active runtime | Concurrent writers could corrupt or overwrite user choices | Read-only side-by-side by default; single writer ownership and atomic compatibility behavior before settings writes |
| Screenshot similarity can hide interaction and lifecycle gaps | False parity signal | Combine capture comparison with input, focus, output, service, and failure-state checks |
| Production switch alters startup and package ownership | Desktop may fail to start or rollback | Stage 5 only after real session and package/rollback gates |
| Upstream and iNToo both own global services or singleton integrations | Duplicate daemons, names, panels, or conflicting session behavior | Require an ownership matrix and isolated dual-run; disable integrations without an iNToo owner decision |
| Niri identity changes or provider reconnect leaves stale references | Surfaces may be misplaced or actions may target the wrong object | Reconcile validated connector IDs and provider generations; reject stale callbacks/actions |
| Cutover rollback has vague or unmeasured triggers | A failing release may persist too long or roll back inconsistently | Decide measurable triggers, decision owner, and stabilization duration before rollout |

## 11. Open decisions

1. Obtain authoritative source-code licensing or a written grant from the
   copyright holder and record maintainers' review; until then, source adoption
   remains blocked.
2. Once cleared, which integration strategy best fits upstream ownership and
   license: upstream contribution, maintained fork, vendored source, submodule,
   or another option?
3. Which audited upstream crates are actually reusable under their license,
   dependency, and lifecycle contracts? If none fit, is bounded host adaptation
   justified, or is the integration a no-go?
4. What exact public/internal seams can upstream support without carrying a
   permanent local rewrite? What is the policy for upstream updates?
5. Does upstream offer any documented plugin mechanism? If not, confirm the
   built-in Rust module/trait boundary and reject dynamic loading for this
   migration unless separately justified.
6. Which GPUI-shell repository/revision and Rust/GPUI toolchain will be pinned
   for the first reproducible prototype?
7. Which host gaps need upstream changes versus iNToo-owned Wayland/platform
   adapters (layer shell, lock, tray, capture, focus, output identity)?
8. What config snapshot/reload/write policy prevents malformed or stale data
   from replacing last-known-good state or overwriting concurrent user edits?
9. What measurable conditions trigger rollback, who makes the decision, and
   what stabilization duration must pass before QML removal is considered?
10. How long must the Quickshell fallback remain after production cutover, and
    which release owner signs off its removal?

These are architecture and delivery decisions for the next roles; they do not
reduce the committed end state: full iNToo visual and behavioral migration with
parity evidence and a safe production cutover.

## 12. Source references

- [Repository guidelines](../AGENTS.md)
- [Architecture](../ARCHITECTURE.md)
- [GPUI migration plan](GPUI_MIGRATION.md)
- [GPUI visual acceptance criteria](GPUI_ACCEPTANCE.md)
- [GPUI-shell migration runtime acceptance criteria](GPUI_SHELL_MIGRATION_ACCEPTANCE.md)
- [GPUI surface map](GPUI_SURFACE_MAP.md)
- [Panel families](PANEL_FAMILIES.md)
- [Configuration system](CONFIG_SYSTEM.md)
- [IPC contract](IPC.md)
- [Runtime lifecycle](RUNTIME.md)
- [GPUI environment plugin contract](GPUI_ENVIRONMENT_PLUGIN_CONTRACT.md)
- [GPUI-shell architecture ADR](GPUI_SHELL_ARCHITECTURE.md)
- [GPUI-shell migration stories](GPUI_SHELL_MIGRATION_STORIES.md)
- [Upstream GPUI-shell repository](https://github.com/andre-brandao/gpui-shell)
