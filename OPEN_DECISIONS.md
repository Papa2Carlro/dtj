# Open Decisions — DTJ Independent Project

This document captures explicit unresolved decisions for the DTJ project.

## License

**Status: Resolved** for v0.1

All currently shipping packages (`crates/dtj`, `packages/dtj-rust`,
`packages/dtj-python`, `packages/dtj-typescript`) declare
`MIT OR Apache-2.0` in their manifests. The dual-licensed `LICENSE-MIT` and
`LICENSE-APACHE` files in the repository root apply to all current source.

This means each component is available under either license at the user's
choice. Dual-licensing is intentional, not a temporary measure.

Future packages adopting a different license will be documented per-package at
their introduction.

## Repository Name

**Status: Resolved** for v0.1

Canonical repository:

```
https://github.com/Papa2Carlro/dtj
```

This is the public location for the v0.1 release. References in `README.md`
and `CONTRIBUTING.md` point to this URL. The previously considered
placeholder organizations (`dtj-standard`, `dtjorg`, `debug-trace`) are
**not** canonical and should not be used in user-facing material.

## Repository Hosting

**Status: Resolved** for v0.1

Remote is hosted on GitHub:

```
git remote -v
origin  https://github.com/Papa2Carlro/dtj.git (fetch)
origin  https://github.com/Papa2Carlro/dtj.git (push)
```

GitHub Releases on this repository are the canonical source of `dtj` and
`dtj-agent` binary artifacts for v0.1.

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