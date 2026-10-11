"""Render exact, numbered tutorial snapshots with reviewed chapters and line notes."""
import hashlib
import html
import re
from pathlib import Path
from ravet_tutorial_content import TUTORIALS
from ravet_line_notes import EXACT_NOTES, FIELD_NOTES
from ravet_editor_notes import editor_note
from ravet_statement_notes import statement_note
from ravet_highlighting import highlight_lines

LINE_NOTES = {
    'frame_offset: event.frame_offset - start': 'Translate an absolute clip position into the current block’s local frame offset. The event payload is retained by the struct update below.',
    'parameter(60_013': 'Change delay feedback (ID 2) to 0.25 at absolute frame 60,013, which intentionally lies inside a 256-frame render block.',
    'parameter(72_007': 'Switch dry bypass (ID 4) on at frame 72,007. Events apply before the sample at their offset.',
    'input[0][73_600]': 'Place a later left-channel impulse after bypass has become active, so the rendered clip can verify its exact dry level.',
    'delay.set_controls([250.': 'Request 250 ms delay, feedback 0.5, fully wet output, bypass off and ping-pong on, in the delay’s five-value physical order.',
    '(1. - cross) * wet[c] + cross * wet[1 - c]': 'Blend same-channel and opposite-channel delayed samples for feedback. cross=0 is stereo; cross=1 is ping-pong. Add dry input before writing the ring.',
    '((1. - mix) * dry[c] + mix * wet[c])': 'Use a linear dry/wet mix. Conversion to f32 occurs only at the output; this expression is not a limiter.',
    'self.values[3] == 1.': 'Test the immediate bypass control. Delay history has already advanced for the frame, so bypass preserves the internal tail.',
    'voice.expression.member_bend + voice.expression.master_bend': 'Add per-member and shared master pitch expression. Their independent ownership is demonstrated by the MPE trace.',
    '(pitch - 69.) / 12.': 'Convert semitone pitch relative to MIDI key 69 (A4) into a frequency ratio. Twelve semitones multiply frequency by two.',
    '/ VOICES as f64': 'Scale the sum by the fixed eight-voice capacity rather than the currently active voice count. This is gain scaling, not overload protection.',
    'voice.expression.timbre + voice.expression.master_timbre': 'Combine member and master timbre, clamping the sum to the supported normalized range before choosing a filter-table entry.',
    'voice.expression.pressure + voice.expression.master_pressure': 'Combine member and master pressure with a normalized clamp before applying the example’s amplitude mapping.',
    'values[0] = 0.': 'Select the sine waveform for this deterministic rendering fixture. The live default patch selects saw.',
    'values[2] = 20_000.': 'Request a 20 kHz base cutoff; SynthPatch still caps designed coefficients below 45% of the actual sample rate.',
    'values[4] = 0.': 'Disable the envelope’s positive filter excursion for this fixture. Its VCA envelope still controls amplitude.',
    '0xb0, 64, 127': 'Send sustain-pedal down (controller 64) on zero-based channel 0. A later note-off remains owned until pedal release.',
    '0xb0, 64, 0': 'Send sustain-pedal up on channel 0, allowing the retained voice to enter its latched release envelope.',
    '[0xb0, 101, 0]': 'Set the RPN most-significant selector to zero on master channel 0. The next selector identifies the specific RPN.',
    '[0xb0, 100, 6]': 'Select RPN 6, the MPE zone configuration, on the lower-zone master channel.',
    '[0xb0, 6, 2]': 'Use Data Entry to request two lower-zone member channels, 1 and 2.',
    '[0xb1, 6, 12]': 'Request a 12-semitone member bend range through RPN 0 on channel 1.',
    '[0xe1, 127, 127]': 'Encode maximum 14-bit member-channel pitch bend. Only member 1’s note receives this bend.',
    '[0xe0, 0, 0]': 'Encode minimum master-channel pitch bend. The master expression affects both zone members.',
    'receiver.finish(0)': 'Explicitly release slot zero after the hypothetical DSP release tail. This inspector has no DSP envelope to wait for.',
    '0xabcd': 'Use a 16-bit note-on velocity value that cannot be represented losslessly as a 7-bit MIDI 1.0 velocity.',
    '0x9123_4567': 'Use an exact 32-bit per-note pitch-bend value to verify that the UMP round trip preserves expression precision.',
    '0xdead_beef': 'Use an exact 32-bit poly-pressure test value, retaining same-frame ordering with the preceding bend message.',
    'decoder.push(*word)': 'Feed one UMP word at a time. The decoder returns a packet only when the required number of words has arrived.',
    'decoder.finish()': 'Check that no partial UMP packet remains after feeding the message’s words.',
    'AudioBuffer::in_place': 'Borrow the active mutable channel slices as both effect input and output. Constructing the view does not allocate audio storage.',
    'AudioBuffer::separate': 'Borrow distinct input/output slices. An instrument supplies an empty input slice; output storage already belongs to the caller.',
    'ProcessBlock::new': 'Combine borrowed audio, the active input-event slice and explicit transport into one processing view.',
    '.validate_block(': 'Check the complete block against prepared limits before the callback mutates DSP history.',
    'block.clear_outputs()': 'Silence active output samples and discard emitted events. This is the documented failure path, and also clears unselected device channels before routing.',
    'ProcessStatus::Continue': 'Represent a successfully processed block. The offline renderer treats any different status as a failed rendering session.',
    'smoothing_samples(': 'Convert a time in milliseconds into a validated, sample-counted ramp duration at the configured rate.',
    '.next_value()': 'Advance a sample-counted smoother once. In the stereo delay this happens once per frame, not once per output channel.',
    '.set_target(': 'Set the requested smoother target; the effective value reaches it over the prepared sample ramp.',
    'create_new(true)': 'Refuse to overwrite an existing file. Use a new output path for a second offline rendering.',
    'writer.finish()': 'Finalize the WAV stream’s bookkeeping before reporting a successful file write.',
    'writer.write_frames(': 'Write the active interleaved frame prefix using worker-side I/O; this call is outside audio processing.',
    'self.shared.panic.swap(false': 'Atomically consume a pending All notes off/reset request using acquire/release ordering.',
    'try_publish(words)': 'Attempt a coherent telemetry update without blocking. Failure may leave the last readable snapshot in place.',
    'telemetry.try_read()': 'Attempt to read a complete coherent word snapshot. If none is available, the UI retains its previous record.',
    'self.frames >> 32': 'Store the high half of the 64-bit rendered-frame counter alongside its low u32 word.',
    '.to_bits()': 'Transfer an f32’s exact representation as a u32 word; the UI reconstructs the value with from_bits.',
    'f32::from_bits': 'Reconstruct a displayed float from the exact word representation published by the audio owner.',
    'allocation_counter::measure': 'Measure heap activity only in the enclosed processing/changes/reset region; preparation and assertions are outside it.',
    'self.config = None': 'Mark the processor inactive before preparation or after deactivation; a failed setup must not retain an apparently valid configuration.',
}


def line_note(line, chapter):
    stripped = line.strip()
    if not stripped: return 'Blank line separating the surrounding statements.'
    if stripped.startswith(('//!','///','//','<!--')):
        note = re.sub(r'^(?://[/!]?|<!--)\s*', '', stripped).removesuffix('-->').strip()
        return 'Source note: ' + note
    if stripped in EXACT_NOTES: return EXACT_NOTES[stripped]
    field = re.match(r'(?:pub )?(\w+):\s*(.*)', stripped)
    if field and field[1] in FIELD_NOTES: return FIELD_NOTES[field[1]]
    for marker, note in LINE_NOTES.items():
        if marker in line: return note
    if stripped in {'}', '};', '),', '];', ')', '{', '];', '},', '];'}:
        return 'Close the surrounding scope, value or argument list in “' + chapter['title'] + '”.'
    if stripped.startswith('#['):
        return 'Apply this Rust attribute to the following item. cfg selects which target/feature/test build contains it; derive generates the named trait implementations.'
    if stripped.startswith(('use ', 'mod ', 'pub mod ')):
        return 'Bring these module/type names into scope for “' + chapter['title'] + '”; the surrounding chapter explains their responsibilities.'
    function = re.search(r'\bfn\s+(\w+)\s*\(', stripped)
    if function:
        return 'Define ' + function.group(1) + '. ' + chapter['text']
    if stripped.startswith(('pub struct ', 'struct ', 'pub enum ', 'enum ', 'impl ')):
        return 'Declare the state/type or implementation used by this chapter. ' + chapter['text']
    if stripped.startswith(('return Err', 'return ProcessStatus::Fault')):
        return 'Exit this path with an explicit error/fault; the caller must not treat this branch as successfully applied state.'
    if stripped.startswith('return '):
        return 'Return this result to the caller; the remaining statements in the function are skipped.'
    if stripped.startswith(('assert_eq!', 'assert!', 'assert_ne!')):
        return 'Verify the fixture’s expected result. A mismatch fails the test rather than becoming a successful support claim.'
    if stripped.startswith(('if ', 'if let ', 'while ', 'for ', 'match ')):
        return 'Evaluate this condition or visit this range in source order. The statements in the following scope implement “' + chapter['title'] + '”.'
    binding = re.match(r'let\s+(mut\s+)?([A-Za-z_]\w*)', stripped)
    if binding:
        return 'Bind ' + ('mutable ' if binding.group(1) else '') + binding.group(2) + ' to the expression on this and any following continuation lines. ' + 'The chapter above explains how this intermediate value is used.'
    assertion = re.search(r'\.expect\("([^"]+)"\)', stripped)
    if assertion:
        return 'Require the invariant “' + assertion.group(1) + '”. expect panics if it is broken; external inputs must be validated before relying on it.'
    if '?' in stripped:
        return 'The ? operator returns an error immediately on failure and unwraps the success value for the next step.'
    if stripped.startswith(('println!', 'print!')):
        return 'Report this explicit fixture result to stdout; printing belongs to the offline/control path, not the audio callback.'
    return statement_note(line, chapter)


# Workspace examples without reviewed walkthroughs. Remove an entry once its
# chapters and line notes exist in TUTORIALS.
UNPUBLISHED_WORKSPACES = {
    'color_workspace', 'font_fallback', 'layout_workspace', 'native_semantics',
    'paragraph_workspace', 'postscript_fonts', 'svg_workspace', 'system_fonts',
}
UNPUBLISHED_FILES = {'tests/content_constraints.rs'}


def unpublished(relative):
    parts = relative.parts
    if str(relative) in UNPUBLISHED_FILES:
        return True
    if parts[:1] == ('src',) and len(parts) > 2 and parts[1] in UNPUBLISHED_WORKSPACES:
        return True
    return parts[:2] == ('src', 'bin') and relative.stem in UNPUBLISHED_WORKSPACES


def load_tutorials(source):
    root = source/'examples/tutorials'
    pages=[]
    expected = {str(p.relative_to(root)) for p in root.rglob('*.rs') if not unpublished(p.relative_to(root))} | {'editor/index.html'}
    defined={entry[0] for entry in TUTORIALS}
    if expected != defined:
        raise ValueError(f'Tutorial coverage changed: missing={expected-defined}, unexpected={defined-expected}')
    for relative,title,command,specs in TUTORIALS:
        path=root/relative
        data=path.read_text()
        lines=data.splitlines()
        chapters=[]
        for needle,heading,text in specs:
            matches=[i for i,line in enumerate(lines) if line.startswith(needle)]
            if len(matches)!=1:
                raise ValueError(f'Walkthrough anchor {needle!r} in {relative}: {len(matches)} matches')
            chapters.append({'start':matches[0],'title':heading,'text':text})
        if chapters[0]['start'] != 0: raise ValueError(f'Unexplained first lines: {relative}')
        if [c['start'] for c in chapters] != sorted(c['start'] for c in chapters):
            raise ValueError(f'Walkthrough chapters out of order: {relative}')
        for index,chapter in enumerate(chapters):
            end=chapters[index+1]['start'] if index+1<len(chapters) else len(lines)
            chapter['end']=end
            chapter['rows']=[{'number':i+1,'code':lines[i],'note':editor_note(lines[i]) if relative.endswith('.html') else line_note(lines[i],chapter)} for i in range(chapter['start'],end)]
        slug=str(Path(relative).with_suffix(''))
        pages.append({'slug':slug,'url':'/ravet/docs/tutorials/'+slug+'/', 'title':title,'file':relative,
            'command':command,'chapters':chapters,'lineCount':len(lines),'hash':hashlib.sha256(data.encode()).hexdigest(),'finalNewline':data.endswith('\n')})
    return pages


def tutorial_html(item):
    escape=html.escape
    source='\n'.join(row['code'] for chapter in item['chapters'] for row in chapter['rows'])
    highlighted=highlight_lines(source, 'html' if item['file'].endswith('.html') else 'rust')
    toc=''.join(f'<li><a href="#L{c["start"]+1}">{escape(c["title"])} <small>lines {c["start"]+1}–{c["end"]}</small></a></li>' for c in item['chapters'])
    content=f'''<h1>{escape(item['title'])}</h1><p class="doc-file"><code>examples/tutorials/{escape(item['file'])}</code> · {item['lineCount']} source lines</p><p>Read the chapters in source order. Select a numbered source line to expand its explanation; the line numbers and code match this snapshot exactly.</p><pre><code class="language-sh">{escape(item['command'])}</code></pre><details class="doc-outline" open><summary>Chapters</summary><ol>{toc}</ol></details><div class="tutorial-actions"><button type="button" class="rv-button secondary" data-tutorial-action="expand">Show all line explanations</button><button type="button" class="rv-button secondary" data-tutorial-action="collapse">Collapse explanations</button></div>'''
    for chapter in item['chapters']:
        content+=f'<section class="tutorial-chapter"><h2>{escape(chapter["title"])}</h2><p>{escape(chapter["text"])}</p><div class="line-guide">'
        for row in chapter['rows']:
            content+=f'<details class="source-line" id="L{row["number"]}"><summary><span class="line-number">{row["number"]}</span><code>{highlighted[row["number"]-1]}</code></summary><div class="line-note"><p>{escape(row["note"])}</p><a href="#L{row["number"]}">Link to line {row["number"]}</a></div></details>'
        content+='</div></section>'
    content+=f'<p class="doc-meta">Source SHA-256: <code>{item["hash"]}</code></p>'
    return content
