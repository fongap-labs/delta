# Delta Computer Use

Delta's own Windows provider observes and operates the whole desktop through built-in Windows PowerShell and Win32 `SendInput` APIs. It does not load Harness's experimental CUA bundle or provider.

The first screenshot includes a short-lived `screenId`; clicks and scrolling use coordinates from that exact scaled screenshot. A successful input returns a new screenshot and invalidates the previous screen ID. Approved desktop operations are serialized across sessions because they act on one shared Windows desktop. This reduces stale-coordinate mistakes, but other people and applications can still change the desktop between observation and action.

Every screenshot, window-title listing, click, text entry, key press, and scroll asks for one-time user approval. Screenshots are stored as Delta attachments under Data. Window titles and visible text can contain private information. The Agent must not type passwords, authentication codes, or secrets; the user enters those directly.

The provider runs the fixed script in `src/powershell.ps1` using `powershell.exe` with a structured JSON payload on standard input. It never evaluates model-provided PowerShell and never passes model text as a shell command. Delta CUA is opt-in and Windows-only.
