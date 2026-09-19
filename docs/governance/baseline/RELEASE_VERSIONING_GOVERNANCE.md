# Delta Release and Compatibility Governance

**Version:** v1.1  
**Status:** Governance Baseline  
**Scope:** Delta Foundation, public SDK/contracts and optional extensions.

## 1. Version authorities

Foundation, SDK/contracts and extensions may have independent semantic versions.
They are not required to release together.

A Foundation patch that does not break public contracts must not force every
extension to publish a new version.

## 2. Public contract compatibility

Extensions declare the Foundation/public-contract range they support. Production
CI resolves a pinned released version or immutable commit/ref inside that range.

Do not make production compatibility depend on an unpinned `main` branch.

Breaking changes to public extension contracts require a major compatibility
boundary or an explicit migration path.

## 3. Foundation release

Foundation release checks cover:

- format/lint/static analysis;
- Rust and application tests;
- public contract tests;
- architecture and license policy;
- Foundation build/package smoke tests.

Foundation release CI never requires private repositories, private package
registries or optional-extension credentials.

## 4. Extension release

An extension release verifies:

- its declared Foundation compatibility;
- public-contract and manifest conformance;
- extension-local unit/integration tests;
- packaging and provenance rules appropriate to that extension.

Latest-main testing may be run separately as an early warning and must not
silently change the production compatibility target.

## 5. Manifest and ABI versions

Wire protocols and manifests carry explicit versions. Unsupported versions fail
closed at the extension boundary without preventing Foundation from starting.

## 6. Release rule

The compatibility contract is public and versioned; implementation repositories
and release cadences remain independent.
