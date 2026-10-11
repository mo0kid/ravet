# First-party SVG import fixtures

`scene.svg` and `integer.svg` were authored for RAVET's reusable SVG importer. They
contain no downloaded art, font, product/license fixture or external resources.
The repository's `media/ravet_LOGO.svg` is the existing first-party outlined logo;
the importer consumes its original bytes. This fixture profile does not discover
system fonts or render text. First-party source/fixture licensing follows RAVET.

`scene.svg` exercises elliptical/quadratic/smooth paths, linear/radial gradients,
clipping, group opacity and transforms. `integer.svg` supplies exact aligned
rectangles. Rust tests additionally exercise all path families and complete input/
output/refusal boundaries. The tutorial's independent fixture oracle creates an
owned Blob SVG image, decodes it with the browser, draws its own aspect-fit canvas,
reads numeric pixels and revokes the Blob URL; it does not call resvg or use a
resvg-generated SVG export. Product canvas pixels come from the Rust importer.

Aligned integer profiles are exact. Other profiles require mean absolute component
error<=3, <=5% pixels with component error>32, ink bounds<=2pixels and nonblank ink
ratio0.95..1.05. Numeric premultiplication permits comparing backend PMA against
browser straight pixel readback. These are small fixture tolerances, not a general
SVG conformance or display-fidelity certificate. Actual trusted browser controls
use Rust-exported reference snapshots with a separate frontend revision; the
owned WK native consumer has a live Rust model and synthetic DOM transport events.

Primary references: [viewport transforms](https://www.w3.org/TR/SVG2/coords.html#ComputingAViewportsTransform),
[SVG path grammar](https://www.w3.org/TR/SVG2/paths.html#PathData),
[SVG paint grammar](https://www.w3.org/TR/SVG2/painting.html#SpecifyingPaint).
