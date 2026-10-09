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

Connect your preferred provider:

```bash
opencode auth login
```

The companion uses OpenCode's configured model. Override it in Settings → Mascot,
or set `mascot.assistant.model` to a `provider/model` identifier in
`~/.config/inir/config.json`. An empty value uses OpenCode's default.

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

OpenCode still uses its normal provider configuration and stores its sessions
locally. The desktop transcript is in memory, but sending a message can persist
it in OpenCode's session store and send it to your configured model provider.
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
