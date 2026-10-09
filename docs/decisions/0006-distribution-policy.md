# 0006 — GPLv3 and commercial licensing

**Date:** 8 October 2026  
**Status:** licensing model, launch prices, and commercial policy accepted; final agreement pending

## Decision

Use GPLv3 only (`GPL-3.0-only`) for the open-source edition and offer a separate
commercial licensing route. The owner selected this model following discussion
of permissive, weak-copyleft, and commercial alternatives.

On 8 October 2026 the owner accepted these commercial launch prices, in USD
excluding applicable taxes:

| Tier | Annual company revenue | Price |
|---|---|---|
| Starter | Up to and including $50,000 | Free |
| Indie | Above $50,000, up to and including $500,000 | $199 per developer per year |
| Pro | Above $500,000, unlimited | $699 per developer per year |

Measure company revenue over the previous twelve months at purchase or renewal.
All tiers include all supported features, platforms, and formats, unlimited
products, and no royalties. Build machines and CI carry no separate fees.
Paid licences purchase twelve months of updates plus permanent commercial rights
to the versions received. Customers may continue building and distributing
products with those versions without renewing. Growth does not retroactively
invalidate earlier licences; the revenue tier applies at purchase or renewal.

Launch with annual billing. Community support covers every tier; paid customers
receive a direct reporting channel. Engineering support and contractual response
times are separate purchases. Keep the preview free and introduce paid licences
when production plug-ins, usable editors, and documentation are reliable.

The standard GPL is unmodified. Its users do not inherit the commercial revenue
limits. Current first-party source, tests, and examples share the GPL route.
The commercial route requires a separately executed agreement; this decision
record supplies no alternative legal permissions.

## Consequences

- Root LICENSE contains the full GPLv3 text.
- All current workspace packages inherit GPL-3.0-only metadata.
- Contributor terms and dependency choices must preserve the ability to grant
  commercial rights; a GPL-only contribution or dependency needs additional
  rights or a replacement before inclusion under proprietary terms.
- Launch prices and the policy above are accepted. The final agreement, detailed
  revenue and seat definitions, Starter retained rights, support operation, and
  contributor terms remain to be established.
- Publication remains disabled. Runtime licensing/activation is not part of
  this decision.

[LICENSING.md](../../LICENSING.md) is the current policy and open-issues record.

## Commercial agreement drafting — 8 October 2026

The owner requested a draft commercial agreement. [Draft 0.1](../legal/COMMERCIAL_LICENSE_DRAFT.md)
implements the accepted policy and proposes remaining details, including
Starter retained rights, revenue/group calculations, seats, warranties,
liability, termination, and governing law. These additional drafting choices
remain proposals for owner and legal review. A completed, accepted final
agreement is required before commercial rights are granted. Drafting does not
change GPL metadata, publication status, or the production-readiness gate.
