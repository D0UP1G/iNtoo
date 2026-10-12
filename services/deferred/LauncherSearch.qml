pragma Singleton

import qs.modules.common
import qs.modules.common.models
import qs.modules.common.functions
import QtQuick
import Quickshell
import Quickshell.Io
import qs.services

Singleton {
    id: root

    readonly property bool _debugDedupe: Quickshell.env("QS_LAUNCHER_DEDUPE_DEBUG") === "1"

    property string query: ""
    property string _debouncedQuery: ""
    readonly property string settledQuery: root._debouncedQuery
    
    // Debounce timer for search - prevents lag while typing
    Timer {
        id: debounceTimer
        interval: 80  // 80ms debounce - fast enough to feel responsive
        onTriggered: {
            if (root._debouncedQuery === root.query)
                root._queueMath()
            else
                root._debouncedQuery = root.query
        }
    }
    
    onQueryChanged: {
        root._cancelMath()
        // Immediate update for empty query (instant clear)
        if (query === "") {
            _debouncedQuery = ""
            debounceTimer.stop()
        } else {
            debounceTimer.restart()
        }
    }

    function ensurePrefix(prefix: string): void {
        const prefixes = [
            Config.options?.search?.prefix?.action ?? "/",
            Config.options?.search?.prefix?.app ?? ">",
            Config.options?.search?.prefix?.clipboard ?? ";",
            Config.options?.search?.prefix?.emojis ?? ":",
            Config.options?.search?.prefix?.math ?? "=",
            Config.options?.search?.prefix?.shellCommand ?? "$",
            Config.options?.search?.prefix?.webSearch ?? "?",
        ]
        if (prefixes.some(p => root.query.startsWith(p))) {
            root.query = prefix + root.query.slice(1)
        } else {
            root.query = prefix + root.query
        }
    }

    // All actions are now centralized in GlobalActions service.
    // This property delegates to GlobalActions.searchActions for backward compatibility.
    property var allActions: GlobalActions.searchActions

    property string mathResult: ""
    property int _mathGeneration: 0
    readonly property string _mathPrefix: Config.options?.search?.prefix?.math ?? "="
    readonly property string _mathQuery: {
        const q = root._debouncedQuery
        const clipboard = Config.options?.search?.prefix?.clipboard ?? ";"
        const emoji = Config.options?.search?.prefix?.emojis ?? ":"
        return q.startsWith(clipboard) || q.startsWith(emoji) ? "" : q
    }
    on_MathQueryChanged: root._queueMath()
    on_MathPrefixChanged: root._queueMath()

    function _cancelMath(): void {
        root._mathGeneration++
        mathTimer.stop()
        mathProc.running = false
        root.mathResult = ""
    }

    function _queueMath(): void {
        root._cancelMath()
        if (root._mathQuery !== "" && root.query === root._debouncedQuery)
            mathTimer.restart()
    }

    Timer {
        id: mathTimer
        interval: Config.options?.search?.nonAppResultDelay ?? 150
        onTriggered: {
            if (root.query !== root._debouncedQuery || root._mathQuery === "") return
            let expr = root._mathQuery
            if (expr.startsWith(root._mathPrefix))
                expr = expr.slice(root._mathPrefix.length)
            mathProc.calculateExpression(expr)
        }
    }

    Process {
        id: mathProc
        property list<string> baseCommand: ["qalc", "-t"]
        property string requestQuery: ""
        property int requestGeneration: -1
        function calculateExpression(expression: string): void {
            mathProc.running = false
            mathProc.requestQuery = root._debouncedQuery
            mathProc.requestGeneration = root._mathGeneration
            mathProc.command = baseCommand.concat(expression)
            mathProc.running = true
        }
        stdout: SplitParser {
            onRead: data => {
                if (mathProc.requestGeneration === root._mathGeneration
                    && mathProc.requestQuery === root._debouncedQuery
                    && mathProc.requestQuery === root.query)
                    root.mathResult = data
            }
        }
    }

    // Rank applications only when their query/catalog/search policy changes.
    // Calculator output and translated presentation rows must not repeat ranking.
    readonly property string _appQuery: {
        const q = root._debouncedQuery
        if (q === "" || q.startsWith(Config.options?.search?.prefix?.clipboard ?? ";")
            || q.startsWith(Config.options?.search?.prefix?.emojis ?? ":")) return ""
        return StringUtils.cleanPrefix(q, Config.options?.search?.prefix?.app ?? ">")
    }
    readonly property var _appEntries: root._appQuery === "" ? [] : AppSearch.fuzzyQuery(root._appQuery)
    readonly property var _appResults: {
        const appEntries = root._appEntries
        const seenAppNames = new Set()
        const appResults = []
        for (let i = 0; i < appEntries.length; i++) {
            const entry = appEntries[i]
            const nameKey = (entry?.name ?? "").trim().toLowerCase()
            if (nameKey.length === 0) continue
            if (seenAppNames.has(nameKey)) {
                if (root._debugDedupe)
                    console.log(`[LauncherSearch] dedupe: skipping duplicate app name='${entry?.name ?? ""}' id='${entry?.id ?? ""}' query='${root._appQuery}'`)
                continue
            }
            seenAppNames.add(nameKey)
            appResults.push({
                type: Translation.tr("App"),
                id: entry.id ?? entry.name ?? "",
                name: entry.name,
                iconName: entry.icon,
                iconType: LauncherSearchResult.IconType.System,
                verb: Translation.tr("Launch"),
                comment: entry.comment ?? "",
                runInTerminal: entry.runInTerminal ?? false,
                genericName: entry.genericName ?? "",
                execute: () => { AppSearch.launchEntry(entry) }
            })
        }
        return appResults
    }

    property list<var> results: {
        const q = root._debouncedQuery
        if (q === "") return []

        const clipboardPrefix = Config.options?.search?.prefix?.clipboard ?? ";"
        const emojisPrefix = Config.options?.search?.prefix?.emojis ?? ":"
        const mathPrefix = Config.options?.search?.prefix?.math ?? "="
        const shellPrefix = Config.options?.search?.prefix?.shellCommand ?? "$"
        const webPrefix = Config.options?.search?.prefix?.webSearch ?? "?"
        const actionPrefix = Config.options?.search?.prefix?.action ?? "/"

        // Clipboard search
        if (q.startsWith(clipboardPrefix)) {
            const searchStr = StringUtils.cleanPrefix(q, clipboardPrefix)
            return Cliphist.fuzzyQuery(searchStr).map(entry => {
                return ({
                    rawValue: entry,
                    name: StringUtils.cleanCliphistEntry(entry),
                    verb: Translation.tr("Copy"),
                    type: Translation.tr("Clipboard"),
                    iconName: "content_copy",
                    iconType: LauncherSearchResult.IconType.Material,
                    execute: () => Cliphist.copy(entry)
                })
            }).filter(Boolean)
        }

        // Emoji search
        if (q.startsWith(emojisPrefix)) {
            const searchStr = StringUtils.cleanPrefix(q, emojisPrefix)
            return Emojis.fuzzyQuery(searchStr).map(entry => {
                const emoji = entry.match(/^\s*(\S+)/)?.[1] ?? ""
                return ({
                    rawValue: entry,
                    name: entry.replace(/^\s*\S+\s+/, ""),
                    iconName: emoji,
                    iconType: LauncherSearchResult.IconType.Text,
                    verb: Translation.tr("Copy"),
                    type: Translation.tr("Emoji"),
                    execute: () => { Quickshell.clipboardText = emoji }
                })
            }).filter(Boolean)
        }

        // Build results
        let result = []
        const startsWithNumber = /^\d/.test(q)
        const startsWithMath = q.startsWith(mathPrefix)
        const startsWithShell = q.startsWith(shellPrefix)
        const startsWithWeb = q.startsWith(webPrefix)

        // Math result (priority if starts with number or =)
        const mathObj = ({
            name: root.mathResult,
            verb: Translation.tr("Copy"),
            type: Translation.tr("Math"),
            fontType: LauncherSearchResult.FontType.Monospace,
            iconName: "calculate",
            iconType: LauncherSearchResult.IconType.Material,
            execute: () => { Quickshell.clipboardText = root.mathResult }
        })

        if (startsWithNumber || startsWithMath) {
            result.push(mathObj)
        }

        // Shell command
        const cmdObj = ({
            name: StringUtils.cleanPrefix(q, shellPrefix).replace("file://", ""),
            verb: Translation.tr("Run"),
            type: Translation.tr("Command"),
            fontType: LauncherSearchResult.FontType.Monospace,
            iconName: "terminal",
            iconType: LauncherSearchResult.IconType.Material,
            execute: () => {
                let cmd = q.replace("file://", "")
                cmd = StringUtils.cleanPrefix(cmd, shellPrefix)
                ShellExec.execCmd(cmd)
            }
        })

        if (startsWithShell) {
            result.push(cmdObj)
        }

        // Web search
        const webObj = ({
            name: StringUtils.cleanPrefix(q, webPrefix),
            verb: Translation.tr("Search"),
            type: Translation.tr("Web"),
            iconName: "travel_explore",
            iconType: LauncherSearchResult.IconType.Material,
            execute: () => {
                const searchQuery = StringUtils.cleanPrefix(q, webPrefix)
                const baseUrl = Config.options?.search?.engineBaseUrl ?? "https://www.google.com/search?q="
                Qt.openUrlExternally(baseUrl + encodeURIComponent(searchQuery))
            }
        })

        if (startsWithWeb) {
            result.push(webObj)
        }

        result = result.concat(root._appResults)

        // Actions (built-in + user scripts)
        const actionResults = root.allActions.map(action => {
            const actionStr = `${actionPrefix}${action.action}`
            if (actionStr.startsWith(q) || q.startsWith(actionStr)) {
                return ({
                    name: q.startsWith(actionStr) ? q : actionStr,
                    verb: Translation.tr("Run"),
                    type: Translation.tr("Action"),
                    iconName: "settings_suggest",
                    iconType: LauncherSearchResult.IconType.Material,
                    execute: () => action.execute(q.split(" ").slice(1).join(" "))
                })
            }
            return null
        }).filter(Boolean)
        result = result.concat(actionResults)

        // Add fallbacks if not prefix-specific
        const showDefaults = Config.options?.search?.prefix?.showDefaultActionsWithoutPrefix ?? true
        if (showDefaults) {
            if (!startsWithShell) result.push(cmdObj)
            if (!startsWithNumber && !startsWithMath) result.push(mathObj)
            if (!startsWithWeb) result.push(webObj)
        }

        return result
    }

}
