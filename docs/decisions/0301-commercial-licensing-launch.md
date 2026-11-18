# 0301: Open commercial licensing

**Date:** 17 November 2026  
**Status:** Accepted by the owner. Ordering is implemented on djwaudio.co (a
separate private website project) and tested offline and against a throwaway
MySQL. The licensor address is set and `commercialLicensing.enabled` is true; it
goes live when the website is deployed. No live Stripe payment has been made.

## Context

[Decision 0006](0006-distribution-policy.md) set GPLv3 plus a commercial route
with launch prices, and held paid licensing until a final agreement existed and
RAVET could reliably ship production plug-ins. A private draft agreement (0.1)
was written on 8 October 2026. The licences page showed the prices with its
buttons disabled.

## Decision

The owner decided to open commercial licensing, treating the draft review as
sufficient and the launch gate as met:

- **Agreement.** The RAVET Commercial Licence Agreement, version 1.0, is the
  draft's terms with the Licensor identified (David Whiting, trading as DJW
  Audio, a sole trader not registered for VAT), online acceptance added as
  clause 1.3A, and the editorial notes removed. It is published as Markdown and
  HTML at djwaudio.co/ravet/licences/agreement/; each Order records the version
  and the SHA-256 of that Markdown. No separate legal review has been carried
  out beyond the owner's review of the draft.
- **Formation.** The customer accepts by submitting the order form with its
  details, certified revenue and express acceptance. The Licensor accepts by
  emailing the completed Order Form (Schedule A): after payment for Indie and
  Pro, after email confirmation for Starter.
- **Payment.** Indie and Pro are one-off Stripe Checkout payments in USD,
  quantity equal to developer seats, with no automatic renewal (agreement 7.3).
  No VAT is charged. A webhook issues the Order only when the paid amount and
  currency match the order exactly; a full refund marks it refunded.
- **Coverage.** Each Order names a RAVET release tag and commit as its initial
  Licensed Versions (all first-party files in the tag, excluding third-party
  materials). The third-party inventory is
  [dependency notices](../dependency-notices.md) and LICENSING.md at that tag.
- **Scope.** Framework licences never mint plug-in activation codes. RAVET and
  products built with it gain no accounts, activation, telemetry or runtime
  licence checks.

## Consequences

- LICENSING.md and the README describe the live route instead of a pending one.
- The website's licensed release constant must move to each new RAVET release
  tag; Orders already issued keep the tag they name.
- Changing the agreement means a new version with its own file and hash. Earlier
  Orders stay on the version they accepted (agreement 7.4).
- Opt-in codec features with LGPL components (MP3 encoding) remain third-party
  materials under their own licences, as LICENSING.md already states.
