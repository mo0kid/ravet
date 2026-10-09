# RAVET licensing

Decision recorded on 8 October 2026: **GPLv3 plus separate commercial licensing**.

## Open-source route

RAVET's first-party Rust and Python source, including its current tests and
examples, is available under the **GNU General Public License version 3 only**
(`GPL-3.0-only`). The full, unmodified licence text is in [LICENSE](LICENSE).
Workspace packages declare that identifier in Cargo metadata.

GPL permits commercial use. This route has no RAVET revenue ceiling or
requirement to buy a commercial licence if the user complies with the GPL.
Distribution of a combined product must satisfy applicable GPL requirements,
including corresponding-source availability and recipients' licence rights.
Making only the RAVET library source available does not automatically satisfy
obligations for a combined plug-in or application. The precise boundary of
separate works depends on the integration; this document grants no blanket
exception for linking or plug-in hosting.

The commercial tier limits below do not modify the GPL or apply additional
restrictions to users choosing it. Third-party dependencies retain their own
licences and notices. This source-code policy does not grant trademark rights.
No separate permissive licence has been selected for the current examples or
future generated templates.

## Commercial route

The owner approved the following launch pricing and commercial policy on
8 October 2026. A separate commercial agreement will permit proprietary
distribution on its agreed terms. **This document is not a commercial licence
grant.** Until an agreement is executed, this repository supplies the GPL route.

| Tier | Annual company revenue (USD) | Approved launch price |
|---|---|---|
| Starter | Up to and including $50,000 | Free |
| Indie | Above $50,000, up to and including $500,000 | $199 per developer per year |
| Pro | Above $500,000, with no upper limit | $699 per developer per year |

Prices are in USD, excluding applicable taxes. Exactly $50,000 qualifies for
Starter; exactly $500,000 qualifies for Indie. Revenue is company-wide, rather
than limited to RAVET-based products, measured over the previous twelve months
at purchase or renewal. These bands identify the minimum qualifying tier.

### Approved commercial policy

- Every tier includes every framework feature, supported platform, and supported
  plug-in format. This is not a promise of functionality not yet implemented.
- Unlimited products and no royalties.
- Paid licences include twelve months of updates and permanent commercial rights
  to the versions received during that period. Renewal purchases further updates;
  developers may continue building and selling products using previously licensed
  versions after renewal stops.
- No separate licence fees for build machines or CI.
- Revenue tier is assessed at purchase or renewal. Company growth does not
  retroactively invalidate earlier licences or their retained distribution rights.
- Community support is available across all tiers. Paid customers receive a direct
  reporting channel. Dedicated engineering support and contractual response times
  are priced separately; the base licence includes no response-time guarantee.
- Start with annual billing. Monthly instalments are a possible future offering,
  not an approved launch option.
- Keep the preview phase free. Introduce paid commercial licensing when RAVET can
  reliably ship production plug-ins with usable editors and documentation. Preview
  pricing does not itself grant permission to distribute proprietary products.

### Commercial agreement draft

[Commercial Licence Agreement — draft 0.1](docs/legal/COMMERCIAL_LICENSE_DRAFT.md)
contains the proposed contract and Order Form. It preserves the approved policy
and lists additional drafting choices for owner and legal review. It is not an
executed licence, and its proposed defaults have not been adopted merely by
being drafted. The repository's GPL metadata and LICENSE remain unchanged.

### Contract details still needed

The pricing and policy above are accepted; the final agreement still needs to
specify:

- Revenue calculation, currency conversion, related entities, and funding treatment.
- Developer-seat definition, contractor coverage, reassignment, and purchases
  partway through a licence year.
- Starter enrolment, eligibility reassessment, and the scope of its retained
  commercial rights; whether customers may voluntarily select a higher tier.
- Exact version coverage, support-channel operation, renewal, and termination
  for breaches of the agreement.
- Rights to modify RAVET, and permitted source/binary redistribution.
- Licence scope and treatment of third-party components.

Have the actual commercial agreement reviewed before offering it to customers.
Do not introduce accounts, activation, telemetry, or runtime licence checks
solely because this licensing decision exists.

## Contributions and dependency selection

Before accepting external contributions intended for the commercial edition,
establish contributor terms granting sufficient rights for both the GPL and
commercial routes, including appropriate patent rights. A contributor may
retain copyright while granting those permissions; copyright assignment is
not inherently required. Keep contributor and provenance records.

A DCO sign-off certifies contribution origin and the right to submit under the
stated licence; it is not, by itself, a grant of proprietary relicensing rights.
Do not promise commercial permissions for contributed code without the rights
to grant them.

Check each dependency's exact licence and linking/distribution obligations for
both routes. GPL-compatible does not necessarily mean compatible with the
commercial agreement. A third-party GPL component cannot simply be sold under
RAVET's commercial terms without the necessary permission or replacement.
Preserve required third-party notices and audit normal, build, development,
vendored, and native dependencies before release.

The Cargo identifier describes the open-source licence currently supplied by
this repository. Do not label it `MIT OR Apache-2.0`, `GPL-3.0-or-later`, or use
an invented standard SPDX identifier for the unfinished commercial agreement.
Keep `publish = false` until release preparation is intentionally completed.

## Primary references

- [GNU GPLv3 licence](https://www.gnu.org/licenses/gpl-3.0.html)
- [GNU licensing FAQ](https://www.gnu.org/licenses/gpl-faq.en.html)
- [GNU explanation of selling licence exceptions](https://www.gnu.org/philosophy/selling-exceptions.html)
- [Developer Certificate of Origin](https://developercertificate.org/)
- [Cargo licence metadata](https://doc.rust-lang.org/cargo/reference/manifest.html#the-license-and-license-file-fields)

## Optional local-font shaping and public test fixture

The default-disabled surface/umbrella `text` feature pins HarfRust 0.14.0 (MIT).
Reviewed downloaded manifests for read-fonts 0.45.0/font-types 0.12.6, bitflags,
smallvec, once_cell and stable_deref_trait declare MIT OR Apache-2.0; bytemuck and
its derive declare Zlib OR Apache-2.0 OR MIT. Yoke/zerofrom and their derives
declare Unicode-3.0; synstructure declares MIT. Locked proc-macro tooling retains
its own notices. Preserve applicable dependency notices in both distribution
routes; this scoped manifest review is not a release-wide licence audit.

The unmodified public Noto Sans Arabic fixture is SIL Open Font License 1.1,
with complete OFL, copyright, upstream commit and hashes retained under
[the fixture directory](crates/ravet-surface/tests/fixtures/text/README.md).
RAVET's first-party licence does not replace that font's terms. The fixture is
reusable testing data, rather than a required product font; consumer fonts/assets
and their separate permissions remain the consumer's responsibility.

The optional text feature also pins unicode-bidi 0.3.18 (MIT OR Apache-2.0), with
its Unicode 16.0 tables and separate upstream notices. Selected unchanged public
Unicode bidi vectors retain the complete Unicode License v3 and exact source/
hash under [their fixture directory](crates/ravet-surface/tests/fixtures/bidi/README.md).
These data terms remain separate from RAVET's first-party licence.
