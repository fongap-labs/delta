# AGENTS — Repository scripts

- Gate scripts invoke pnpm without a shell; normalize repository-relative glob paths to `/` at ingestion. Keep platform adaptation in the gate that needs it.
- Source-ownership gates use syntax-aware discovery and reject empty or unexpectedly narrowed corpora. Verify admitted and excluded forms when the detection boundary changes.
- Existing script specs run in forked workers alongside other tests and gate processes. Each spec owns its ports, temporary paths and child processes. A spec that passes only alone is defective. Follow docs/testing.md for execution semantics and central test ownership for changes.
