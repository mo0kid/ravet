# 0044 — portable visual mechanisms

## Decision

Keep bounded geometry/layout and explicit-clock scalar animation independent of
native windows/rendering. Provide a local SVG drawing backend for reusable shape,
text and theme output that a consumer can display without external assets/scripts.
A finite public drawing format is useful immediately; importing a GPU dependency
or declaring a renderer interface alone would not establish working rendering.

## Consequences and scope

Rows/columns validate all constraints and compute into bounded stack storage before
copying output. Allocation-free layout/tweens can be sampled by a consumer-owned
UI timer; there is no automatic scheduling or audio/UI thread crossover. Drawing
owns bounded UI allocations, escapes XML text/attributes and never accepts raw
markup or URLs. Browser/native SVG renderers retain font shaping/measurement;
font/asset loaders, transforms/paths, GPU/video and native control accessibility
remain separate capabilities. Theme colors are explicit consumer values.

## Validation

Four fixture groups cover cap redistribution, proportional shrink, nested resize,
input/arithmetic rejection preservation, 10000 allocation-free layouts/tweens,
exact animation edges/retarget/time overflow, escaped hostile text and command/
document limits. Two width/theme/time examples render as valid local SVG in the
collaborative browser with measured text bounds fitting the viewBox.
