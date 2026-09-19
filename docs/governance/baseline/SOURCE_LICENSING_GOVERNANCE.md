# Delta Foundation Source and Licensing Governance

**Version:** v1.1  
**Status:** Governance Baseline  
**Scope:** Public Delta Foundation source, distributions, dependencies and contributions.

## 1. Foundation license

Delta-owned Foundation source is licensed under Apache License 2.0 unless an
individual file or bundled third-party asset states otherwise.

The root `LICENSE`, `NOTICE`, `THIRD_PARTY_NOTICES.md`, and `LICENSES/`
are the authoritative public licensing files.

## 2. Third-party dependencies

Dependencies keep their upstream licenses. New dependencies must be reviewed for
license compatibility and attribution obligations before release.

Strong-copyleft or source-available dependencies that would impose incompatible
distribution obligations on Foundation require explicit legal review and must
not be added by default.

## 3. Bundled assets

Bundled fonts, icons, source fragments and other assets must retain required
copyright and license notices. Full license text is stored under `LICENSES/`
when redistribution requires it.

## 4. Contributions

Contributors license accepted contributions under the repository's applicable
Foundation license. Contribution policy may require additional provenance or
CLA/DCO controls as the project evolves.

## 5. Optional extensions

Public extension contracts do not determine the license of an independently
distributed extension. An extension's own license and distribution terms are
outside Foundation source licensing, provided it complies with the public
contract and all applicable third-party licenses.

## 6. Release gate

Foundation release CI verifies:

- root license identity;
- required notices;
- known bundled third-party attributions;
- manifest license declarations;
- dependency-policy checks.

Private product licensing, pricing and distribution policy are intentionally not
defined in this public Foundation document.
