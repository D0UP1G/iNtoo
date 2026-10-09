pragma Singleton
pragma ComponentBehavior: Bound

import QtQuick
import Quickshell
import Quickshell.Io
import qs.modules.common

Singleton {
    id: root
    property bool opened: false
    property bool busy: false
    property string status: ""
    property string requestData: ""
    property bool receivedReply: false
    property bool receivedEvent: false
    property var messages: []

    function append(role: string, text: string): void {
        root.messages = root.messages.concat([
            {
                role: role,
                text: text
            }
        ]).slice(-40);
    }

    function send(message: string, includeHistory: bool, allowActions: bool): bool {
        if (root.busy || !message.trim())
            return false;
        root.requestData = JSON.stringify({
            message: message.trim(),
            conversation: root.messages.slice(-20),
            includeHistory: includeHistory,
            allowActions: allowActions,
            model: Config.options?.mascot?.assistant?.model ?? ""
        }) + "\n";
        root.append("user", message.trim());
        root.busy = true;
        root.receivedReply = false;
        root.receivedEvent = false;
        root.status = Translation.tr("Thinking…");
        worker.stdinEnabled = true;
        worker.running = true;
        return true;
    }

    function cancel(): void {
        if (worker.running)
            worker.signal(15);
    }

    function clear(): void {
        if (!root.busy)
            root.messages = [];
    }

    Process {
        id: worker
        command: ["python3", Directories.scriptsPath + "/ai/mascot-assistant.py"]
        onStarted: {
            worker.write(root.requestData);
            worker.stdinEnabled = false;
            root.requestData = "";
        }
        stdout: SplitParser {
            onRead: line => {
                try {
                    const event = JSON.parse(line);
                    root.receivedEvent = true;
                    if (event.type === "text") {
                        if (!root.receivedReply) {
                            root.append("assistant", event.text);
                            root.receivedReply = true;
                        } else {
                            const copy = root.messages.slice();
                            copy[copy.length - 1] = {
                                role: "assistant",
                                text: copy[copy.length - 1].text + event.text
                            };
                            root.messages = copy;
                        }
                    } else if (event.type === "error") {
                        root.append("error", event.text);
                        root.receivedReply = false;
                    } else if (event.type === "status") {
                        root.status = event.text;
                    }
                } catch (error) {
                    console.warn("[MascotAssistant] Invalid bridge event:", error);
                }
            }
        }
        onExited: code => {
            if (!root.receivedEvent && code !== 0)
                root.append("error", Translation.tr("The assistant could not start. Check Python and OpenCode."));
            root.busy = false;
            root.status = "";
        }
    }
}
