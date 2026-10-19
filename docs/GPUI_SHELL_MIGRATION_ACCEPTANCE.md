# GPUI-shell migration acceptance criteria

**Status:** Verification contract; implementation acceptance is pending.
**Date:** 2026-10-19
**Scope:** Migration of iNToo's visual runtime to a GPUI-shell-based host.

This document turns the migration requirements into observable checks. It is
not evidence that the new runtime exists or has passed. The production
Quickshell/QML runtime remains the reference and rollback path. The current
source-clearance gate blocks importing or linking upstream source; only the
documentation and repository-state checks in **Checks available now** can be
accepted before that gate is cleared.

The target is a shared iNToo Material foundation and three built-in
environment compositions: `ii`, `waffle`, and `iris`. “Plugin” here means an
iNToo-owned built-in Rust module boundary. This contract does not assume an
upstream plugin API or dynamic plugin loading.

## Evidence rules

- Record the scenario ID, result, date, environment, commands or manual steps,
  and links to logs/screenshots or other artifacts. Mark unrun scenarios
  **Pending**, never Passed.
- A build proves buildability only. It does not prove visual, interaction,
  accessibility, lifecycle, or compatibility parity.
- For visual comparisons, capture QML and replacement at the same output,
  resolution, scale, family, configuration, theme, and service/data state.
  Preserve the inputs and captures so the comparison can be repeated.
- A runtime scenario is not applicable until its prerequisite stage is
  complete. A prerequisite failure blocks dependent scenarios; it is not a
  pass or an approved exception.
- Resolve the expected behavior from the source QML composition and the
  linked contracts before recording results. Do not infer omitted behavior
  from an upstream demo or a similarly named upstream surface.

## Checks available now

These checks do not require upstream source incorporation and are the only
acceptance checks that can be completed while licensing clearance is absent.

```gherkin
Feature: Keep the source adoption gate closed until rights are established

  Scenario: Treat missing upstream source terms as a blocking condition
    Given the audited upstream revision is 110f0f7126df8df9c738924b94130a0e0c057a06
    And the reviewed audit records no applicable source license or written grant
    When an implementation task proposes copying, vendoring, linking, forking, or subtree-importing upstream source
    Then the task is blocked before any source or dependency change is made
    And the missing clearance is recorded as the blocking reason
    And the repository's own GPL license is not treated as permission for upstream code

  Scenario: Continue iNToo-authored planning without reproducing upstream code
    Given upstream source clearance is absent
    When maintainers edit iNToo-authored requirements, contracts, test plans, or inventories
    Then those changes contain no copied upstream implementation
    And the production launcher, QML payload, and runtime ownership remain unchanged

  Scenario: Keep target architecture claims within audited evidence
    Given the upstream project is audited as a standalone application
    When a requirement or implementation document describes integration
    Then it does not claim that gpuishell is a published library dependency
    And it does not name an unverified upstream extension or plugin API
    And any environment plugin boundary is described as iNToo-owned and built-in
```

Before source clearance, QA can verify the evidence and repository diff for
these scenarios. A negative-gate check must inspect the proposed change, not
merely rely on a ticket label. The audit is pinned to the revision named above;
any different upstream revision requires renewed provenance and license review.

## Conditional source-gate outcomes

These scenarios require a future licensing outcome and are **Pending** until
that outcome occurs. They are not checks that can be run against the current
uncleared upstream state.

```gherkin
Feature: Resolve source clearance before adopting upstream code

  Scenario: Reopen source work only with recorded authoritative clearance
    Given an authoritative license or written grant covering the intended use and revision is available
    When maintainers record the source, scope, revision, attribution, and review decision
    Then the source adoption gate may be reconsidered
    And no integration method is considered accepted until its separate architecture review is recorded

  Scenario: Verify the complete scope and compatibility of a source grant
    Given a license or written grant is proposed for an upstream revision
    When maintainers review its terms
    Then the evidence identifies the exact covered revisions
    And it explicitly covers the planned modification, derivative-work, and redistribution scope
    And compatibility with iNToo's GPL-3.0 distribution is reviewed and recorded
    And copied assets are cleared under their own applicable terms
    And an unclear term leaves the source gate blocked

  Scenario: Stop source integration when clearance is denied or remains ambiguous
    Given the copyright holder denies permission or the proposed terms remain ambiguous
    When maintainers decide whether to continue the migration
    Then upstream-derived source and dependency integration is rejected
    And an iNToo-authored clean-room implementation or a different cleared foundation is evaluated
    And no implementation detail from uncleared source is copied into that route
```

## Checks after source clearance and integration decision

The following scenarios are **Pending** until the source gate is cleared and a
reviewed integration decision identifies the source revision and method. They
describe required outcomes, not assumed upstream APIs.

```gherkin
Feature: Reproduce the selected gpui-shell base

  Scenario: Build the pinned upstream base from a clean checkout
    Given source clearance and an integration decision are recorded
    And the decision identifies an immutable upstream revision and tested GPUI/Zed revision
    And the supported Rust toolchain and native build prerequisites are documented
    When a maintainer follows the documented clean-checkout build procedure
    Then the selected upstream base builds without unrecorded local edits
    And the resulting revision and dependency resolution match the recorded pins
    And any generated or environment-specific inputs are identified in the build record

  Scenario: Detect a moved or mismatched upstream dependency
    Given a build checkout does not match the recorded upstream or GPUI/Zed pin
    When the reproducibility check runs
    Then it reports the mismatch and does not mark the base build accepted

  Scenario: Confirm that the cleared upstream base has useful, bounded seams
    Given source clearance and a reproducible pinned build are recorded
    When maintainers compare candidate reusable components with iNToo requirements
    Then the decision names the useful seams that can be reused and their owners
    And it bounds the maintained fork and identifies changes that would require ongoing upstream divergence
    And migration is a no-go on this base if no useful seam can be maintained within the reviewed boundary

  Scenario: Revalidate provenance after changing imported upstream revisions
    Given an upstream revision has passed source clearance
    When an update proposes a different upstream revision or copied component
    Then provenance and applicable license scope are checked again before importing it
    And asset terms are checked separately for each newly copied asset set
    And the update remains blocked until the review is recorded

Feature: Integrate an iNToo host without assuming an upstream extension API

  Scenario: Run the iNToo test host alongside the production shell
    Given the pinned base has passed its reproducible build check
    And the host integration uses only seams confirmed by the reviewed decision
    When the iNToo test target starts while the QML shell is active
    Then the test target reports its own startup status and diagnostics
    And it reads iNToo configuration without writing competing changes
    And it does not claim production launcher, service, singleton IPC, or DBus ownership
    And QML remains operable with no changed production startup configuration

  Scenario: Stop the side-by-side host cleanly
    Given the iNToo test host is running beside QML
    When the test host is stopped or encounters a handled startup failure
    Then its surfaces, event subscriptions, and owned resources are released
    And QML remains operable
    And no stale input region, exclusive-zone reservation, or competing service remains

  Scenario: Exercise side-by-side behavior in an isolated Niri session and output fixture
    Given a disposable Niri session and deterministic output fixture are available
    And the fixture records output identity, geometry, and expected surface placement
    When the test host is started, exercised, stopped, and started again
    Then each run uses only the fixture's declared session and outputs
    And QML remains operable throughout the run
    And cleanup leaves no owned process, surface, input region, reservation, or subscription from the prior run
    And repeated startup produces no duplicate surface or service ownership

  Scenario: Prevent side-by-side integrations from claiming global services
    Given the production QML shell owns the current iNToo service integrations
    When the replacement runs in side-by-side mode
    Then it does not register competing DBus names or global sockets
    And it does not launch duplicate daemons or claim singleton integrations
    And every service is classified as read-only, isolated, or unavailable in the side-by-side ownership matrix
    And any service without a safe isolated mode remains disabled in the test host

  Scenario: Report unsupported host capabilities explicitly
    Given a required iNToo capability is not available through the verified host integration
    When a surface requires that capability
    Then the host exposes a defined unavailable or failed state
    And it does not fabricate successful state or silently omit the requirement
    And the capability gap has a recorded owner and resolution plan
```

## Shared Material and environment boundaries

These checks require the iNToo host and plugin scaffold. Run contract checks
before visual surface claims.

```gherkin
Feature: Share Material foundations across three environment compositions

  Scenario: Resolve a shared Material control from iNToo appearance settings
    Given the same valid iNToo theme and appearance configuration is supplied
    When a shared Material control is rendered by each enabled environment
    Then semantic color, typography, spacing, shape, focus, and state roles resolve from iNToo-owned tokens
    And any intentional family override is explicit and scoped to that family
    And the result is compared with the matching QML appearance reference

  Scenario: Keep Material as the shared base rather than a fourth environment
    Given the built-in environment registry is initialized
    When its available environment IDs are inspected
    Then it contains exactly the supported IDs ii, waffle, and iris
    And the shared Material foundation is not selectable as an environment
    And ii remains an environment composition that can use the shared Material foundation

  Scenario Outline: Select a persisted environment using its stable ID
    Given the iNToo configuration contains panelFamily "<family>"
    When the host resolves the active environment
    Then exactly the "<family>" composition is selected
    And other environment-owned surfaces and input regions are not active

    Examples:
      | family  |
      | ii      |
      | waffle  |
      | iris    |

  Scenario: Handle an unknown environment without rewriting user configuration
    Given panelFamily contains an unsupported or unknown value
    When the host resolves the active environment
    Then the documented safe fallback or startup error is exposed
    And a diagnostic identifies the invalid value
    And the original and unrelated configuration fields remain unchanged

  Scenario: Use the documented ii fallback only when it is safe to construct
    Given an unknown family value is present
    And the ii composition and required host services are available
    When the host applies the documented safe fallback
    Then it selects ii and records a diagnostic for the invalid value
    And it does not rewrite panelFamily without the established persistence action
    When ii cannot be constructed
    Then startup fails closed with a diagnostic instead of selecting an arbitrary environment

  Scenario: Retire one environment before presenting another
    Given an environment composition is active
    When a valid family switch is applied
    Then old-family surfaces and input regions are closed before new-family surfaces become active
    And the configured family-cycle order and transition setting are respected
    And the selected family is persisted only through the established iNToo config contract
    And transient surfaces are canceled or closed during the switch
    And pending actions from the retired environment cannot mutate the newly selected environment

  Scenario: Keep environment styles and local state isolated
    Given each environment has its own style configuration and local UI state
    When ii, Waffle, or Iris is selected in turn
    Then only that environment interprets its style identifiers and local state
    And shared state is exposed only through an explicit host or Material contract
    And failure to build one environment does not close unrelated host-owned surfaces
```

## QML, configuration, IPC, services, and lifecycle compatibility

The source contracts are [Configuration system](CONFIG_SYSTEM.md),
[IPC contract](IPC.md), and [Runtime lifecycle](RUNTIME.md). For each migrated
surface, the source QML composition and its gates remain the expected behavior.

```gherkin
Feature: Preserve established configuration and IPC contracts

  Scenario Outline: Handle malformed, partial, and inaccessible configuration safely
    Given the configuration is <condition>
    When the host loads or reloads configuration
    Then it follows the documented validation and fallback policy
    And it preserves the last valid state when the contract requires that behavior
    And it does not silently replace malformed or unreadable user data
    And it reports a diagnostic that identifies the affected configuration scope

    Examples:
      | condition                                      |
      | missing an optional field                      |
      | missing a required field                       |
      | malformed JSON                                 |
      | unreadable due to file permissions             |
      | referenced through a supported symbolic link   |
      | changed rapidly while a prior reload is pending|
      | partially written when the process is interrupted|

  Scenario: Resolve rapid configuration edits in a defined order
    Given a valid configuration reload is pending
    When one or more newer edits arrive before it completes
    Then the host applies the latest complete valid snapshot
    And it does not publish a stale snapshot after a newer one
    And rejected edits leave the last accepted snapshot active and produce a diagnostic

  Scenario: Read and update supported settings without dropping unrelated data
    Given an existing iNToo configuration contains known settings and unknown fields
    When a supported setting is read and changed through the replacement UI
    Then the setting uses the established iNToo key, type, default, and normalization rules
    And unknown and unrelated fields are preserved
    And the updated value is reflected in the relevant surface after the supported reload path

  Scenario: Preserve established IPC targets and results
    Given an existing documented iNToo IPC request is sent to the replacement runtime
    When the request is valid, invalid, or targets an unavailable action
    Then the request name, arguments, response shape, and error semantics match the documented IPC contract
    And unavailable actions do not report success

  Scenario: Present service data without fabricating provider success
    Given a UI surface consumes compositor, audio, media, network, notification, tray, wallpaper, or session state
    When its provider is loading, unavailable, stale, or failed
    Then the surface exposes the corresponding supported state
    And it does not substitute fabricated values or successful actions
    And cached data is retained only where the source contract defines that behavior

  Scenario: Preserve startup, restart, and shutdown ownership
    Given the runtime is launched through a supported iNToo development or production path
    When configuration, services, and outputs become ready or the runtime stops
    Then startup readiness and surface gates follow the documented lifecycle
    And owned services and surfaces are cleaned up on stop or failure
    And the test runtime does not change production service-manager ownership before cutover
```

## Niri and multi-output behavior

These scenarios are eligible only after the host owns a verified output and
Niri state adapter. The adapter's exact implementation/API is an architecture
choice; the observable contract is iNToo-owned.

```gherkin
Feature: Reconcile iNToo surfaces with Niri outputs

  Scenario: Route a surface to its configured Niri output
    Given Niri reports two or more outputs with validated connector identities
    And an iNToo surface is configured for one reported connector
    When the host reconciles output state
    Then that surface is attached to the matching output
    And it does not select an output by enumeration order or geometry alone

  Scenario: Reconcile output lifecycle changes
    Given a supported family has active per-output surfaces
    When an output is added, removed, renamed, resized, or changes scale
    Then the host updates or closes affected surfaces and reservations
    And no surface or input region remains attached to a stale output identity
    And configured stale-output fallback follows the existing iNToo contract

  Scenario: Reconcile Niri output rename and re-add without confusing identities
    Given an output is removed and another output is reported with the same connector name
    When the host processes removal, rename, and re-add events
    Then surface ownership follows validated current output identity and generation
    And surfaces from the removed output are closed before the new identity is attached
    And no surface is retained solely because a connector name or list position was reused

  Scenario: Ignore stale or reordered provider events
    Given the provider has accepted a newer Niri state generation
    When a delayed event from an older generation arrives after reconnect or hotplug
    Then the host does not restore stale output, workspace, or window state
    And the current state is reconciled from the newest valid snapshot

  Scenario: Resolve an action already in flight during output or family change
    Given a Niri action was requested for the currently focused output or window
    When that output is removed or the active family changes before completion
    Then the action is canceled or completed according to the documented action contract
    And it is not redirected to a different output or newly selected environment
    And the UI reports failure or unavailability if completion cannot be confirmed

  Scenario: Preserve per-output workspace and window scope
    Given Niri reports workspaces and windows associated with multiple outputs
    When an output-scoped surface is displayed
    Then it presents only the workspaces and windows allowed by that surface's source contract
    And missing output identity produces an explicit unavailable state
    And it does not display unfiltered data from another output

  Scenario: Handle Niri provider loss and recovery
    Given the initial Niri snapshot is available
    When the event connection is lost and later restored
    Then stale or unavailable compositor state is exposed according to the source contract
    And a new snapshot reconciles surfaces without duplicate subscriptions or stale controls
    And unsupported Niri actions are unavailable rather than reported successful
```

## Surface, state, visual, and accessibility parity

Use [GPUI surface map](GPUI_SURFACE_MAP.md) as the complete inventory and
[existing GPUI acceptance criteria](GPUI_ACCEPTANCE.md) for detailed
surface-specific behavior already specified. A row is complete only with
implementation and evidence, not merely a file or registry entry.

```gherkin
Feature: Track parity for every in-scope QML surface

  Scenario: Trace each inventory item to implementation and evidence
    Given an in-scope row exists in the GPUI surface map
    When a migration slice is proposed for acceptance
    Then its QML source composition, family, configuration gates, and states are identified
    And its replacement implementation and evidence record are linked from the row
    And any unimplemented state remains marked incomplete

  Scenario: Preserve applicable visual and interaction states
    Given a source QML surface is in a supported loading, empty, unavailable, error, disabled, or ready state
    When the same configuration and service state are applied to the replacement
    Then equivalent visible content, geometry, material, transition, and input regions are presented
    And applicable keyboard, pointer, wheel, drag, activation, focus, and dismissal behavior is preserved
    And no unsupported or fabricated state is shown as successful

  Scenario: Compare family visuals under matched conditions
    Given paired QML and replacement captures use the same family, output, resolution, scale, theme, configuration, and data state
    When a reviewer compares each in-scope visual slice
    Then differences in content, geometry, typography, color, materials, and motion are recorded
    And every unresolved difference is fixed or explicitly approved as a compatibility change before cutover

  Scenario: Preserve keyboard and assistive technology semantics
    Given a migrated surface has interactive controls and a defined QML focus/accessibility contract
    When a keyboard-only user navigates and activates the surface
    Then focus order, visible focus, keyboard actions, focus restoration, and dismissal match the supported contract
    And controls expose meaningful accessible names, roles, states, and value changes
    And environment changes do not leave focus in removed content

  Scenario: Test lock and security surfaces only in an isolated session
    Given the replacement has a dedicated disposable Niri test session with no production credentials or active lock ownership
    When lock, PolicyKit, session, or authentication surfaces are exercised
    Then the test cannot lock, unlock, or take over the user's production session
    And credential and authorization results come from the test provider or an explicitly safe test fixture
    And the real-session security and focus contract is separately reviewed before any production cutover

  Scenario: Keep a partially migrated surface out of the completed inventory
    Given one or more required surface states, interactions, outputs, or accessibility behaviors are missing
    When migration progress is reported
    Then the inventory item remains partial or blocked
    And screenshot similarity or successful compilation does not mark it complete
```

## Side-by-side rollout, cutover, and rollback

Production cutover is a later, separately reviewed change. Until every
pre-cutover gate passes, the QML launcher remains authoritative.

```gherkin
Feature: Roll out the replacement without disrupting the production shell

  Scenario: Keep QML as production while the replacement is under test
    Given one or more migration acceptance gates remain pending or failed
    When a developer starts or stops the replacement test target
    Then the production launcher continues to select QML
    And QML configuration, services, and surfaces remain operable
    And the test target does not compete for configuration writes or exclusive zones

  Scenario: Require complete parity evidence before production cutover
    Given all surface-map rows have passing evidence or explicitly approved exceptions
    And family, lifecycle, IPC, configuration, service, accessibility, and Niri multi-output checks pass
    And clean install, upgrade, restart, diagnostics, and rollback pass on supported packaging and service-manager paths
    When a release owner reviews the cutover proposal
    Then every acceptance result and remaining compatibility change is linked and reviewable
    And cutover is rejected while any required P0/P1 check is unresolved

  Scenario: Roll back to QML after a replacement runtime failure
    Given a production cutover has been separately approved and the QML rollback payload is retained
    When the replacement fails a startup, session, upgrade, or runtime health check
    Then the documented rollback procedure restores the QML launcher and its supported service ownership
    And the user's existing configuration remains readable
    And the rollback result and any lost or migrated state are recorded

  Scenario: Apply only a release-approved stabilization policy
    Given the replacement has passed the cutover review
    When the release owner defines the stabilization window and rollback triggers
    Then the window has a measurable duration or release milestone recorded before rollout
    And triggers name observable failures such as startup failure, parity regression, or unrecoverable service/output behavior
    And production rollback is initiated when an approved trigger occurs
    And this criterion remains blocked until the project records its actual duration, thresholds, and responsible owner

  Scenario: Retain QML until rollback is no longer required
    Given the replacement has passed initial cutover
    When the agreed stabilization period or supported distribution validation is incomplete
    Then QML and Quickshell rollback assets remain available
    And their removal is not included in the cutover change
```

## Gate summary and current disposition

| Gate | Required evidence | Current disposition |
|---|---|---|
| Source clearance | Authoritative license or written grant for source/revision, reviewed and recorded | **Blocked**; upstream source must not be incorporated |
| Integration and reproducibility | Reviewed integration decision, immutable upstream and GPUI/Zed pins, documented clean build | Pending source clearance |
| Host feasibility | Isolated Niri host, side-by-side lifecycle, verified output/focus/input/reservation behavior | Pending integration |
| Material and environments | Shared iNToo Material contracts and isolated `ii`, `waffle`, `iris` behavior | Pending host scaffold |
| Full migration parity | All inventory surfaces, states, accessibility, config/IPC/service, visual and Niri evidence | Pending implementation |
| Production rollout | Supported package/install/service-manager, cutover and rollback evidence | Pending complete parity |

The following policy values are product/release decisions, not assumptions this
specification may fill in:

- Configuration behavior for each malformed, partial, unreadable, or
  concurrently edited file state must be linked to an approved iNToo policy;
  until then, the affected compatibility scenario is blocked.
- The side-by-side service ownership matrix must name each current integration
  and its permitted mode before enabling that integration in the test host.
- The safe `ii` fallback must be confirmed against startup behavior and
  configuration compatibility; if ii cannot be built, the host fails closed.
- The release owner must set the stabilization window, measurable rollback
  triggers, and escalation owner before production rollout.
- Security-surface testing requires an isolated test session and test identity;
  no live production credential or lock ownership is an acceptance fixture.

An authorized exception must identify the exact behavior, affected surfaces
and families, user impact, owner, compensating behavior, and approval. A
missing capability or implementation is not an exception by default.

## References

- [Migration PRD](GPUI_SHELL_MIGRATION_PRD.md)
- [Migration user stories and flows](GPUI_SHELL_MIGRATION_STORIES.md)
- [gpui-shell architecture decision](GPUI_SHELL_ARCHITECTURE.md)
- [GPUI migration plan](GPUI_MIGRATION.md)
- [GPUI surface map](GPUI_SURFACE_MAP.md)
- [Existing GPUI surface-specific acceptance criteria](GPUI_ACCEPTANCE.md)
- [Configuration system](CONFIG_SYSTEM.md)
- [IPC contract](IPC.md)
- [Runtime lifecycle](RUNTIME.md)
- [Environment plugin contract](GPUI_ENVIRONMENT_PLUGIN_CONTRACT.md)
