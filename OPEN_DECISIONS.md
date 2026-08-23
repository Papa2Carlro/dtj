# Open Decisions — DTJ Independent Project

This document captures explicit unresolved decisions for the DTJ project.

## License

**Status: Unresolved**

The project has MIT AND Apache-2.0 dual licenses (LICENSE-MIT, LICENSE-APACHE),
but which license applies to which components, or if a single license should be
chosen, has not been explicitly decided.

**Action needed**: Project team must decide and document:
- Whether dual-licensing is intentional or a temporary measure
- Which license applies to the Rust core (`crates/dtj/`)
- Which license applies to Python MCP (`packages/dtj-mcp/`)
- Which license applies to VS Code integration (`packages/dtj-vscode/`)
- Whether a FOSS exception or additional terms are needed

## Repository Name

**Status: Unresolved**

The canonical repository name/organization has not been finalized.

**Options under consideration**:
- `github.com/dtj-standard/dtj`
- `github.com/dtjorg/dtj`
- `github.com/debug-trace/dtj`

**Action needed**: Choose and announce canonical repository name.

## Repository Hosting

**Status: Pending**

Remote GitHub repository has not been created yet.

**Constraint**: Per task rules, no `git push`, `git submodule`, or `GitHub repo` creation
should be performed as part of this migration.

**Action needed**: Repository URL placeholder to be used in documentation until
remote hosting is arranged externally.

## Pro Tier Pricing and Features

**Status: Unresolved** (carried over from DTJ_EXTRACTION_DECISION.md)

The Pro tier pricing and feature boundaries were documented in the extraction decision
but have not been formally adopted as project policy.

**Items needing decision**:
- Final Pro tier monthly/annual pricing
- Which of the listed features (advanced analysis, CI reporting, VS Code explorer,
  team governance, encryption/signing) are included vs. optional addons
- Whether signing/encryption will be in v1 or deferred to v2

## Versioning Cadence

**Status: Unresolved**

Semantic versioning is assumed (`major.minor.patch`), but:
- What constitutes a breaking change requiring `format_version >= 2`?
- What is the minimum acceptable release frequency?
- Are pre-release tags (`-alpha`, `-beta`) supported?

## API/SDK Public Status

**Status: Unresolved**

The extraction decision states C#, Python, and TypeScript SDKs are "not ready for
public release," but:
- What exactly "ready" means (alpha, beta, 1.0, feature-complete)?
- What minimum conformance must be achieved before SDK announcement?
- Who owns SDK maintenance and deprecation policy?

## Migration Timeline from Doc Hub

**Status: Unresolved**

No concrete timeline for migrating Doc Hub users to the independent DTJ has been
established.

**Constraints per task rules**: No commercial packaging, no license sales, no
package publication should be attempted.

**Action needed**: Define migration path that satisfies:
- Existing Doc Hub users
- Independence from Doc Hub paid pack mechanics
- Smooth transition without broken links or broken builds

## Drift from ADR 0008

**Status: Under review**

The independent DTJ format specification (`specs/dtj-format-v1.md`) was derived
from ADR 0008 but may have diverged during extraction.

**Action needed**: Review and document any differences between the new spec and
ADR 0008, and whether a new ADR is needed for the independent project.

---
*This document is a living record of decisions that need explicit project
consensus. No action item should be auto-closed without documented approval.*