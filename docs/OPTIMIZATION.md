# QML/Quickshell Performance Optimization Guide

Best practices for optimizing iNtoo based on Qt6 QML documentation and KDAB recommendations.

## Quick Reference

| Do                             | Don't                                        |
| ------------------------------ | -------------------------------------------- |
| `property int size: 10`        | `property var size: 10`                      |
| `visible: false`               | `opacity: 0`                                 |
| `anchors.fill: parent`         | `width: parent.width; height: parent.height` |
| `root.myProperty` (qualified)  | `myProperty` (unqualified)                   |
| `asynchronous: true` on images | Sync image loading                           |
| Cache lookups in loops         | Repeated property access                     |

## 1. Type Annotations

Always use concrete types instead of `var`:

```qml
// Bad
property var size: 10
property var items: []

// Good
property int size: 10
property list<string> items: []
```

Annotate function parameters and return types:

```qml
// Bad
function calculate(width, height) {
    return width * height
}

// Good
function calculate(width: real, height: real): real {
    return width * height
}
```

## 2. Qualified Property Lookups

Always qualify property access with object id:

```qml
Item {
    id: root
    property int size: 10

    Rectangle {
        // Bad - unqualified lookup
        width: size

        // Good - qualified lookup
        width: root.size
    }
}
```

## 3. Property Resolution Caching

Cache property lookups outside tight loops:

```qml
// Bad - resolves rect.color 4 times per iteration
for (var i = 0; i < 1000; ++i) {
    printValue("red", rect.color.r)
    printValue("green", rect.color.g)
    printValue("blue", rect.color.b)
    printValue("alpha", rect.color.a)
}

// Good - resolve once, use cached value
var rectColor = rect.color
for (var i = 0; i < 1000; ++i) {
    printValue("red", rectColor.r)
    printValue("green", rectColor.g)
    printValue("blue", rectColor.b)
    printValue("alpha", rectColor.a)
}
```

## 4. Binding Optimization

Use temporary accumulators to avoid intermediate re-evaluations:

```qml
// Bad - triggers 6 binding re-evaluations
for (var i = 0; i < someData.length; ++i) {
    accumulatedValue = accumulatedValue + someData[i]
}

// Good - single re-evaluation at the end
var temp = accumulatedValue
for (var i = 0; i < someData.length; ++i) {
    temp = temp + someData[i]
}
accumulatedValue = temp
```

## 5. Visibility vs Opacity

Use `visible: false` instead of `opacity: 0`:

```qml
// Bad - still renders, just transparent
opacity: 0

// Good - skips rendering entirely
visible: false
```

## 6. Anchors vs Bindings

Prefer anchors for relative positioning:

```qml
// Bad - binding-based positioning
Rectangle {
    x: rect1.x
    y: rect1.y + rect1.height
    width: rect1.width - 20
}

// Good - anchor-based positioning
Rectangle {
    anchors.left: rect1.left
    anchors.top: rect1.bottom
    anchors.right: rect1.right
    anchors.rightMargin: 20
}
```

## 7. Image Loading

Always use async loading and explicit source size:

```qml
Image {
    source: "large-image.png"
    asynchronous: true  // Load in background thread
    sourceSize: Qt.size(200, 200)  // Scale before loading
    cache: true  // Cache decoded image
    smooth: false  // Disable if not needed
}
```

## 8. Text Performance

Use simplest text format possible:

```qml
Text {
    // Best performance
    textFormat: Text.PlainText

    // Only if you need basic formatting
    // textFormat: Text.StyledText

    // Avoid - expensive parsing
    // textFormat: Text.AutoText
    // textFormat: Text.RichText
}
```

## 9. ListView/Delegates

Keep delegates simple and avoid clipping:

```qml
ListView {
    // Buffer delegates outside viewport
    cacheBuffer: 200

    delegate: Item {
        // NEVER clip in delegates
        // clip: true  // BAD!

        // Keep delegate simple
        // Avoid ShaderEffects in delegates
    }
}
```

## 10. Lazy Loading (Quickshell)

Use LazyLoader for small components already compiled with their parent. For large
optional panel sources, use the AsyncPanelLoader pattern below so unselected
source files do not compile before they are requested:

```qml
// LazyLoader properties:
// - active: sync load, destroys on false
// - loading: starts async background load
// - activeAsync: async load, can read/write like active
// - item: accessing forces sync load if not ready

LazyLoader {
    // Load when condition is true
    active: Config.ready && someCondition

    HeavyComponent {}
}
```

**Important**: `loading` only STARTS async load, doesn't KEEP component active. Use `active` to maintain loaded state.

## 11. Clipping

Avoid `clip: true` whenever possible:

```qml
// Bad - increases renderer complexity
Rectangle {
    clip: true
    // ...
}

// Good - use ClippingRectangle only when necessary
// Or restructure to avoid clipping
```

## 12. Null Safety & Config Access

iNtoo's config uses Quickshell's `JsonAdapter` + `FileView`. Every property is declared in `Config.qml` with a typed default. When the user's `config.json` has a value, JsonAdapter reads it; when it doesn't, the schema default applies. The property always exists for declared schema keys.

```qml
// Config access: schema properties are guaranteed by JsonAdapter
property int value: Config.options.bar.cornerStyle  //  always valid

// ❌ Direct assignment: persists to disk via JsonAdapter, but does NOT emit
//    configChanged(). Listeners (settings pages, bar layout, theme reactivity)
//    will not update. This is the #1 silent bug source in iNtoo.
// Config.options.bar.bottom = true

// ✅ Always use setNestedValue(): persists to disk AND emits configChanged()
//    so every listener reacts correctly.
Config.setNestedValue("bar.bottom", true)

// Runtime data: may genuinely be null, USE optional chaining here
property string title: NiriService.activeWindow?.title ?? ""
```

> **Project rule:** always use `Config.setNestedValue("dot.path", value)` for any config write. Direct property assignment (`Config.options.x = y`) skips the `configChanged()` signal. The value reaches disk but the UI and any reactive listeners never see the change.

> **Also:** use `?.` + `??` on config reads in module code. It protects against edge cases like key renames during migrations or malformed user configs. JsonAdapter guarantees schema defaults, but defensive access is the safer habit.

## Quickshell-Specific

### PanelLoader Pattern

```qml
component PanelLoader: AsyncPanelLoader {
    required property string identifier
    property bool extraCondition: true
    requested: Config.ready
        && (Config.options?.enabledPanels ?? []).includes(identifier)
        && extraCondition
}

PanelLoader {
    identifier: "wBar"
    sourcePath: "modules/waffle/bar/WaffleBar.qml"
}
```

AsyncPanelLoader starts asynchronous source compilation only when requested,
creates the panel asynchronously, and releases it when the request ends. Its
`ready` property becomes true after an unrequested loader completes, or after a
requested child is active. Critical hosts aggregate child readiness and propagate
load failures to the shell family fallback.

### StyledImage

Already optimized with `asynchronous: true` by default.

### Appearance System

Use `Appearance.animationsEnabled` and `Appearance.effectsEnabled` to respect user preferences and GameMode.

### Runtime performance controls

On Niri 26.04 or newer, Settings › Effects can delegate supported translucent surfaces to Niri's `ext-background-effect-v1` implementation and avoid a duplicate QML blur pass. The page is shared by ii and Waffle and provides a default backend plus overrides for bars, docks, panels, islands and desktop widgets. `Auto` preserves each global style's intended material; `Wallpaper`, `Compositor` and `Off` are explicit requests.

Native blur is deliberately shape-aware. Rounded rectangles publish their exact item bounds and radius, while the islands bar publishes a union of its five live cards. Complex connected decorations, pill compositions and non-rounded silhouettes use wallpaper blur when an equivalent compositor region cannot be expressed. Disabling compositor blur keeps the selected global style and resolves through its wallpaper or solid fallback.

Launchers, overview, wallpaper pickers and most other heavy panels are created on demand. Their IPC commands remain registered through lightweight routers. Sidebars are the deliberate exception: their fullscreen roots load in the deferred phase, their content waits for the first valid mapped geometry, and both remain resident afterward so rapid close/reopen can reverse one surface without rebuilding its workspace.

Thumbnail jobs launched by the wallpaper and generated-image pickers run in a transient user scope rather than the `inir.service` cgroup. `intoo restart` cancels any unfinished iNtoo thumbnail pool, and completed scopes are collected automatically, so their workers and page cache are not reported as shell memory.

### Stateful visual lifetimes

Most heavy visual trees are disposable, but sidebar workspaces are resident by design. They mount at final geometry, animate by transforming or clipping the same tree, release the fullscreen backdrop input as soon as closing begins, and preserve navigation, searches and bottom-widget state. Settings keeps only the current page and its immediate neighbours rendered; ii/Waffle navigation, theme filters and Gowall editor context live in persistent settings state instead of page delegates.

Blur eligibility is topology-based. A surface must declare an exact `rectangle`, `rounded-rectangle` or `islands-union` region before an explicit compositor backend can be used. Unsupported or morphing silhouettes retain their wallpaper material, and `auto` remains fidelity-first. Explicit Island/Ricelin surfaces own their complete material so global ZZZ, Aurora, Angel or iNtoo chrome cannot leak into the same surface.

Desktop widgets keep decoded wallpaper images warm, but release their per-widget mask and blur framebuffer objects whenever the widget is hidden or the widget power manager pauses visual work. Clocks, Cava and resource sampling continue to use their own visibility and consumer gates, so returning to the desktop restores the same visuals without keeping invisible render targets active.

The bar spectrum uses one shared Cava process regardless of monitor count. Its
`Primary only` mode is the performance default: only the configured primary
output uploads and paints spectrum canvases, while the other bars keep their
normal layout. `All monitors` is available in Settings › Bar › Audio spectrum
for users who prefer matching visualizers on every output. M3 visualizer slots
collapse while the shared service reports no audio signal, so an empty Cava
frame does not reserve bar space.

Game Mode can suspend audio visualizers independently from its other visual
effects. When **Disable audio visualizers** is enabled, shared Cava consumers
unsubscribe and visualizer render loops stop for both manual and fullscreen
Game Mode activation; they resume from the same configuration afterward.


## Measured runtime changes

The changes preserve desktop appearance and public search, service and launcher
behavior. They reduce work in shared matching functions, event bursts and helper
startup. No runtime dependency was added.

### Reproduce the checks

Run the behavioral suites from the repository root:

```sh
make test-audit
make test-local
```

The audit includes native Quickshell checks in temporary HOME/XDG directories,
private PID/network namespaces and a headless compositor. The local suite uses
temporary fixtures and command stubs for installation/session contracts. Neither
command installs this checkout or restarts the user's desktop. `make test-local`
also runs `make build`, which sets script executable permissions.

These commands report benchmark samples without imposing timing thresholds:

```sh
node scripts/test-levendist-performance.js --benchmark
python3 scripts/test-color-generation-performance.py --python /path/to/venv/bin/python3 --benchmark
python3 scripts/test-dictionary-performance.py --benchmark --runs 20
python3 scripts/test-translation-performance.py --benchmark
node scripts/test-resource-frequency.js --benchmark
```

The color comparison requires the existing NumPy, Pillow and materialyoucolor
dependencies. Its default stdlib-only import tests always run; real generator
comparisons explicitly report a skip when the selected interpreter lacks those
dependencies. The measured interpreter in this workspace was
`/home/dpg/.local/state/quickshell/.venv/bin/python3`.

For event burst counts and native scoring/scheduler regressions separately:

```sh
node scripts/test-hyprland-refresh.js
node scripts/test-runtime-performance.js
python3 -m unittest -v scripts/test-shell-startup.py -k native_qml
```

Run timing samples with other benchmarks stopped. Host load, CPU frequency,
interpreter version and caches affect elapsed time. Deterministic operation
counts and output comparisons are the acceptance criteria.

### Recorded measurements

These samples were collected on Linux x86_64 with Qt 6.11.2 and Quickshell 0.3.1.
They measure individual algorithms or helpers, rather than whole-desktop FPS,
idle CPU, memory consumption or total login time.

| Change | Measured workload | Before → after |
| --- | --- | --- |
| Shared Levenshtein matching | Native QML, 100 launcher typo scores, median of 5 samples | 10 → 3 ms |
| Shared Levenshtein matching | Native QML, 2 clipboard scores with one typo, median of 5 samples | 146 → 8 ms |
| Hyprland refresh coalescing | 50 synchronous raw events, modeled Quickshell process semantics | 10 → 5 collector launches; 250 → 5 `running` assignments |
| Lazy color imports | Fixed color, dark tonal-spot and terminal outputs, 7 interleaved cold subprocess samples | 0.4847 → 0.3315 s median |
| Dictionary traversal pruning | Synthetic 21-character OCR text, temporary SQLite, 20 interleaved warm samples | 273 → 23 SELECTs; 36.532 → 2.889 ms median |
| Translation scanner batching | Repository's 18 catalogues, 9 interleaved subprocess samples | 18 → 1 basename helpers; 37.560 → 4.005 ms median |
| Direct CPU frequency query | Real `lscpu` output plus parser, 9 interleaved samples | 13.031 → 8.033 ms median; 4 → 1 executables by command structure |

The matching formulas remain exact. Tests compare a frozen baseline and an
independent full-matrix distance oracle across exhaustive, random and adversarial
UTF-16 inputs. Prefix/suffix trimming, direct window access and bounded distance
work improve relevant workloads; the worst-case asymptotic complexity remains.
Node's unrelated-string microbenchmarks were 2–3% slower in one run, while the
native Qt samples improved. Node timings do not substitute for Qt measurements.

Hyprland's old Process API already queued one rerun while a collector was busy;
the baseline is therefore 10 launches for this synchronous burst, rather than
250. Requests received during collection still cause a subsequent refresh. A
native regression verifies real Process completion and the final published
snapshot. Translation batching uses the project's GNU/Linux tools; `basename
-s` supports multiple inputs and `--` protects the option boundary.

Color tests compare stdout and all seven generated output types byte for byte
for fixed colors, fixed/automatic image schemes, smart mode and source precedence.
Existing warning source locations are normalized. Dictionary tests preserve
complete payloads, term ordering, deinflection and run/offset priorities.

Earlier optimizations also retain behavioral regressions: asynchronous critical
panel readiness for all three panel families, cached launcher ranking, Niri
window-event batching and wallpaper-luminance job deduplication. Comparable
whole-desktop timing measurements for those changes were not collected.

### Further profiling targets

| Target | Measurement needed before another change | Current boundary |
| --- | --- | --- |
| Native visual effects and hidden panels | Same compositor, screens, wallpaper and interaction; frame times and idle CPU/GPU before/after | Headless software startup checks establish loading/readiness, not live visual performance |
| Thumbnail workers | Cold/warm folders of known size; process starts, wall time and shutdown behavior for one versus several workers | Changing the pool strategy needs representative image/video and GnomeDesktop workloads |
| Dictionary searches without a match | Large real dictionaries; query counts and latency for bounded OCR selections | Current exact pruning accelerates found winners; no-match scanning still examines candidates |
| Memory-pressure collection | Per-poll process count and total five-minute idle CPU cost | Existing five-minute cadence makes helper consolidation a small unmeasured saving |

The installed desktop was not updated or restarted for these measurements.
Live figures from another installed checkout cannot establish gains for this
working tree. Further changes should have a measured workload and preserve the
same observable behavior; an absolute optimization maximum is not established.

## Tools

- **QML Profiler** (Qt Creator): Find slow bindings and functions
- **GammaRay**: Analyze QML scenes
- **Hotspot**: CPU profiling
- **Heaptrack**: Memory profiling

## References

- [Qt Quick Performance](https://doc.qt.io/qt-6/qtquick-performance.html)
- [KDAB: 10 Tips for QML Performance](https://www.kdab.com/10-tips-to-make-your-qml-code-faster-and-more-maintainable/)
- [Quickshell LazyLoader](https://quickshell.outfoxxed.me/docs/v0.1.0/types/Quickshell/LazyLoader/)
