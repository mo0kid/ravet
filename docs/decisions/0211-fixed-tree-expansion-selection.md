# ADR0211: fixed hierarchical tree expansion and selection

Status: Accepted

## Context

The browser controls have fixed flat selection and virtual focus, but no reusable
hierarchical branch navigation. Consumers need expansion without implicit
selection and retained selected descendants across collapse.

## Decision

Add a bounded TreeView over consumer-supplied fixed HTML items, direct groups and
native type=button toggles. Limit entries to256, depth32, descendant profile
scan8,192, labels256, IDs128 and owned attribute/display originals4,096 code
units; group raw style originals16,384. Require exact connected original-document
membership, preorder, associations and ID lookup. Later invalid ownership retires.

Use one labelled root Tab stop with virtual aria-activedescendant. Selected and
expanded models are independent. Initial unavailable selections are permitted;
later newly unavailable additions reject while existing unavailable selections
can remain or be removed. Collapsing a branch retains selection and quietly
falls active focus back to an eligible ancestor. Separate changed-only frozen
callbacks report selection or expansion, with guarded reentry and post-focus/
callback revalidation. Accepted model notifications cannot roll back on later
consumer mutation/exception.

Leave native pointerdown/default button focus intact. Before click admits any
expansion, root focus and original row/toggle eligibility must succeed. Route
nested event targets to their deepest item; group background remains unowned.
Hidden/disabled toggles block user expansion but do not block programmatic model
updates. Collapsed groups receive owned hidden plus display:none!important,
restoring exact original attributes/display on disposal while preserving
unrelated consumer style edits.

## Consequences and evidence limits

This supplies fixed hierarchical navigation and selection, not dynamic trees,
virtualization, sorting, tables/grids, typeahead or drag/reorder. A bounded model
and owned descendant scan cannot bound browser layout or global document/ancestor
costs. Local Chromium/WebKit fixtures plus trusted browser keys/pointer are
behavior evidence only; native accessibility-tree/screen-reader/OS focus and
other-platform support remain unvalidated. No additional dependency, generated
nodes or global focus trap is introduced.
