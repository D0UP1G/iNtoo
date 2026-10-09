pragma Singleton
pragma ComponentBehavior: Bound

import QtQuick
import Quickshell
import Quickshell.Io
import qs.modules.common
import qs.services

// System memory pressure plus JSGCHeap mapping diagnostics (#164).
// Deleted mappings are reported as counts only; MemAvailable drives low-memory
// behavior because a mapping count does not measure resident memory.
Singleton {
    id: root

    // ── Config ────────────────────────────────────────────────────────────
    readonly property bool enabled: Config.options?.performance?.memoryMonitoring ?? true
    readonly property bool notifyEnabled: Config.options?.performance?.memoryWarningNotification ?? false
    readonly property int deletedMappingsThreshold: Config.options?.performance?.jsgcThreshold ?? 300
    readonly property int checkIntervalMs: 300000  // check every 5 min
    readonly property int lowMemoryThresholdKb: Math.max(256, Number(
        Config.options?.performance?.lowMemoryThresholdMb ?? 768)) * 1024
    readonly property int lowMemoryRecoveryKb: root.lowMemoryThresholdKb + 256 * 1024

    // ── State ─────────────────────────────────────────────────────────────
    property int currentDeletedMappings: 0
    property int currentTotalMappings: 0
    property int memoryAvailableKb: 0
    property int residentSetKb: 0
    property int proportionalSetKb: 0
    property bool lowMemory: false
    property bool notificationShown: false
    property bool userDismissed: false

    // ── Public API ────────────────────────────────────────────────────────
    function forceGc(): void {
        gc()
        _log("gc() forced")
    }

    function restart(): void {
        _log("user requested restart")
        Quickshell.execDetached([
            "/usr/bin/notify-send",
            "iNtoo",
            Translation.tr("Restarting shell..."),
            "-a", "Shell",
            "--hint=int:transient:1",
        ])
        // Small delay so notification shows
        Qt.callLater(() => {
            Quickshell.execDetached(["bash", Quickshell.shellPath("scripts/inir"), "restart"])
        })
    }

    function dismiss(): void {
        root.userDismissed = true
        root.notificationShown = false
        _log("user dismissed memory warning")
    }

    function reset(): void {
        root.userDismissed = false
        root.notificationShown = false
        _log("reset state")
    }

    function getStats(): string {
        return JSON.stringify({
            deletedMappings: root.currentDeletedMappings,
            totalMappings: root.currentTotalMappings,
            threshold: root.deletedMappingsThreshold,
            availableMemoryKb: root.memoryAvailableKb,
            shellRssKb: root.residentSetKb,
            shellPssKb: root.proportionalSetKb,
            lowMemoryThresholdKb: root.lowMemoryThresholdKb,
            lowMemory: root.lowMemory,
            notificationShown: root.notificationShown,
            userDismissed: root.userDismissed,
            enabled: root.enabled,
            notifyEnabled: root.notifyEnabled
        })
    }

    // ── Internal ──────────────────────────────────────────────────────────
    function _log(...args): void {
        if (Quickshell.env("QS_DEBUG") === "1")
            console.log("[MemoryPressure]", ...args)
    }

    function _checkMemoryPressure(): void {
        if (!root.enabled) return
        _mapsReader.running = true
    }

    function _notifyUser(): void {
        if (!root.notifyEnabled) { _log("threshold hit, notification disabled"); return }
        if (root.notificationShown || root.userDismissed) return
        
        root.notificationShown = true
        const availableMb = Math.round(root.memoryAvailableKb / 1024)
        const rssMb = Math.round(root.residentSetKb / 1024)

        Quickshell.execDetached([
            "/usr/bin/notify-send",
            "iNtoo",
            Translation.tr("Available memory is low (%1 MB); iNtoo uses %2 MB. Close unused panels or apps.").arg(availableMb).arg(rssMb),
            "-u", "critical",
            "-a", "Shell",
        ])
        _log("notified user, available:", availableMb, "MB; shell RSS:", rssMb, "MB")
    }

    // ── Timers ────────────────────────────────────────────────────────────
    Timer {
        id: _checkTimer
        interval: root.checkIntervalMs
        repeat: true
        running: root.enabled
        onTriggered: root._checkMemoryPressure()
    }

    // ── Maps reader ───────────────────────────────────────────────────────
    Process {
        id: _mapsReader
        // /proc/$PPID, not /proc/self: this runs in an sh child of the shell, so
        // /proc/self is that sh process (zero JSGCHeap mappings) and the counter
        // always read 0 — the threshold could never trip. $PPID is the shell.
        command: ["sh", "-c", "p=$PPID; awk '/^MemAvailable:/ {print $2}' /proc/meminfo; awk '/^VmRSS:/ {print $2}' /proc/$p/status; awk '/^Pss:/ {print $2}' /proc/$p/smaps_rollup 2>/dev/null; awk '/JSGCHeap.*deleted/ {d++} /JSGCHeap/ {t++} END {print d+0; print t+0}' /proc/$p/maps"]
        stdout: SplitParser {
            property int lineNum: 0
            onRead: line => {
                const val = parseInt(line.trim()) || 0
                if (lineNum === 0) root.memoryAvailableKb = val
                else if (lineNum === 1) root.residentSetKb = val
                else if (lineNum === 2) root.proportionalSetKb = val
                else if (lineNum === 3) root.currentDeletedMappings = val
                else if (lineNum === 4) root.currentTotalMappings = val
                lineNum++
            }
        }
        onExited: (code, status) => {
            _mapsReader.stdout.lineNum = 0
            
            const wasLow = root.lowMemory
            root.lowMemory = wasLow
                ? root.memoryAvailableKb < root.lowMemoryRecoveryKb
                : root.memoryAvailableKb < root.lowMemoryThresholdKb
            if (root.lowMemory !== wasLow)
                _log("low-memory mode", root.lowMemory ? "enabled" : "disabled",
                    "available:", Math.round(root.memoryAvailableKb / 1024), "MB")
            if (root.lowMemory) root._notifyUser()
        }
    }

    // ── IPC ───────────────────────────────────────────────────────────────
    IpcHandler {
        target: "memory"
        function collect(): string { root.forceGc(); return "gc() called" }
        function stats(): string { return root.getStats() }
        function restart(): string { root.restart(); return "restarting..." }
        function dismiss(): string { root.dismiss(); return "dismissed" }
        function reset(): string { root.reset(); return "reset" }
    }

    Component.onCompleted: {
        if (!root.enabled) return
        Qt.callLater(() => {
            _checkTimer.start()
            // Prime it once: the timer's first tick is a full interval away, so
            // without this the service reports 0 mappings for the first 5 minutes
            // after every restart.
            root._checkMemoryPressure()
        })
    }
}
