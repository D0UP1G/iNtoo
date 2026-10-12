#!/usr/bin/env node
// Run the production QML JavaScript with deterministic timers and Process state.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const qml = fs.readFileSync(path.resolve(__dirname, "../services/HyprlandData.qml"), "utf8");
const collectors = [
    ["getClients", "clientsCollector", []],
    ["getMonitors", "monitorsCollector", []],
    ["getLayers", "layersCollector", {}],
    ["getWorkspaces", "workspacesCollector", []],
    ["getActiveWorkspace", "activeWorkspaceCollector", null]
];

function method(name) {
    const start = qml.indexOf("    function " + name + "(");
    assert(start >= 0, "Missing method " + name);
    const end = qml.indexOf("\n    }", start);
    assert(end > start, "Missing method end " + name);
    return qml.slice(start, end + "\n    }".length);
}

function handler(marker, endIndent, after = 0) {
    const start = qml.indexOf(marker, after);
    assert(start >= after, "Missing handler " + marker);
    const end = qml.indexOf("\n" + endIndent + "}", start);
    assert(end > start, "Missing handler end " + marker);
    return qml.slice(start + marker.length, end);
}

function fixture({baseline = false, hyprland = true} = {}) {
    const state = {
        _pendingRefresh: 0,
        CompositorService: {isHyprland: hyprland},
        windowList: [], windowByAddress: {}, addresses: [], monitors: [], layers: {},
        workspaces: [], workspaceById: {}, workspaceIds: [], activeWorkspace: null,
        console: {log() {}},
        refreshTimer: {running: false, starts: 0, start() { this.running = true; this.starts++; }}
    };
    state.root = state;
    const context = vm.createContext(state);
    vm.runInContext([
        "_scheduleRefresh", "_requestRefresh", "_startRefresh", "_flushRefresh",
        "updateWindowList", "updateMonitors", "updateLayers", "updateWorkspaces", "updateAll"
    ].map(method).join("\n"), context);
    const compile = body => vm.runInContext("(function () {" + body + "})", context);
    state.rawEvent = compile(handler("function onRawEvent(event) {", "        "));
    state.initialize = compile(handler("Component.onCompleted: {", "    "));
    const trigger = qml.match(/onTriggered: ([^\n]+)/);
    assert(trigger, "Missing timer handler");
    const flush = compile(trigger[1]);
    assert.match(qml, /id: refreshTimer\s+interval: 0\s+repeat: false/);
    state.tick = () => {
        if (!state.refreshTimer.running) return false;
        state.refreshTimer.running = false;
        flush();
        return true;
    };

    for (const [id, collector, fallback] of collectors) {
        const start = qml.indexOf("id: " + id + "\n");
        const next = qml.indexOf("\n    Process {", start);
        const block = qml.slice(start, next < 0 ? qml.length : next);
        const runningChanged = block.match(/onRunningChanged: ([^\n]+)/);
        assert(runningChanged, "Missing completion scheduling for " + id);
        const changed = vm.runInContext("(function (running) {" + runningChanged[1] + "})", context);
        const collect = compile(handler("onStreamFinished: {", "            ", start));
        state[collector] = {text: JSON.stringify(fallback)};
        // Quickshell Process::setRunning(true) keeps targetRunning while busy.
        // Process::onFinished() parses stdout, emits runningChanged, then starts
        // that single queued rerun. Model this rather than counting assignments.
        state[id] = {
            _running: false, targetRunning: false, starts: 0, assignments: 0,
            get running() { return this._running; },
            set running(value) {
                assert.equal(value, true, "Collectors should only be started");
                this.assignments++;
                this.targetRunning = value;
                this.startIfReady();
            },
            startIfReady() {
                if (this._running || !this.targetRunning) return;
                this.targetRunning = false;
                this._running = true;
                this.starts++;
                if (!baseline) changed(true);
            },
            finish(text = JSON.stringify(fallback), {failedToStart = false} = {}) {
                assert(this._running, "Finishing idle collector " + id);
                this._running = false;
                if (!failedToStart) {
                    state[collector].text = text;
                    collect();
                }
                if (!baseline) changed(false);
                if (!failedToStart) this.startIfReady();
            }
        };
    }
    if (baseline) {
        // The old updateAll() directly set running=true on these five collectors.
        state.updateAll = () => {
            if (!state.CompositorService.isHyprland) return;
            for (const [id] of collectors) state[id].running = true;
        };
    }
    state.starts = () => collectors.reduce((sum, [id]) => sum + state[id].starts, 0);
    state.assignments = () => collectors.reduce((sum, [id]) => sum + state[id].assignments, 0);
    state.drain = () => {
        for (let turn = 0; turn < 100; turn++) {
            state.tick();
            const running = collectors.filter(([id]) => state[id].running);
            if (running.length === 0 && !state.refreshTimer.running) return;
            for (const [id] of running) state[id].finish();
        }
        assert.fail("Refresh queue did not settle");
    };
    return state;
}

// A synchronous burst starts one batch. The actual previous behavior starts a
// batch immediately, then implicitly queues one rerun on each busy Process.
const burstSize = 50;
const before = fixture({baseline: true});
const after = fixture();
for (let i = 0; i < burstSize; i++) {
    before.rawEvent();
    after.rawEvent();
}
assert.equal(before.starts(), 5);
assert.equal(after.starts(), 0);
assert.equal(after.refreshTimer.starts, 1, "Requests must not restart the timer");
before.drain();
after.drain();
assert.equal(before.starts(), 10);
assert.equal(before.assignments(), burstSize * 5);
assert.equal(after.starts(), 5);
assert.equal(after.assignments(), 5);
assert.equal(after._pendingRefresh, 0);

// Each public entrypoint retains its collector scope, including both workspace
// collectors. Independent requests share a tick without widening their scope.
for (const [entrypoint, expected] of [
    ["updateWindowList", ["getClients"]], ["updateMonitors", ["getMonitors"]],
    ["updateLayers", ["getLayers"]], ["updateWorkspaces", ["getWorkspaces", "getActiveWorkspace"]]
]) {
    const state = fixture();
    state[entrypoint]();
    state[entrypoint]();
    state.tick();
    assert.deepEqual(collectors.filter(([id]) => state[id].running).map(([id]) => id), expected);
    state.drain();
    assert.equal(state.starts(), expected.length);
}
{
    const state = fixture();
    state.updateWindowList();
    state.updateLayers();
    state.tick();
    assert.equal(state.starts(), 2);
    assert.equal(state.getMonitors.running, false);
}

// Events during collection leave exactly one follow-up per collector, publish
// the final snapshot, and settle after the last event even with staggered exits.
{
    const state = fixture();
    state.initialize();
    state.tick();
    for (let i = 0; i < burstSize; i++) state.rawEvent();
    state.tick();
    assert.equal(state.starts(), 5);
    assert.equal(state.refreshTimer.running, false, "Busy collectors must not poll");
    for (const [id] of collectors) {
        state[id].finish();
        assert.equal(state.refreshTimer.running, true, "Completion must wake pending refresh");
        state.tick();
        assert.equal(state[id].starts, 2);
    }
    state.getClients.finish('[{"address":"0xfinal","workspace":{"id":2}}]');
    state.getMonitors.finish('[{"id":2,"name":"final-monitor"}]');
    state.getLayers.finish('{"final-monitor":{"levels":{}}}');
    state.getWorkspaces.finish('[{"id":2,"name":"final-workspace"}]');
    state.getActiveWorkspace.finish('{"id":2,"name":"final-workspace"}');
    assert.equal(state.starts(), 10);
    assert.equal(state.windowByAddress["0xfinal"].workspace.id, 2);
    assert.deepEqual(Array.from(state.addresses), ["0xfinal"]);
    assert.equal(state.monitors[0].name, "final-monitor");
    assert("final-monitor" in state.layers);
    assert.equal(state.workspaceById[2].name, "final-workspace");
    assert.deepEqual(Array.from(state.workspaceIds), [2]);
    assert.equal(state.activeWorkspace.id, 2);
    assert.equal(state._pendingRefresh, 0);
    assert.equal(state.refreshTimer.running, false);
}

// The two workspace collectors can finish independently; a slow one must not
// block the other or lose the request that arrived while it was running.
{
    const state = fixture();
    state.updateWorkspaces();
    state.tick();
    state.updateWorkspaces();
    state.tick();
    state.getActiveWorkspace.finish();
    state.tick();
    assert.equal(state.getActiveWorkspace.starts, 2);
    assert.equal(state.getWorkspaces.starts, 1);
    state.getActiveWorkspace.finish();
    state.tick();
    assert.equal(state.refreshTimer.running, false);
    state.getWorkspaces.finish();
    state.tick();
    state.getWorkspaces.finish();
    assert.equal(state.starts(), 4);
    assert.equal(state._pendingRefresh, 0);
}

// Compositor guards apply to initialization, every entrypoint and an already
// scheduled tick. Hyprland event names remain unfiltered.
{
    const state = fixture({hyprland: false});
    state.initialize();
    for (const name of ["updateAll", "updateWindowList", "updateMonitors", "updateLayers", "updateWorkspaces"])
        state[name]();
    state.rawEvent();
    assert.equal(state._pendingRefresh, 0);
    assert.equal(state.refreshTimer.running, false);
    assert.equal(state.starts(), 0);
    state.CompositorService.isHyprland = true;
    state.rawEvent();
    state.CompositorService.isHyprland = false;
    state.tick();
    assert.equal(state.starts(), 0);
    assert.equal(state._pendingRefresh, 0);
    state.CompositorService.isHyprland = true;
    state.updateAll();
    state.drain();
    assert.equal(state.starts(), 5);
}

// Malformed output preserves the existing fallback and lookup behavior; a
// pending refresh still runs after parsing fails, then publishes valid data.
{
    const state = fixture();
    state.updateAll();
    state.tick();
    state.rawEvent();
    state.tick();
    for (const [id] of collectors) state[id].finish("invalid JSON");
    assert.equal(state.windowList.length, 0);
    assert.equal(Object.keys(state.windowByAddress).length, 0);
    assert.equal(state.addresses.length, 0);
    assert.equal(state.monitors.length, 0);
    assert.equal(Object.keys(state.layers).length, 0);
    assert.equal(state.workspaces.length, 0);
    assert.equal(Object.keys(state.workspaceById).length, 0);
    assert.equal(state.workspaceIds.length, 0);
    assert.equal(state.activeWorkspace, null);
    state.tick();
    state.getClients.finish('[{"address":"0xrecovered"}]');
    state.getMonitors.finish('[{"name":"recovered"}]');
    state.getLayers.finish('{"recovered":{}}');
    state.getWorkspaces.finish('[{"id":3,"name":"recovered"}]');
    state.getActiveWorkspace.finish('{"id":3}');
    assert.equal(state.starts(), 10);
    assert.equal(state.windowByAddress["0xrecovered"].address, "0xrecovered");
    assert.equal(state.monitors[0].name, "recovered");
    assert("recovered" in state.layers);
    assert.equal(state.workspaceById[3].name, "recovered");
    assert.equal(state.activeWorkspace.id, 3);
    assert.equal(state._pendingRefresh, 0);
}

// FailedToStart emits runningChanged without streamFinished/onExited. A pending
// request can recover, while an unrequested failure must not retry forever.
{
    const state = fixture();
    state.updateWindowList();
    state.tick();
    state.updateWindowList();
    state.tick();
    state.getClients.finish(undefined, {failedToStart: true});
    state.tick();
    assert.equal(state.getClients.starts, 2);
    state.getClients.finish(undefined, {failedToStart: true});
    assert.equal(state.refreshTimer.running, false);
    assert.equal(state._pendingRefresh, 0);
    state.updateWindowList();
    state.tick();
    state.getClients.finish('[{"address":"0xretry"}]');
    assert.equal(state.windowByAddress["0xretry"].address, "0xretry");
}

console.log(`PASS: ${burstSize}-event burst: modeled previous launches ${before.starts()} -> ${after.starts()}, running assignments ${before.assignments()} -> ${after.assignments()}; in-flight refresh, guards and parse recovery passed`);
