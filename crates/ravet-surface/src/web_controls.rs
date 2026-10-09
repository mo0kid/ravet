//! Local DOM controls for consumer-owned WebKit/WebView-style documents.

/// Reusable slider/number, rotary knob, piano keyboard, toggle, choice, action-button and meter JavaScript.
/// Native inputs and knob ARIA slider roles supply keyboard/focus semantics;
/// consumers provide meaningful labels and knob theme CSS.
/// Consumers own markup, layout, theme, transport and actual platform validation.
/// Meter text retains actual peak/RMS beyond visual bounds; invalid snapshots
/// preserve the display. Sliders/knobs track held navigation keys per gesture.
/// Knobs support linear/logarithmic travel, pointer cancellation, fine dragging
/// and explicit numeric commits; DOM presentation is consumer-owned.
/// Controls do not transmit data by themselves. Their explicit transport receives
/// physical parameter edits/gestures or action messages. Host adapters remain
/// responsible for schema validation, sample timing, queue bounds and end slots.
pub const WEB_CONTROLS_SCRIPT: &str = include_str!("../web/controls.js");

/// Default warm charcoal/plum/amber semantic palette, piano-key styling and text selection policy.
/// UI labels and other body text are not selectable; inputs, textareas, editable
/// content and preformatted/code output remain selectable. Consumer CSS may override this policy.
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
