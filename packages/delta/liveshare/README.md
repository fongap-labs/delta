# Delta LiveShare Relay

Delta LiveShare uses a Relay that the user operates. The Relay is a transport
boundary: it can read session data while forwarding it. Do not deploy it on a
host you do not trust.

## Requirements

- Node.js 24.22.3 or 26.13.1 and later supported versions.
- A public HTTPS origin with a valid TLS certificate; the same certificate
  serves the viewer page and WSS connections.
- A 32-byte random owner token. Generate one with
  `pnpm --filter @delta/liveshare relay --generate-owner-token` and store it in
  a secret manager before starting the Relay.
- An absolute state-file path on storage restricted to the Relay operator.

The server refuses to start without TLS material or a public HTTPS origin.
Its default bind address is `127.0.0.1`; set
`DELTA_LIVESHARE_HOST=0.0.0.0` only when the machine should accept public
connections directly.

## Start

Set these environment variables in the Relay service manager:

| Variable | Purpose |
| --- | --- |
| `DELTA_LIVESHARE_OWNER` | Owner credential generated once and rotated by the operator |
| `DELTA_LIVESHARE_STATE` | Absolute path for share metadata |
| `DELTA_LIVESHARE_KEY` | TLS private key path |
| `DELTA_LIVESHARE_CERT` | TLS certificate chain path |
| `DELTA_LIVESHARE_ORIGIN` | Public origin, for example `https://share.example.com` |
| `DELTA_LIVESHARE_HOST` | Optional bind address; defaults to loopback |
| `DELTA_LIVESHARE_PORT` | Optional TLS port; defaults to `8443` |

Then run `pnpm --filter @delta/liveshare relay`. The service has no insecure
HTTP mode, does not load third-party page assets, and does not emit request
access logs. A reverse proxy in front of it must not record authorization
headers or request bodies. Deployments must permit outbound WSS from Delta
Desktop to the Relay.

## Data and access

The state file contains share identifiers, creation and expiry times,
revocation state, and SHA-256 digests of the viewer and Host credentials. It
never contains the credentials themselves or session messages. The Relay
forwards bounded JSON frames in memory and drops them when either side
disconnects. Viewer tokens travel in the URL fragment, are removed from the
address bar on page load, and are sent to the Relay only after the WSS
connection is established. They are not sent in HTTP request paths or queries.

Share links expire within 24 hours and can be revoked. The viewer transport
permits one `follow` request per viewer; the Relay caps active viewer connections
globally and per share. The Delta Host must still enforce its own one-session
read-only allowlist for every request. A Host that restarts does not restore
active shares automatically.

The Relay cannot make its operator blind to transit data. Treat the operator,
host filesystem, TLS private key, secret store, and reverse-proxy configuration
as part of the trust boundary.
