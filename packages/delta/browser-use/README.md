# Delta Browser

Delta's own browser provider uses the stable Playwright API to drive a visible Microsoft Edge or Google Chrome window. It does not load Harness's experimental browser providers. Enable the `delta-browser-use` profile row to add `browser_control`.

Each Delta session gets a separate browser profile under Delta Data. The browser window and profile are isolated from the user's everyday browser profile. The profile remains after `/browser_control` closes the window so site login state is not silently discarded; remove that session's profile directory from Delta Data to clear it.

Every open, read, click, text entry, key press, screenshot, and close action asks for one-time user approval. Password fields are never filled by the Agent; the user enters credentials directly in the visible browser. Page text and screenshots are marked as untrusted. Downloads are disabled. Only HTTP and HTTPS navigation is accepted. Each page request and WebSocket destination is checked; local and private network addresses are blocked unless the profile is explicitly configured with `allowLocalNetwork: true`. This provider does not replace operating-system network isolation.

The host must have Microsoft Edge or Google Chrome installed. Browser profiles and downloaded page state are user data and belong under Delta's portable `Data` directory.
