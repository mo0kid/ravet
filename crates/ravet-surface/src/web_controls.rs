//! Local DOM controls for consumer-owned WebKit/WebView-style documents.

/// Reusable slider/number, rotary knob, piano keyboard, toggle, choice, action-button, meter, roving-focus, bounded textarea, popup-menu, tabbed-panel, native modal-dialog, split-pane, fixed-list, hierarchical-tree, readonly table-grid and fixed-model virtual-list JavaScript.
/// Native inputs and knob ARIA slider roles supply keyboard/focus semantics;
/// consumers provide meaningful labels and knob theme CSS.
/// Consumers own markup, layout, theme, transport and actual platform validation.
/// Meter text retains actual peak/RMS beyond visual bounds; invalid snapshots
/// preserve the display. Sliders/knobs track held navigation keys per gesture.
/// Knobs support linear/logarithmic travel, pointer cancellation, fine dragging
/// and explicit numeric commits; DOM presentation is consumer-owned.
/// Pointer adjustment of knobs/sliders leaves paired numeric editing, suppresses
/// label activation and uses the shared theme to hide pointer focus outlines.
/// Keyboard navigation restores focus styling; direct numeric entry remains native.
/// Knob disabled state removes sequential focus and coordinates native numbers, retaining refused
/// end ownership until accepted retry. Host acceptance and local painting remain
/// separate when callbacks retire a target. Meter label references restore exactly;
/// formatter retirement/disposal cannot write stale display state.
/// Roving focus validates 1..256 distinct consumer-labelled members, supplies
/// one tab stop with explicit direction/wrapping, and restores tabindex on disposal.
/// Piano keys share one tab stop; focus/visibility/removal/disable transitions
/// release owned notes, and overlapping Space/Enter presses retain ownership.
/// Text editing owns at most 4096 committed UTF-8 bytes and 64 combined history
/// snapshots, with explicit local revision tokens, browser grapheme boundaries and
/// composition guards. Consumers own transport and Rust document acknowledgement.
/// Parameter gestures are owned per live document, with same-document paired-input
/// validation and owner-window lifecycle events. Popup menus own one level of at
/// most 256 native action buttons and restore exact attributes on disposal.
/// Labelled tabbed panels compose at most 256 fixed native-button/panel pairs with
/// manual or automatic activation, owner-document lifecycle and exact restoration.
/// Native modal dialogs require browser APIs, a fixed same-document focus target and
/// scalar completion values of at most 128 UTF-16 code units. Queued trusted close
/// retains lifecycle ownership; disposed nodes retain a tombstone until native drain,
/// while different nodes become usable after synchronous closure/callback unwinding.
/// Split panes own three-track physical grid layout with measured pixel minima,
/// TooSmall input suspension, original-document pointer/keyboard lifecycle and exact
/// owned style/attribute restoration. Unsupported transforms and anonymous/generated
/// grid items cannot silently displace the fixed profile.
/// Fixed listboxes separate one root Tab stop/virtual active focus from bounded
/// single/multiple selection and Shift ranges. Eligibility changes retain selected
/// unavailable IDs; explicit replacements remove them without newly selecting others.
/// Both components guard consumer reentry/retirement and bound caller-visible snapshots.
/// Fixed trees bound entries to 256, depth to 32 and descendant profile scans to
/// 8192 elements, with one virtual root focus and separate branch expansion/selection.
/// Collapsed descendants retain selected model state; active fallback stays quiet.
/// Direct native toggles require successful original-document root focus before
/// user expansion, and synchronous custom-element disposal stops later owned writes.
/// Readonly native table grids separate active cell navigation from selected rows,
/// with up to 256 rows, 32 columns, 4096 body cells and 16384 scanned descendants.
/// Fixed explicit column-header associations participate in cell eligibility;
/// unavailable row selections survive refresh, and quiet fallback is deterministic.
/// Directional boundaries do not install or extend a selection range.
/// Fixed virtual text lists copy 1..65536 items, with model extent at most 1000000px,
/// 12..256px fixed rows, 0..32 overscan, at most 512 mounted/free rows and 256 selected
/// indices. Original active/pressed identities pin exact rows through native clicks;
/// consumed Capacity clicks retain their pin, while accepted keyboard/programmatic
/// intent cancels an abandoned press even at a no-movement navigation boundary.
/// Selection arrays are dense. Local list/tree/table/virtual-list retirement cleans
/// original nodes/documents without removing listeners on a replacement WindowProxy.
/// These DOM contracts are not assistive-technology or visual-focus certification.
/// Controls do not transmit data by themselves. Their explicit transport receives
/// physical parameter edits/gestures or action messages. Host adapters remain
/// responsible for schema validation, sample timing, queue bounds and end slots.
pub const WEB_CONTROLS_SCRIPT: &str = include_str!("../web/controls.js");

/// Default warm charcoal/plum/amber semantic palette, piano-key styling and text selection policy.
/// UI labels and other body text are not selectable; inputs, textareas, editable
/// content and preformatted/code output remain selectable. Consumer CSS may override this policy.
/// Shared knobs/sliders suppress pointer focus outlines until keyboard input.
/// Consumer CSS may override the `--ravet-*` variables. Matches `drawing::Theme::DARK`.
pub const WEB_THEME_STYLE: &str = include_str!("../web/theme.css");

/// Replace exactly one `<!-- RAVET_CONTROLS -->` marker in a trusted local page
/// with the inline reusable script and default palette/piano styles. Allocates on the UI/preparation thread.
/// Rejects missing/duplicate markers or pages over 1 MiB before allocation.
/// The consumer's CSP must permit the inline script. No script/markup sanitization
/// is supplied: this is for consumer-authored documents, never remote content.
pub fn embed_web_controls(template: &str) -> Option<String> {
    const MARKER: &str = "<!-- RAVET_CONTROLS -->";
    if template.matches(MARKER).count() != 1
        || template.len().checked_sub(MARKER.len())?.checked_add(
            WEB_CONTROLS_SCRIPT.len()
                + WEB_THEME_STYLE.len()
                + "<style></style><script></script>".len(),
        )? > 1_048_576
    {
        return None;
    }
    Some(template.replacen(
        MARKER,
        &format!("<style>{WEB_THEME_STYLE}</style><script>{WEB_CONTROLS_SCRIPT}</script>"),
        1,
    ))
}
