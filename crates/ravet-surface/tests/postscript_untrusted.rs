//! Untrusted PostScript source bytes: crafted DICT regressions and a
//! deterministic seeded mutation loop. Every failure must be a `Result`;
//! nothing reachable from font bytes may panic.
#![cfg(feature = "text")]
use ravet_surface::text::{TextError, TextFont, TextOutlineFormat, TextVariation};
use std::panic::{AssertUnwindSafe, catch_unwind};

const CFF: &[u8] = include_bytes!("fixtures/postscript/analytic-cff.otf");
const CFF2: &[u8] = include_bytes!("fixtures/postscript/analytic-cff2.otf");
const CID: &[u8] = include_bytes!("fixtures/postscript/cid-2-cff.otf");

fn table(data: &[u8], tag: &[u8; 4]) -> (usize, usize) {
    let count = usize::from(u16::from_be_bytes([data[4], data[5]]));
    (0..count)
        .map(|i| 12 + 16 * i)
        .find(|r| &data[*r..*r + 4] == tag)
        .map(|r| {
            let word =
                |at: usize| u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as usize;
            (word(r + 8), word(r + 12))
        })
        .expect("fixture table")
}

/// CFF1 INDEX element ranges (absolute file offsets) and the end of the INDEX.
fn index(data: &[u8], at: usize) -> (Vec<(usize, usize)>, usize) {
    let count = usize::from(u16::from_be_bytes([data[at], data[at + 1]]));
    let size = usize::from(data[at + 2]);
    let offset = |k: usize| {
        data[at + 3 + k * size..at + 3 + (k + 1) * size]
            .iter()
            .fold(0usize, |a, b| a << 8 | usize::from(*b))
    };
    let base = at + 2 + (count + 1) * size;
    let ranges = (0..count)
        .map(|k| (base + offset(k), base + offset(k + 1)))
        .collect();
    (ranges, base + offset(count))
}

/// Absolute range of the only CFF1 Top DICT.
fn cff_top_dict(data: &[u8]) -> (usize, usize) {
    let (cff, _) = table(data, b"CFF ");
    let (_, after_names) = index(data, cff + usize::from(data[cff + 2]));
    index(data, after_names).0[0]
}

fn with_top_dict(base: &[u8], dict: &[u8]) -> Vec<u8> {
    let mut data = base.to_vec();
    let (start, end) = cff_top_dict(&data);
    assert_eq!(end - start, dict.len(), "length-preserving replacement");
    data[start..end].copy_from_slice(dict);
    data
}

/// The exact 26-byte Top DICT from the root review reproduction. RAVET's own
/// decimal grammar would consume it as one real number terminated at 0x9f; the
/// backend tokenizer stops at an exponent above 1000 and reinterprets the rest.
const CRAFTED_TOP: [u8; 26] = [
    30, 0x1c, 0x99, 0x99, 0x99, 0x10, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99,
    0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x9f, 0x0e,
];

fn quiet<T>(f: impl FnOnce() -> T) -> std::thread::Result<T> {
    catch_unwind(AssertUnwindSafe(f))
}

#[test]
fn exact_crafted_top_dict_refuses_with_result_not_panic() {
    let data = with_top_dict(CFF, &CRAFTED_TOP);
    let font = TextFont::from_bytes(&data, 0).expect("font loads");
    let format = quiet(|| font.outline_format()).expect("outline_format must not panic");
    assert_eq!(format, Err(TextError::UnsupportedFont));
    for id in 0..4 {
        let outline = quiet(|| font.outline_glyph(id)).expect("outline_glyph must not panic");
        #[cfg(feature = "postscript-outlines")]
        assert_eq!(outline, Err(TextError::UnsupportedFont));
        #[cfg(not(feature = "postscript-outlines"))]
        assert_eq!(outline, Err(TextError::UnsupportedFont));
    }
}

#[test]
fn crafted_fdarray_font_dict_refuses_with_result_not_panic() {
    // Lengthen the first of two Font DICTs to 15 bytes in place (the second
    // becomes one byte); the first must be refused before the second is read.
    let mut data = CID.to_vec();
    let (cff, _) = table(&data, b"CFF ");
    let fdarray = cff + 265;
    assert_eq!(&data[fdarray..fdarray + 6], &[0, 2, 1, 1, 9, 17]);
    data[fdarray + 4] = 16;
    let (dicts, _) = index(&data, fdarray);
    let (start, end) = dicts[0];
    // Backend: 0 14 Private, 14 14 Private ... ; RAVET: one real then Private.
    let crafted = [
        30, 0x1c, 0x99, 0x99, 0x99, 0x99, 0x12, 0x99, 0x99, 0x12, 0x99, 0x99, 0x99, 0x9f, 0x12,
    ];
    assert_eq!(end - start, crafted.len());
    data[start..end].copy_from_slice(&crafted);
    let original = TextFont::from_bytes(CID, 0).unwrap();
    assert_eq!(original.outline_format(), Ok(TextOutlineFormat::Cff));
    let font = TextFont::from_bytes(&data, 0).expect("font loads");
    assert_eq!(
        quiet(|| font.outline_format()).expect("no panic"),
        Err(TextError::UnsupportedFont)
    );
    assert_eq!(
        quiet(|| font.outline_glyph(1)).expect("no panic"),
        Err(TextError::UnsupportedFont)
    );
}

/// Encode a decimal spelling as CFF BCD nibbles including the terminator.
fn bcd(spelling: &str) -> Vec<u8> {
    let mut nibbles: Vec<u8> = Vec::new();
    let mut chars = spelling.chars().peekable();
    while let Some(c) = chars.next() {
        nibbles.push(match c {
            '0'..='9' => c as u8 - b'0',
            '.' => 10,
            'E' if chars.peek() == Some(&'-') => {
                chars.next();
                12
            }
            'E' => 11,
            '-' => 14,
            _ => panic!("bad spelling"),
        });
    }
    nibbles.push(15);
    if nibbles.len() % 2 == 1 {
        nibbles.push(15);
    }
    let mut out = vec![30];
    out.extend(nibbles.chunks(2).map(|p| p[0] << 4 | p[1]));
    out
}

/// A 26-byte Top DICT carrying `ItalicAngle <spelling>` plus the analytic
/// fixture's Charset/Private/CharStrings, padded with SID operators.
fn top_with_italic(spelling: &str) -> Vec<u8> {
    let mut dict = bcd(spelling);
    dict.extend([12, 2]); // ItalicAngle
    dict.extend([0xf7, 0x1d, 0x0f]); // Charset 137
    dict.extend([0x8e, 0xf7, 0x52, 0x12]); // Private 3 @ 190
    let charstrings: &[u8] = &[0xf7, 0x24, 0x11]; // CharStrings 144
    let wide: &[u8] = &[0x1c, 0x00, 0x90, 0x11];
    let mut pad = 26 - dict.len() - charstrings.len();
    let charstrings = if pad == 1 {
        pad = 0;
        wide
    } else {
        charstrings
    };
    // SID operators of size 2/3/4/6 (operand width 1/2/3/5, one-byte opcode).
    let operand = |width: usize| -> Vec<u8> {
        match width {
            2 => vec![139],
            3 => vec![247, 0],
            4 => vec![28, 0, 1],
            _ => vec![29, 0, 0, 0, 1],
        }
    };
    for opcode in [0u8, 1, 2, 3, 4] {
        if pad == 0 {
            break;
        }
        let size = match pad {
            2..=4 | 6 => pad,
            5 => 3,
            7 => 3,
            _ => 6,
        };
        dict.extend(operand(size));
        dict.push(opcode);
        pad -= size;
    }
    assert_eq!(pad, 0, "padding budget");
    dict.extend(charstrings);
    assert_eq!(dict.len(), 26);
    dict
}

#[test]
fn exponent_boundary_and_leading_zero_spellings_in_an_actual_top_dict() {
    let check = |spelling: &str, expected: Result<TextOutlineFormat, TextError>| {
        let data = with_top_dict(CFF, &top_with_italic(spelling));
        let font = TextFont::from_bytes(&data, 0).unwrap();
        let got = quiet(|| font.outline_format()).expect("no panic");
        assert_eq!(got, expected, "{spelling}");
        #[cfg(feature = "postscript-outlines")]
        if expected.is_ok() {
            assert_eq!(
                font.outline_glyph(2),
                TextFont::from_bytes(CFF, 0).unwrap().outline_glyph(2)
            );
        }
    };
    let ok = Ok(TextOutlineFormat::Cff);
    let refused = Err(TextError::UnsupportedFont);
    for s in ["1E-999", "1E-1000", "1E3", "-1.5E-1000", "1E-1", "0"] {
        check(s, ok);
    }
    // Above 1000 the backend either saturates or (with a further digit)
    // stops early and reinterprets the remaining nibbles; both refuse.
    for s in [
        "1E-1001", "1E-1009", "1E-9999", "1E-10000", "1E-10010", "1E1001", "1E1000",
        // leading-zero exponents are a conservative refusal
        "1E0", "1E05", "1E-05", "1E-0001", "1E-00010",
    ] {
        check(s, refused);
    }
}

#[test]
fn truncated_top_and_font_dicts_are_results() {
    for base in [CFF, CID] {
        let (start, end) = cff_top_dict(base);
        for cut in start..end {
            // Shorten the dictionary in place by filling its tail with a
            // trailing operand: the stream then ends with an unused operand.
            let mut data = base.to_vec();
            for byte in &mut data[cut..end] {
                *byte = 139;
            }
            let font = TextFont::from_bytes(&data, 0).unwrap();
            let format = quiet(|| font.outline_format()).expect("no panic");
            assert!(format.is_err(), "{cut}");
            for id in 0..4 {
                let _ = quiet(|| font.outline_glyph(id)).expect("no panic");
            }
        }
    }
}

#[test]
fn operand_count_mismatch_in_an_actual_top_dict_refuses() {
    // Two operands before CharStrings in the same dictionary length.
    let mut dict = top_with_italic("1E-1000");
    let at = dict.len() - 3;
    dict[at] = 0x8b; // `0 36 CharStrings` (two operands) in the same length
    dict[at + 1] = 0xaf;
    let font = TextFont::from_bytes(&with_top_dict(CFF, &dict), 0).unwrap();
    assert_eq!(font.outline_format(), Err(TextError::UnsupportedFont));
}

/// SplitMix64: deterministic, dependency-free and seedable.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

#[derive(Default, Debug)]
struct Stats {
    inputs: usize,
    flips: usize,
    truncations: usize,
    insertions: usize,
    dict_rewrites: usize,
    load_errors: usize,
    format_ok: usize,
    format_unsupported: usize,
    format_capacity: usize,
    format_other: usize,
    glyph_ok: usize,
    glyph_err: usize,
    panics: Vec<(usize, String)>,
}

const MUTATIONS_PER_FONT: usize = 20_000;

/// Absolute Top DICT range for a CFF1 or CFF2 table.
fn top_dict(data: &[u8], cff2: bool) -> (usize, usize) {
    if cff2 {
        let (at, _) = table(data, b"CFF2");
        let start = at + usize::from(data[at + 2]);
        (
            start,
            start + usize::from(u16::from_be_bytes([data[at + 3], data[at + 4]])),
        )
    } else {
        cff_top_dict(data)
    }
}

fn fuzz(name: &str, base: &[u8], seed: u64) -> Stats {
    let cff2 = name.ends_with("cff2");
    let (start, length) = table(base, if cff2 { b"CFF2" } else { b"CFF " });
    let (top, top_end) = top_dict(base, cff2);
    let mut rng = Rng(seed);
    let mut stats = Stats::default();
    for case in 0..MUTATIONS_PER_FONT {
        let mut data = base.to_vec();
        // Three quarters of edits target the CFF/CFF2 table, the rest the file.
        let (lo, hi) = if rng.below(4) == 0 {
            (0, data.len())
        } else {
            (start, start + length)
        };
        for _ in 0..1 + rng.below(4) {
            let hi = hi.min(data.len());
            if hi <= lo {
                break;
            }
            let at = lo + rng.below(hi - lo);
            match rng.below(4) {
                // Length-preserving Top DICT rewrite with a real number whose
                // nibbles are digit-heavy: the tokenizer desynchronising class.
                3 => {
                    stats.dict_rewrites += 1;
                    if top_end > data.len() || top_end <= top {
                        continue;
                    }
                    let at = if rng.below(2) == 0 {
                        top
                    } else {
                        top + rng.below(top_end - top)
                    };
                    data[at] = 30;
                    let mut next = at + 1;
                    if next < top_end && rng.below(4) != 0 {
                        // Mantissa digit then E- or E.
                        data[next] = (rng.below(10) as u8) << 4 | [11, 12][rng.below(2)];
                        next += 1;
                    }
                    for byte in &mut data[next..top_end] {
                        *byte = if rng.below(16) == 0 {
                            rng.next() as u8
                        } else {
                            (rng.below(10) as u8) << 4 | rng.below(10) as u8
                        };
                    }
                    if top_end - at >= 3 {
                        data[top_end - 2] = 0x9f;
                        data[top_end - 1] = [0x0e, 0x11, 0x12, 0x18][rng.below(4)];
                    }
                }
                0 => {
                    stats.flips += 1;
                    if rng.below(2) == 0 {
                        data[at] ^= 1 << rng.below(8);
                    } else {
                        data[at] = rng.next() as u8;
                    }
                }
                1 => {
                    stats.truncations += 1;
                    if rng.below(2) == 0 {
                        data.truncate(at);
                    } else {
                        let end = (at + 1 + rng.below(16)).min(data.len());
                        data.drain(at..end);
                    }
                }
                _ => {
                    stats.insertions += 1;
                    let count = 1 + rng.below(8);
                    let bytes: Vec<u8> = (0..count).map(|_| rng.next() as u8).collect();
                    data.splice(at..at, bytes);
                }
            }
        }
        stats.inputs += 1;
        let run = quiet(|| {
            let Ok(font) = TextFont::from_bytes(&data, 0) else {
                return None;
            };
            let format = font.outline_format();
            let mut glyphs: Vec<_> = (0..6)
                .map(|id| font.outline_glyph(id).map(|c| c.len()))
                .collect();
            // Also exercise private blends/outlines at a nondefault instance.
            if let Ok(axes) = font.variation_axes() {
                let settings: Vec<_> = axes
                    .iter()
                    .map(|a| TextVariation {
                        tag: a.tag,
                        value: a.maximum as f32,
                    })
                    .collect();
                if let Ok(instance) = font.with_variations(&settings) {
                    let _ = instance.outline_format();
                    glyphs.push(instance.outline_glyph(2).map(|c| c.len()));
                }
            }
            Some((format, glyphs))
        });
        match run {
            Err(payload) => {
                let message = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                stats.panics.push((case, message));
            }
            Ok(None) => stats.load_errors += 1,
            Ok(Some((format, glyphs))) => {
                match format {
                    Ok(_) => stats.format_ok += 1,
                    Err(TextError::UnsupportedFont) => stats.format_unsupported += 1,
                    Err(TextError::Capacity) => stats.format_capacity += 1,
                    Err(_) => stats.format_other += 1,
                }
                for glyph in glyphs {
                    if glyph.is_ok() {
                        stats.glyph_ok += 1;
                    } else {
                        stats.glyph_err += 1;
                    }
                }
            }
        }
    }
    stats
}

#[test]
fn seeded_mutation_fuzz_of_analytic_fonts_never_panics() {
    let mut panics = Vec::new();
    for (name, base, seed) in [
        ("analytic-cff", CFF, 0x5246_0001_u64),
        ("analytic-cff2", CFF2, 0x5246_0002_u64),
    ] {
        let stats = fuzz(name, base, seed);
        println!(
            "FUZZ {name} seed={seed:#x} inputs={} flips={} truncations={} insertions={} \
             dict_rewrites={} load_errors={} format_ok={} format_unsupported={} format_capacity={} \
             format_other={} glyph_ok={} glyph_err={} panics={}",
            stats.inputs,
            stats.flips,
            stats.truncations,
            stats.insertions,
            stats.dict_rewrites,
            stats.load_errors,
            stats.format_ok,
            stats.format_unsupported,
            stats.format_capacity,
            stats.format_other,
            stats.glyph_ok,
            stats.glyph_err,
            stats.panics.len()
        );
        assert_eq!(stats.inputs, MUTATIONS_PER_FONT);
        panics.extend(stats.panics.into_iter().map(|(c, m)| (name, c, m)));
    }
    assert!(
        panics.is_empty(),
        "panics: {:?}",
        &panics[..panics.len().min(10)]
    );
}
