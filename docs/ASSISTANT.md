# Desktop companion

Press **Super+S** to open iNtoo's mascot and compact chat on the focused Niri output.
The same companion works in Material ii, Waffle, and Island. Escape, the close
button, or Super+S closes it. Replies continue while the window is closed; Stop
cancels the current request. Chat remains in memory until Clear or a shell restart.

The dependency installation phase installs OpenCode 1.x through the official
`opencode-ai` npm package in a user-owned prefix. On Gentoo, Portage installs
`net-libs/nodejs` with the `npm` USE flag first. Existing OpenCode installations
are kept. `--skip-deps` also skips OpenCode installation. Package-managed installs
must supply OpenCode themselves. OpenCode 2 is not supported by this adapter.

The companion defaults to the local Ollama model `qwen3.5:0.8b` (0.8B parameters).
On the first message, it starts Ollama if needed and downloads the model automatically.
The download is about 1.2 GB and requires internet access; inference then runs locally
without an API key. `./setup install` offers to install [Ollama](https://ollama.com/download);
with `--skip-deps`, install it separately. Override the model in Settings → Mascot, or set
`mascot.assistant.model` to a `provider/model` identifier in
`~/.config/inir/config.json`.

Settings → Mascot also chooses the provider independently of the model. Set
`mascot.assistant.provider` to a provider id (for example `openai`, `anthropic` or
`google`) and optionally `mascot.assistant.apiBase` to any OpenAI-compatible base URL;
`mascot.assistant.apiFormat` picks the protocol (`openai`, `anthropic` or `google`).
An empty provider keeps the provider encoded in the `provider/model` string. Credentials
stay in OpenCode's own store — run `inir assistant auth` once per provider.

**Command history** attaches up to 20 saved commands per available Bash, Zsh or
Fish history file to the next request. The first-message shortcut enables this
option and asks the assistant to explain the commands. Terminal output and exit
status are not captured. Open Bash/Zsh sessions may not have saved their newest
commands; `history -a` in Bash or `fc -AI` in Zsh saves them. Commands that look
like they contain credentials are omitted, but inspect your history before
sharing it with a remote model. Leave the option off to chat without history.

**Allow actions** enables OpenCode's shell and file tools for one message. It resets
after sending. The companion can run commands and edit files as your user; give
a concrete request such as “create a folder named Notes in my home directory.”
Chat mode disables tools, including tools inherited from the global OpenCode
configuration. An isolated workspace avoids loading project instructions. The
assistant reports when an operation needs an interactive command or root access.

The assistant uses its own OpenCode database at
`$XDG_STATE_HOME/inir/assistant/opencode.db` (or
`~/.local/state/inir/assistant/opencode.db`). This keeps it separate from
OpenCode 2.x databases, whose schema is incompatible with the 1.x CLI used by
the assistant. The desktop transcript is in memory, but sending a message can persist
it in the assistant's OpenCode database and send it to the selected model provider.
Requests are not published with OpenCode's share feature.

For existing installations, migration `046-mascot-assistant-keybind` adds the
shortcut only when Super+S is free throughout the Niri include tree. Custom
bindings are preserved. You can also open the assistant directly:

```bash
inir assistant toggle
```

References: [OpenCode installation](https://opencode.ai/docs/),
[CLI](https://opencode.ai/docs/cli/), and
[permissions](https://opencode.ai/docs/permissions/).
