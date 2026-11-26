# Delta design resources

English | [中文](design-system.zh.md)

This guide defines where shared visual, copy, and brand resources live. It complements [Web UI styling](web-styling.md) and the generated [design token catalog](design-token-catalog.md). The refactor is intended to preserve current appearance and behavior; visual changes require a separate product decision.

## Sources of truth

| Resource | Authority | Consumers |
| --- | --- | --- |
| Shared design tokens and theme mappings | `packages/client/ui-theme/src/styles/` | Client CSS Modules, shared primitives, plugin surfaces |
| Product UI copy | Each feature package's locale resources; shared terms in `packages/client/i18n/` | Typed translators and localized component props |
| Product name and official logo assets | `packages/client/ui-brand-official/` | Shells and product surfaces |
| Cross-feature controls and interaction patterns | `packages/client/ui-primitives/` | Feature packages and plugins |

Platform protocol strings, package names, environment variables, persisted keys, API fields, third-party names, and historical migration identifiers are technical identifiers. Keep them stable unless a compatibility migration is explicitly intended. Brand consistency applies to user-facing copy and assets, not to mechanical replacement of those identifiers.

## Token layers

- **Primitive:** raw palette steps and base scales, owned by the theme package. Feature styles do not consume `--dsw-static-*` values.
- **Semantic:** purpose-named `--dsw-alias-*` values, mapped for light and dark themes in the theme source. Feature styles consume these values instead of duplicating colors or theme branches.
- **Component or surface:** `--dsw-specific-*` values for a real, narrowly scoped contract that cannot be represented by an existing semantic token. Prefer semantic tokens and shared primitive props first.
- **Shared geometry and typography:** use the existing `--dsw-radius-*`, `--dsw-font-*`, elevation, and theme scale values where their meaning matches. Truly local geometry may remain in a CSS Module when it is intrinsic to that component and is not a repeated system-wide value.

Before adding a token, search the [catalog](design-token-catalog.md) and theme sheets for an existing semantic match. Define shared values once in their owning theme sheet. Add a component override only with a documented scope and fallback. Do not add a second theme map or use a token alias as an unrelated shortcut.

`pnpm run verify-design-token-contract` checks that owned token references resolve, shared names have a single stylesheet owner, global definitions remain in `ui-theme`, allowed component rebinding stays explicit, token references are acyclic, and the generated catalog matches source. Run `pnpm run generate-design-token-catalog` after adding or removing token names, then inspect the catalog diff.

## Copy and terminology

Static interface labels, actions, placeholders, status messages, and errors belong in locale dictionaries and are read through the typed translator. Put shared product terminology in the shared i18n resource. Use parameterized messages for dynamic values instead of assembling translated fragments in the component. User input, model output, and external content remain data and should not be moved into locale dictionaries.

The `verify-client-ui-i18n` gate checks feature UI source for hard-coded copy, while locale parity and metadata gates check language resources. Preserve technical identifiers and third-party attributions. Do not weaken these checks to accommodate a new string; add the correct locale entry or a narrow documented exemption where the text is demonstrably not UI copy.

## Components and plugins

Reuse a control from `ui-primitives` when the same control appears across feature packages. Feature packages may add behavior and composition, but should not fork shared button, menu, dialog, input, icon, or focus styling. Plugin surfaces use the same tokens and exported primitives when running inside the Client UI. Professional renderers such as Markdown, syntax highlighting, and file previews should consume theme tokens at their integration boundary while retaining renderer-specific syntax or data semantics.

## Governance and scope

Existing focused specs govern color literals, radii, menu materials, elevation, focus, hover, and related interaction rules. The shared token contract is an additional CI gate, not a replacement for those specs. CSS geometry that is specific to one component, runtime measurements, OS-native window behavior, SVG path data, and externally supplied values are not automatically violations. Keep exceptions narrow, explain the owner and reason beside the existing gate, and avoid broad path or property exemptions.

This inventory does not claim that every component-local dimension is already tokenized. When repeated design values are encountered during normal changes, promote them to an existing shared scale or a well-scoped token and migrate the affected consumers without changing rendered values. A visual overhaul is intentionally outside this governance pass.
