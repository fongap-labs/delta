---
description: "Publish completed Desktop installers and update feeds to the selected download service. This reference covers deployment identity, upload credentials, fixed downloads and audit records. Build configuration and signing operations have separate owners."
---

# Delta releases

The inherited Desktop cloud publishing, website installer, staged update and CDN purge procedures have been removed. Delta release artifacts are published through the central Action Worker using the repository release manifest. Download artifacts from Delta's own GitHub Releases. No automatic update feed is currently configured; upstream publishing credentials are not used.

https://github.com/fongap-labs/delta/releases
