#!/usr/bin/env node
// Exercise the actual service methods without a compositor or hardware helpers.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const repository = path.resolve(__dirname, "..");
function source(relative) {
    return fs.readFileSync(path.join(repository, relative), "utf8");
}

// Top-level QML methods end at their own indentation. Remove only signature
// types: their bodies are the production JavaScript, including all branches.
function method(qml, name) {
    const start = qml.indexOf("    function " + name + "(");
    assert(start >= 0, "Missing service method " + name);
    const end = qml.indexOf("\n    }", start);
    assert(end >= 0, "Missing method end " + name);
    const text = qml.slice(start, end + "\n    }".length);
    const body = text.indexOf("{");
    return text.slice(0, body).replace(/:\s*\w+/g, "") + text.slice(body);
}

function fixture(qml, names, state) {
    state.root = state;
    const context = vm.createContext(state);
    vm.runInContext(names.map(name => method(qml, name)).join("\n"), context);
    return state;
}

const niriSource = source("services/NiriService.qml");
function niriFixture() {
    const state = fixture(niriSource, [
        "sortWindowsByLayout", "scheduleWindowsUpdate", "_normalizeWindowFocus",
        "_windowOrderDiffers", "handleWindowsChanged", "handleWindowOpenedOrChanged",
        "handleWindowFocusChanged", "handleWindowClosed", "handleWindowLayoutsChanged",
        "handleWorkspacesChanged", "handleWorkspaceActivated", "handleWorkspaceActiveWindowChanged",
        "handleOutputsChanged", "updateDisplayScales"
    ], {
        windows: [], _pendingWindows: [], _windowsDirty: false,
        _windowOrderDirty: false, _windowSortDirty: false,
        _latestFocusedWindowId: undefined, activeWindow: null,
        workspaces: {1: {id: 1, idx: 1, output: "A"}, 2: {id: 2, idx: 2, output: "A"}},
        outputs: {A: {logical: {x: 0, y: 0, scale: 1}}},
        allWorkspaces: [], focusedWorkspaceIndex: 0, mruWindowIds: [],
        updateCurrentOutputWorkspaces() {}, _scheduleSingleWindowCheck() {},
        fetchOutputs() {},
        orderSignals: 0, windowOrderChanged() { state.orderSignals++; },
        windowsUpdateTimer: {running: false, restart() { this.running = true; }},
        sortCalls: 0
    });
    const sort = state.sortWindowsByLayout;
    state.sortWindowsByLayout = list => { state.sortCalls++; return sort(list); };
    const timerStart = niriSource.indexOf("id: windowsUpdateTimer");
    const bodyStart = niriSource.indexOf("onTriggered: {", timerStart) + "onTriggered: {".length;
    const bodyEnd = niriSource.indexOf("\n        }", bodyStart);
    assert(bodyStart >= 0 && bodyEnd >= bodyStart);
    const flush = vm.runInContext("(function () {" + niriSource.slice(bodyStart, bodyEnd) + "})", vm.createContext(state));
    state.flush = () => {
        state.windowsUpdateTimer.running = false;
        flush();
    };
    return state;
}

function window(id, column, extra = {}) {
    return {id, app_id: "app", title: "title", workspace_id: 1,
        is_floating: false, is_focused: false,
        layout: {pos_in_scrolling_layout: [column, 0]}, ...extra};
}
function ids(windows) { return Array.from(windows, win => win.id); }
function seededNiri() {
    const state = niriFixture();
    state.handleWindowsChanged({windows: [window(2, 2), window(1, 1)]});
    state.flush();
    state.sortCalls = 0;
    state.orderSignals = 0;
    return state;
}

// Snapshots with unchanged keys must still be sorted, regardless of their input order.
{
    const state = seededNiri();
    state.handleWindowsChanged({windows: [window(2, 2), window(1, 1)]});
    state.flush();
    assert.deepEqual(ids(state.windows), [1, 2]);
    assert.equal(state.sortCalls, 1);
}

// Title, focus and size-only layout events publish current data without sorting.
{
    const state = seededNiri();
    state.handleWindowOpenedOrChanged({window: window(1, 1, {title: "new title"})});
    assert.equal(state._pendingWindows[0].title, "new title");
    state.flush();
    assert.equal(state.windows[0].title, "new title");
    state.handleWindowFocusChanged({id: 2});
    assert.equal(state._pendingWindows.find(win => win.id === 2).is_focused, true);
    state.flush();
    assert.equal(state.activeWindow.id, 2);
    state.handleWindowLayoutsChanged({changes: [[1, {
        pos_in_scrolling_layout: [1, 0], window_size: [640, 480]
    }]]});
    state.flush();
    assert.deepEqual(Array.from(state.windows[0].layout.window_size), [640, 480]);
    state.handleWindowLayoutsChanged({changes: []});
    state.flush();
    assert.equal(state.sortCalls, 0);
    assert.equal(state.orderSignals, 0);
    assert.deepEqual(ids(state.windows), [1, 2]);
}

// The pending batch keeps every title/layout/focus update and sorts only once.
{
    const state = seededNiri();
    state.handleWindowOpenedOrChanged({window: window(1, 1, {title: "batched title"})});
    state.handleWindowLayoutsChanged({changes: [[1, {pos_in_scrolling_layout: [3, 0]}]]});
    state.handleWindowFocusChanged({id: 1});
    state.flush();
    assert.deepEqual(ids(state.windows), [2, 1]);
    assert.equal(state.activeWindow.title, "batched title");
    assert.equal(state.sortCalls, 1);
    assert.equal(state.orderSignals, 1);
}

// Map lookup preserves strict id matching, index zero and last-change-wins semantics.
{
    const state = seededNiri();
    state.handleWindowLayoutsChanged({changes: [
        ["1", {pos_in_scrolling_layout: [99, 0]}],
        [999, {pos_in_scrolling_layout: [99, 0]}]
    ]});
    assert.equal(state._windowsDirty, false);
    state.handleWindowLayoutsChanged({changes: [
        [1, {pos_in_scrolling_layout: [4, 0]}],
        [1, {pos_in_scrolling_layout: [0, 0]}]
    ]});
    state.flush();
    assert.deepEqual(ids(state.windows), [1, 2]);
    assert.equal(state.windows[0].layout.pos_in_scrolling_layout[0], 0);
    assert.equal(state.sortCalls, 1);
}

// Membership and matching-key updates still notify ordered consumers.
{
    const state = seededNiri();
    state.handleWindowOpenedOrChanged({window: window(3, 0)});
    state.flush();
    assert.deepEqual(ids(state.windows), [3, 1, 2]);
    state.handleWindowClosed({id: 1});
    state.flush();
    assert.deepEqual(ids(state.windows), [3, 2]);
    state.handleWindowOpenedOrChanged({window: window(2, 2, {app_id: "other"})});
    state.flush();
    assert.equal(state.orderSignals, 3);
}

// A workspace/output sort key may change while a title-only batch is pending.
{
    const state = seededNiri();
    state.windows[1].workspace_id = 2;
    state.handleWindowOpenedOrChanged({window: {...state.windows[0], title: "pending"}});
    state.handleWorkspacesChanged({workspaces: [
        {id: 1, idx: 2, output: "A"}, {id: 2, idx: 1, output: "A"}
    ]});
    state.flush();
    assert.deepEqual(ids(state.windows), [2, 1]);
    assert.equal(state.sortCalls, 1);
    state.sortCalls = 0;
    state.handleWindowOpenedOrChanged({window: {...state.windows[0], title: "pending output"}});
    state.workspaces[2].output = "B";
    state.handleOutputsChanged({outputs: {
        A: {logical: {x: 0, y: 0, scale: 1}}, B: {logical: {x: 100, y: 0, scale: 1}}
    }});
    state.flush();
    assert.deepEqual(ids(state.windows), [1, 2]);
    assert.equal(state.sortCalls, 2); // Immediate output update, then pending batch.
}

// Workspace activation and active-window updates keep focus/state working.
{
    const state = seededNiri();
    state.handleWorkspaceActivated({id: 2, focused: true});
    assert.equal(state.workspaces[2].is_focused, true);
    state.handleWorkspaceActiveWindowChanged({workspace_id: 2, active_window_id: 2});
    assert.equal(state.workspaces[2].active_window_id, 2);
    state.handleWindowFocusChanged({id: null});
    state.flush();
    assert.equal(state.activeWindow, null);
    assert.equal(state.sortCalls, 0);
}

const lumaSource = source("services/WallpaperLuma.qml");
function lumaFixture() {
    return fixture(lumaSource, ["keyFor", "request", "_next"], {
        maps: {}, _failed: {}, _queue: [], cell: 24,
        proc: {running: false, job: null},
        startTimer: {restart() {}}, watchdog: {restart() {}},
        Quickshell: {shellPath: value => value}
    });
}

// Repeated reads during analysis create only one job; another image/size remains queued.
{
    const state = lumaFixture();
    state.request("a.png", 1920, 1080);
    state._next();
    const active = state.proc.job;
    for (let i = 0; i < 10; i++) state.request("a.png", 1920, 1080);
    assert.equal(state._queue.length, 0);
    state.request("b.png", 1920, 1080);
    state.request("a.png", 1280, 720);
    assert.equal(state._queue.length, 2);
    state._next();
    assert.equal(state.proc.job, active);
    state.maps[active.key] = {};
    state.proc.job = null;
    state.proc.running = false;
    state._next();
    assert.equal(state.proc.job.path, "b.png");
}

// Cached/failed jobs are skipped, failed suppression and latest-three retention remain.
{
    const state = lumaFixture();
    state.request("cached.png", 1920, 1080);
    state.request("failed.png", 1920, 1080);
    state.request("next.png", 1920, 1080);
    state.maps[state.keyFor("cached.png", 1920, 1080)] = {};
    state._failed[state.keyFor("failed.png", 1920, 1080)] = true;
    state._next();
    assert.equal(state.proc.job.path, "next.png");
    state.request("failed.png", 1920, 1080);
    assert.equal(state._queue.length, 0);
    for (let i = 0; i < 6; i++) state.request("preview" + i, 1920, 1080);
    assert.deepEqual(Array.from(state._queue, job => job.path), ["preview3", "preview4", "preview5"]);
    state.request("", 1920, 1080);
    state.request("invalid.png", 0, 1080);
    assert.equal(state._queue.length, 3);
}

console.log("Runtime service performance behavior checks passed");
