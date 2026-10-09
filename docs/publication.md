# Public publication boundary

RAVET's public repository contains reusable framework code and public examples.
Proprietary consumer products are developed in separate private sibling
projects, with their own manifests, assets, fixtures, wrappers, and documentation.
They are not public workspace members. Ignoring a path is a secondary guard;
it does not protect files that were already committed.

External framework comparison notes and feature-parity plans also stay in
private sibling notes outside the checkout. Public documentation describes
RAVET's own capabilities and limits directly. Review file contents and
historical revisions for comparison narratives before publication;
the automated publication checker checks known private paths only.

Before changing repository visibility to public or pushing history to a public
remote, run:

```sh
python3 scripts/check-publication.py
python3 scripts/check-docs.py
```

The publication check inspects tracked paths and all reachable local Git
history for known private consumer paths, including deleted and renamed paths
from commit changes. Regression tests use disposable repositories to verify
these cases. A pass is a path check, not a content
or licensing audit; also review the complete tracked-file list and dependency
provenance. Do not publish product source, sound fixtures, assets, sessions,
licences, or credentials through source archives or build artifacts.
Local application checkpoint refs are included in `--all` and must be reviewed
alongside branches and tags; deleting a working file does not clear those refs.

## Clean publication history — 9 October 2026

The publishable history starts from a fresh root containing the reviewed public
framework snapshot. Earlier consumer code, external comparison notes and local
checkpoint history were removed from reachable local refs. The original history
and uncommitted work were backed up in a private sibling directory outside this
checkout. Keep that backup private and never push its refs or merge its commits
into the cleaned history. Other existing clones must use the cleaned history too.

The private remote's `main` was replaced with the clean root using an explicit
force-with-lease. This ref rewrite does not change repository visibility and does
not prove that hosted caches, pull-request refs, forks or other clones were
purged. Before making an existing remote public, resolve any remaining hosted
copies through the provider, or use a new public repository with only the cleaned
history. See [GitHub's history-removal guidance](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/removing-sensitive-data-from-a-repository).

A post-rewrite GitHub API check still resolves the original private root commit
by SHA. Keep the existing remote private until its remaining hosted copies are
removed, or publish only the cleaned history to a new repository.

The publication checker makes no visibility changes, history rewrites or public
pushes. Private consumer validation is not a public support claim.
