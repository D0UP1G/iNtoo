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
    property var catalogModels: []
    property bool loadingModels: false
    property string catalogError: ""
    property string catalogRequest: ""
    property bool receivedCatalog: false
    readonly property string configuredProvider: (Config.options?.mascot?.assistant?.provider ?? "").trim().toLowerCase()
    readonly property string configuredModel: (Config.options?.mascot?.assistant?.model ?? "").trim()
    readonly property string providerId: configuredProvider || (configuredModel.includes("/")
        ? configuredModel.split("/")[0] : "ollama")
    readonly property string selectedModel: configuredModel.length > 0
        ? (configuredModel.includes("/") ? configuredModel : providerId + "/" + configuredModel)
        : (providerId === "ollama" ? "ollama/qwen3.5:0.8b" : "")
    readonly property bool modelReady: selectedModel.startsWith(providerId + "/")
        && selectedModel.length > providerId.length + 1
    readonly property var providers: {
        const entries = [
            { value: "ollama", label: "Ollama" },
            { value: "opencode", label: "OpenCode" },
            { value: "github-copilot", label: "GitHub Copilot" },
            { value: "openai", label: "OpenAI" },
            { value: "anthropic", label: "Anthropic" },
            { value: "google", label: "Google" },
            { value: "openrouter", label: "OpenRouter" },
            { value: "groq", label: "Groq" },
            { value: "mistral", label: "Mistral" },
            { value: "deepseek", label: "DeepSeek" }
        ];
        const ids = root.catalogModels.map(model => model.split("/")[0]).concat([root.providerId]);
        for (const value of ids) {
            if (value && !entries.some(entry => entry.value === value))
                entries.push({ value: value, label: value });
        }
        return entries;
    }
    readonly property var modelOptions: root.modelsFor(root.providerId).map(model => ({
        value: model,
        label: model.substring(model.indexOf("/") + 1)
    }))

    function modelsFor(provider: string): var {
        const models = root.catalogModels.filter(model => model.startsWith(provider + "/"));
        if (provider === "ollama" && !models.includes("ollama/qwen3.5:0.8b"))
            models.unshift("ollama/qwen3.5:0.8b");
        if (root.selectedModel.startsWith(provider + "/") && !models.includes(root.selectedModel))
            models.unshift(root.selectedModel);
        return models;
    }

    function selectProvider(provider: string): void {
        if (root.busy)
            return;
        if (!provider) {
            Config.setNestedValue("mascot.assistant.provider", "");
            return;
        }
        if (provider === root.providerId)
            return;
        const model = root.modelsFor(provider)[0] ?? "";
        Config.setNestedValues({
            "mascot.assistant.provider": provider,
            "mascot.assistant.model": model,
            "mascot.assistant.apiBase": "",
            "mascot.assistant.apiFormat": provider === "anthropic" ? "anthropic"
                : provider === "google" ? "google" : "openai"
        });
    }

    function selectModel(model: string): void {
        const value = model.trim();
        if (root.busy || !value)
            return;
        Config.setNestedValue("mascot.assistant.model", value.startsWith(root.providerId + "/")
            ? value : root.providerId + "/" + value);
    }

    function connection(): var {
        return {
            model: root.selectedModel,
            provider: root.providerId,
            apiBase: Config.options?.mascot?.assistant?.apiBase ?? "",
            apiFormat: Config.options?.mascot?.assistant?.apiFormat ?? "openai"
        };
    }

    function refreshModels(): void {
        if (root.loadingModels)
            return;
        root.catalogError = "";
        root.receivedCatalog = false;
        root.catalogRequest = JSON.stringify(root.connection()) + "\n";
        root.loadingModels = true;
        catalogWorker.stdinEnabled = true;
        catalogWorker.running = true;
    }

    onOpenedChanged: if (opened)
        root.refreshModels()

    function append(role: string, text: string): void {
        root.messages = root.messages.concat([
            {
                role: role,
                text: text
            }
        ]).slice(-40);
    }

    function send(message: string, includeHistory: bool, allowActions: bool): bool {
        if (root.busy || !root.modelReady || !message.trim())
            return false;
        root.requestData = JSON.stringify(Object.assign(root.connection(), {
            message: message.trim(),
            conversation: root.messages.slice(-20),
            includeHistory: includeHistory,
            allowActions: allowActions
        })) + "\n";
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
        id: catalogWorker
        command: ["python3", Directories.scriptsPath + "/ai/mascot-assistant.py", "--list-models"]
        onStarted: {
            catalogWorker.write(root.catalogRequest);
            catalogWorker.stdinEnabled = false;
            root.catalogRequest = "";
        }
        stdout: SplitParser {
            onRead: line => {
                try {
                    const event = JSON.parse(line);
                    if (event.type === "catalog") {
                        root.receivedCatalog = true;
                        root.catalogModels = event.models ?? [];
                        root.catalogError = event.error ?? "";
                    } else if (event.type === "error") {
                        root.receivedCatalog = true;
                        root.catalogError = event.text;
                    }
                } catch (error) {
                    console.warn("[MascotAssistant] Invalid model catalog:", error);
                }
            }
        }
        onExited: code => {
            if (!root.receivedCatalog)
                root.catalogError = Translation.tr("Could not load models. Check OpenCode or enter a model name.");
            root.loadingModels = false;
        }
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
                root.append("error", Translation.tr("The assistant could not start. Check Python, OpenCode, and Ollama."));
            root.busy = false;
            root.status = "";
        }
    }
}
