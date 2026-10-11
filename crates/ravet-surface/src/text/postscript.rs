//! Source-profile admission for optional unhinted OpenType PostScript outlines.
use super::{TextError, TextFont, TextOutlineFormat};
use read_fonts::{
    FontData, FontRead, FontRef, TableProvider,
    ps::cff::{
        blend::BlendState,
        dict::{self, Entry, Operator, Token},
        index::Index,
        stack::Number,
    },
    tables::variations::ItemVariationStore,
    types::F2Dot14,
};

// Metadata admission bounds, distinct from backend CharString work/memory limits.
const MAXIMUM_DICT_BYTES: usize = 4096;
const MAXIMUM_FONT_DICTS: u32 = 256;

#[derive(Clone, Copy)]
enum DictContext {
    CffTop,
    CffFont,
    Cff2Top,
    Cff2Font,
}
fn operator_profile(operator: Operator, context: DictContext, count: usize) -> bool {
    use Operator::*;
    let admitted = match context {
        DictContext::CffTop => matches!(
            operator,
            Version
                | Notice
                | FullName
                | FamilyName
                | Weight
                | FontBbox
                | CharstringsOffset
                | PrivateDictRange
                | Copyright
                | IsFixedPitch
                | ItalicAngle
                | UnderlinePosition
                | UnderlineThickness
                | PaintType
                | CharstringType
                | FontMatrix
                | StrokeWidth
                | FdArrayOffset
                | FdSelectOffset
                | Encoding
                | Charset
                | UniqueId
                | Xuid
                | SyntheticBase
                | PostScript
                | BaseFontName
                | BaseFontBlend
                | Ros
                | CidFontVersion
                | CidFontRevision
                | CidFontType
                | CidCount
                | UidBase
        ),
        DictContext::CffFont => matches!(operator, PrivateDictRange | FontName | FontMatrix),
        DictContext::Cff2Top => matches!(
            operator,
            CharstringsOffset | VariationStoreOffset | FdArrayOffset | FdSelectOffset | FontMatrix
        ),
        DictContext::Cff2Font => matches!(operator, PrivateDictRange),
    };
    if !admitted {
        return false;
    }
    match operator {
        FontBbox => count == 4,
        FontMatrix => count == 6,
        PrivateDictRange => count == 2,
        Ros => count == 3,
        Xuid | BaseFontBlend => count > 0,
        _ => count == 1,
    }
}

// The pinned read-fonts 0.45.0 BCD parser (`ps/num.rs`) stops consuming a real
// number when it reads another exponent digit while the accumulated exponent is
// already above 1000. The remaining nibble bytes are then tokenized as fresh
// operands/operators, so this grammar and the backend would disagree about the
// DICT's token boundaries. Every exponent above 1000 is refused, whether or not
// that particular spelling would be truncated; with no leading zeros, every
// admitted exponent prefix is then at most 1000, so the backend consumes
// exactly the bytes this grammar consumes.
const MAXIMUM_DECIMAL_EXPONENT: u32 = 1000;

fn decimal_spelling(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    let (mantissa, exponent) = body
        .split_once('E')
        .map_or((body, None), |(m, e)| (m, Some(e)));
    if let Some(exponent) = exponent {
        let e = exponent.strip_prefix('-').unwrap_or(exponent);
        if !matches!(e.as_bytes().first(), Some(b'1'..=b'9'))
            || !e.bytes().all(|b| b.is_ascii_digit())
            || e.len() > 4
            || e.parse::<u32>().is_ok_and(|n| n > MAXIMUM_DECIMAL_EXPONENT)
        {
            return false;
        }
        if mantissa.is_empty() || mantissa == "." {
            return false;
        }
    }
    let (integer, fraction) = mantissa
        .split_once('.')
        .map_or((mantissa, None), |(i, f)| (i, Some(f)));
    if fraction.is_some_and(|f| !f.bytes().all(|b| b.is_ascii_digit())) {
        return false;
    }
    integer.is_empty()
        || integer == "0"
        || (matches!(integer.as_bytes().first(), Some(b'1'..=b'9'))
            && integer.bytes().all(|b| b.is_ascii_digit()))
}

// Decode source numerics separately from the backend's fixed-point projection.
// In particular, a legal CFF2 FontMatrix scale must not be inferred from a
// rounded backend matrix. Each decimal spelling is bounded to 64 characters.
fn source_operands(data: &[u8]) -> Result<Vec<Vec<(f64, bool)>>, TextError> {
    let mut groups = Vec::new();
    let mut values = Vec::new();
    let mut at = 0;
    let bad = || TextError::UnsupportedFont;
    while let Some(&first) = data.get(at) {
        at += 1;
        let mut take = |count: usize| -> Result<&[u8], TextError> {
            let end = at.checked_add(count).ok_or_else(bad)?;
            let result = data.get(at..end).ok_or_else(bad)?;
            at = end;
            Ok(result)
        };
        let number = match first {
            28 => Some((
                f64::from(i16::from_be_bytes(take(2)?.try_into().map_err(|_| bad())?)),
                false,
            )),
            29 => Some((
                f64::from(i32::from_be_bytes(take(4)?.try_into().map_err(|_| bad())?)),
                false,
            )),
            32..=246 => Some((f64::from(first) - 139., false)),
            247..=250 => Some((
                (f64::from(first) - 247.) * 256.
                    + f64::from(*take(1)?.first().ok_or_else(bad)?)
                    + 108.,
                false,
            )),
            251..=254 => Some((
                -(f64::from(first) - 251.) * 256.
                    - f64::from(*take(1)?.first().ok_or_else(bad)?)
                    - 108.,
                false,
            )),
            30 => {
                let mut spelling = String::new();
                'decimal: loop {
                    let byte = *take(1)?.first().ok_or_else(bad)?;
                    for (i, nibble) in [byte >> 4, byte & 15].into_iter().enumerate() {
                        if nibble == 15 {
                            if i == 0 && byte & 15 != 15 {
                                return Err(bad());
                            }
                            break 'decimal;
                        }
                        let text = match nibble {
                            0..=9 => {
                                spelling.push(char::from(b'0' + nibble));
                                ""
                            }
                            10 => ".",
                            11 => "E",
                            12 => "E-",
                            14 => "-",
                            _ => return Err(bad()),
                        };
                        if spelling.ends_with('E') && nibble == 14 {
                            return Err(bad());
                        }
                        spelling.push_str(text);
                        if spelling.len() > 64 {
                            return Err(TextError::Capacity);
                        }
                    }
                }
                // The CFF2 specification expressly permits empty and dot-only
                // decimals as zero; all other admitted spellings must parse.
                if !decimal_spelling(&spelling) {
                    return Err(bad());
                }
                let body = spelling.strip_prefix('-').unwrap_or(&spelling);
                let value = if body.is_empty() || body == "." {
                    0.
                } else {
                    spelling.parse::<f64>().map_err(|_| bad())?
                };
                if !value.is_finite() {
                    return Err(bad());
                }
                Some((value, true))
            }
            0..=27 | 31 => {
                if first == 12 {
                    take(1)?;
                }
                groups.push(core::mem::take(&mut values));
                None
            }
            _ => return Err(bad()),
        };
        if let Some(value) = number {
            if values.len() == 48 {
                return Err(TextError::Capacity);
            }
            values.push(value);
        }
    }
    if !values.is_empty() {
        return Err(bad());
    }
    Ok(groups)
}

/// One backend DICT operator with its backend operands and the independently
/// decoded source operands of the same group.
struct DictGroup {
    operator: Operator,
    operands: Vec<Number>,
    source: Vec<(f64, bool)>,
}

/// Tokenize one DICT with both the backend and the independent source decoder
/// and require them to agree before either is used: the same number of
/// operators, the same operand count per operator and the same operand kind
/// (integer or real) at each position. Unknown operators refuse.
fn aligned_dict(data: &[u8]) -> Result<Vec<DictGroup>, TextError> {
    if data.len() > MAXIMUM_DICT_BYTES {
        return Err(TextError::Capacity);
    }
    align_dict(data, source_operands(data)?)
}

fn align_dict(data: &[u8], source: Vec<Vec<(f64, bool)>>) -> Result<Vec<DictGroup>, TextError> {
    let mut source = source.into_iter();
    let mut groups = Vec::new();
    let mut operands = Vec::new();
    let mut real = Vec::new();
    for token in dict::tokens(data) {
        match token.map_err(|_| TextError::UnsupportedFont)? {
            Token::Operand(number, components) => {
                if operands.len() == 48 {
                    return Err(TextError::Capacity);
                }
                operands.push(number);
                real.push(components.is_some());
            }
            Token::Operator(operator) => {
                let values = source.next().ok_or(TextError::UnsupportedFont)?;
                if values.len() != operands.len()
                    || values.iter().zip(operands.iter().zip(&real)).any(
                        |((value, source_real), (number, real))| {
                            *source_real != *real
                                || matches!(number, Number::I32(n) if f64::from(*n) != *value)
                        },
                    )
                {
                    return Err(TextError::UnsupportedFont);
                }
                real.clear();
                groups.push(DictGroup {
                    operator,
                    operands: core::mem::take(&mut operands),
                    source: values,
                });
            }
            Token::InvalidOperator(_) | Token::InvalidTwoByteOperator(_) => {
                return Err(TextError::UnsupportedFont);
            }
        }
    }
    if !operands.is_empty() || source.next().is_some() {
        return Err(TextError::UnsupportedFont);
    }
    Ok(groups)
}

fn source_integer_profile(operator: Operator, values: &[(f64, bool)]) -> bool {
    use Operator::*;
    if matches!(
        operator,
        Version
            | Notice
            | FullName
            | FamilyName
            | Weight
            | Copyright
            | PostScript
            | BaseFontName
            | FontName
            | CharstringsOffset
            | VariationStoreOffset
            | FdArrayOffset
            | FdSelectOffset
            | PrivateDictRange
            | Encoding
            | Charset
            | UniqueId
            | Xuid
            | SyntheticBase
            | CidFontType
            | CidCount
            | UidBase
            | PaintType
            | CharstringType
            | IsFixedPitch
    ) {
        if values
            .iter()
            .any(|(n, _)| n.fract() != 0. || *n < 0. || *n > f64::from(u32::MAX))
        {
            return false;
        }
        if operator == IsFixedPitch && values.first().is_none_or(|(n, _)| *n > 1.) {
            return false;
        }
    }
    if operator == Ros
        && values.get(..2).is_none_or(|ids| {
            ids.iter()
                .any(|(n, _)| n.fract() != 0. || *n < 0. || *n > 65535.)
        })
    {
        return false;
    }
    true
}

fn cff2_matrix_profile(data: &[u8], units: u16) -> Result<(), TextError> {
    let groups = aligned_dict(data)?;
    let matrix = groups
        .iter()
        .rev()
        .find(|g| g.operator == Operator::FontMatrix)
        .map(|g| g.source.as_slice());
    let Some(values) = matrix else {
        return if units == 1000 {
            Ok(())
        } else {
            Err(TextError::UnsupportedFont)
        };
    };
    let [
        (xx, xx_real),
        (yx, _),
        (xy, _),
        (yy, yy_real),
        (dx, _),
        (dy, _),
    ] = values
    else {
        return Err(TextError::UnsupportedFont);
    };
    if !xx_real
        || !yy_real
        || *xx <= 0.
        || xx != yy
        || [yx, xy, dx, dy].into_iter().any(|n| *n != 0.)
        || (xx * f64::from(units) - 1.).abs() > 1e-12
    {
        return Err(TextError::UnsupportedFont);
    }
    Ok(())
}

fn checked_dict(data: &[u8], context: DictContext) -> Result<Vec<Entry>, TextError> {
    if data.len() > MAXIMUM_DICT_BYTES {
        return Err(TextError::Capacity);
    }
    let mut operators = Vec::new();
    for DictGroup {
        operator,
        operands,
        source,
    } in aligned_dict(data)?
    {
        if !operator_profile(operator, context, operands.len())
            || !source_integer_profile(operator, &source)
        {
            return Err(TextError::UnsupportedFont);
        }
        if operators.contains(&operator) {
            return Err(TextError::UnsupportedFont);
        }
        if operator == Operator::CharstringType
            && !matches!(operands.as_slice(), [Number::I32(2)])
            && !matches!(operands.as_slice(), [Number::Fixed(n)] if n.to_bits() == 2 << 16)
        {
            return Err(TextError::UnsupportedFont);
        }
        if operator == Operator::FontMatrix && operands.len() != 6 {
            return Err(TextError::UnsupportedFont);
        }
        operators.push(operator);
    }
    let entries: Vec<_> = dict::entries(data, None)
        .collect::<Result<_, _>>()
        .map_err(|_| TextError::UnsupportedFont)?;
    // The backend deliberately ignores invalid FontMatrix data. Our supported
    // profile requires every explicitly authored matrix to parse successfully.
    if entries
        .iter()
        .filter(|e| matches!(e, Entry::FontMatrix(_)))
        .count()
        != operators
            .iter()
            .filter(|o| **o == Operator::FontMatrix)
            .count()
    {
        return Err(TextError::UnsupportedFont);
    }
    if entries
        .iter()
        .any(|e| matches!(e, Entry::PaintType(n) if *n != 0))
    {
        return Err(TextError::UnsupportedFont);
    }
    Ok(entries)
}

fn checked_fdselect(data: &[u8], cff2: bool, count: u32, glyphs: u32) -> Result<(), TextError> {
    let invalid = || TextError::UnsupportedFont;
    match data.first().copied().ok_or_else(invalid)? {
        0 => {
            let end = usize::try_from(glyphs)
                .ok()
                .and_then(|n| n.checked_add(1))
                .ok_or_else(invalid)?;
            let fds = data.get(1..end).ok_or_else(invalid)?;
            if fds.iter().any(|fd| u32::from(*fd) >= count) {
                return Err(invalid());
            }
        }
        format @ (3 | 4) => {
            if format == 4 && !cff2 {
                return Err(invalid());
            }
            let wide = format == 4;
            let prefix = if wide { 5 } else { 3 };
            let step = if wide { 6 } else { 3 };
            // Big-endian 2- or 4-byte field; every offset is checked.
            let integer = |at: usize, width: usize| -> Result<u32, TextError> {
                let end = at.checked_add(width).ok_or_else(invalid)?;
                let bytes = data.get(at..end).ok_or_else(invalid)?;
                Ok(bytes
                    .iter()
                    .fold(0u32, |value, byte| value << 8 | u32::from(*byte)))
            };
            let offset = |i: usize, extra: usize| -> Result<usize, TextError> {
                i.checked_mul(step)
                    .and_then(|n| n.checked_add(prefix))
                    .and_then(|n| n.checked_add(extra))
                    .ok_or_else(invalid)
            };
            let ranges = integer(1, if wide { 4 } else { 2 })?;
            if ranges == 0 || ranges > glyphs {
                return Err(invalid());
            }
            let ranges = usize::try_from(ranges).map_err(|_| invalid())?;
            let mut previous = None;
            for i in 0..ranges {
                let first = integer(offset(i, 0)?, if wide { 4 } else { 2 })?;
                let fd = if wide {
                    integer(offset(i, 4)?, 2)?
                } else {
                    u32::from(*data.get(offset(i, 2)?).ok_or_else(invalid)?)
                };
                if first >= glyphs
                    || fd >= count
                    || previous.is_none() && first != 0
                    || previous.is_some_and(|p| p >= first)
                {
                    return Err(invalid());
                }
                previous = Some(first);
            }
            if integer(offset(ranges, 0)?, if wide { 4 } else { 2 })? != glyphs {
                return Err(invalid());
            }
        }
        _ => return Err(invalid()),
    }
    Ok(())
}
fn private_operator_profile(op: Operator, cff2: bool, count: usize) -> bool {
    use Operator::*;
    match op {
        BlueValues | FamilyBlues => count > 0 && count <= 14 && count.is_multiple_of(2),
        OtherBlues | FamilyOtherBlues => count > 0 && count <= 10 && count.is_multiple_of(2),
        StemSnapH | StemSnapV => count > 0 && count <= 12,
        StdHw | StdVw | SubrsOffset | BlueScale | BlueShift | BlueFuzz | LanguageGroup
        | ExpansionFactor => count == 1,
        DefaultWidthX | NominalWidthX | ForceBold | InitialRandomSeed if !cff2 => count == 1,
        VariationStoreIndex if cff2 => count == 1,
        _ => false,
    }
}
fn integer(number: Number) -> Option<u32> {
    match number {
        Number::I32(n) if n >= 0 => Some(n as u32),
        Number::Fixed(n) if n.to_bits() >= 0 && n.to_bits() & 65535 == 0 => {
            Some((n.to_bits() >> 16) as u32)
        }
        _ => None,
    }
}
fn checked_private_ranges(
    entries: &[Entry],
    bytes: &[u8],
    cff2: bool,
    store: Option<ItemVariationStore<'_>>,
    coords: &[F2Dot14],
) -> Result<(), TextError> {
    for entry in entries {
        if let Entry::PrivateDictRange(range) = entry {
            let data = bytes.get(range.clone()).ok_or(TextError::UnsupportedFont)?;
            if data.len() > MAXIMUM_DICT_BYTES {
                return Err(TextError::Capacity);
            }
            let groups = aligned_dict(data)?;
            let state = || {
                store
                    .clone()
                    .map(|s| BlendState::new(s, coords, 0))
                    .transpose()
                    .map_err(|_| TextError::UnsupportedFont)
            };
            let mut blend_state = state()?;
            let mut operands: Vec<Number> = Vec::new();
            let mut operators = Vec::new();
            let mut blended = false;
            for group in groups {
                // Blend leaves its defaults on the stack for the next operator.
                if operands.len().saturating_add(group.operands.len()) > 48 {
                    return Err(TextError::Capacity);
                }
                operands.extend(group.operands);
                match group.operator {
                    Operator::Blend if cff2 => {
                        let n = integer(*operands.last().ok_or(TextError::UnsupportedFont)?)
                            .filter(|n| *n > 0)
                            .ok_or(TextError::UnsupportedFont)?
                            as usize;
                        let k = blend_state
                            .as_ref()
                            .ok_or(TextError::UnsupportedFont)?
                            .region_count()
                            .map_err(|_| TextError::UnsupportedFont)?;
                        let count = n
                            .checked_mul(k + 1)
                            .and_then(|n| n.checked_add(1))
                            .ok_or(TextError::Capacity)?;
                        if count > 48 {
                            return Err(TextError::Capacity);
                        }
                        let start = operands
                            .len()
                            .checked_sub(count)
                            .ok_or(TextError::UnsupportedFont)?;
                        // Preserve the defaults only for subsequent structural
                        // arity admission. Actual values come from backend blend.
                        operands.truncate(start.checked_add(n).ok_or(TextError::UnsupportedFont)?);
                        blended = true;
                    }
                    op => {
                        if !private_operator_profile(op, cff2, operands.len())
                            || operators.contains(&op)
                        {
                            return Err(TextError::UnsupportedFont);
                        }
                        if op == Operator::VariationStoreIndex {
                            if blended {
                                return Err(TextError::UnsupportedFont);
                            }
                            let index = operands
                                .first()
                                .copied()
                                .and_then(integer)
                                .and_then(|n| u16::try_from(n).ok())
                                .ok_or(TextError::UnsupportedFont)?;
                            blend_state
                                .as_mut()
                                .ok_or(TextError::UnsupportedFont)?
                                .set_store_index(index)
                                .map_err(|_| TextError::UnsupportedFont)?;
                        }
                        let first = operands.first().copied().and_then(integer);
                        if op == Operator::SubrsOffset && first.is_none() {
                            return Err(TextError::UnsupportedFont);
                        }
                        if op == Operator::ForceBold && first.is_none_or(|n| n > 1) {
                            return Err(TextError::UnsupportedFont);
                        }
                        operators.push(op);
                        operands.clear();
                    }
                }
            }
            if !operands.is_empty() {
                return Err(TextError::UnsupportedFont);
            }
            for e in dict::entries(data, state()?) {
                if let Entry::SubrsOffset(n) = e.map_err(|_| TextError::UnsupportedFont)? {
                    let offset = range
                        .start
                        .checked_add(n)
                        .ok_or(TextError::UnsupportedFont)?;
                    Index::new(bytes.get(offset..).ok_or(TextError::UnsupportedFont)?, cff2)
                        .map_err(|_| TextError::UnsupportedFont)?;
                }
            }
        }
    }
    Ok(())
}

impl TextFont {
    fn sfnt_ref(&self) -> Result<FontRef<'_>, TextError> {
        use read_fonts::model::{Kind, Source};
        let Kind::Sfnt(_, index) = self.font.kind() else {
            return Err(TextError::UnsupportedFont);
        };
        let Source::Blob(data) = self.font.source() else {
            return Err(TextError::UnsupportedFont);
        };
        FontRef::from_index(data.as_ref(), index).map_err(|_| TextError::UnsupportedFont)
    }

    pub(super) fn selected_outline_format(&self) -> Result<TextOutlineFormat, TextError> {
        let font = self.sfnt_ref()?;
        if self.font.num_glyphs() == 0 {
            return Err(TextError::UnsupportedFont);
        }
        // Preserve the existing TrueType path and precedence; no alternate
        // PostScript table is ever used after a present glyf table fails.
        if font
            .data_for_tag(read_fonts::types::Tag::new(b"glyf"))
            .is_some()
        {
            return Ok(TextOutlineFormat::TrueType);
        }
        let cff = font.data_for_tag(read_fonts::types::Tag::new(b"CFF "));
        let cff2 = font.data_for_tag(read_fonts::types::Tag::new(b"CFF2"));
        if cff.is_some() == cff2.is_some() {
            return Err(TextError::UnsupportedFont);
        }
        let (format, bytes, top) = if let Some(data) = cff {
            let table = font.cff().map_err(|_| TextError::UnsupportedFont)?;
            let h = table.header();
            if h.major() != 1
                || h.minor() != 0
                || h.hdr_size() != 4
                || !(1..=4).contains(&h.off_size())
                || table.names().count() != 1
                || table.top_dicts().count() != 1
            {
                return Err(TextError::UnsupportedFont);
            }
            (
                TextOutlineFormat::Cff,
                data.as_bytes(),
                table.top_dicts().get(0).ok_or(TextError::UnsupportedFont)?,
            )
        } else {
            let data = cff2.ok_or(TextError::UnsupportedFont)?;
            let table = font.cff2().map_err(|_| TextError::UnsupportedFont)?;
            let h = table.header();
            if h.major_version() != 2 || h.minor_version() != 0 || h.header_size() != 5 {
                return Err(TextError::UnsupportedFont);
            }
            (
                TextOutlineFormat::Cff2,
                data.as_bytes(),
                table.top_dict_data(),
            )
        };
        let entries = checked_dict(
            top,
            if format == TextOutlineFormat::Cff {
                DictContext::CffTop
            } else {
                DictContext::Cff2Top
            },
        )?;
        if format == TextOutlineFormat::Cff2 {
            cff2_matrix_profile(
                top,
                font.head()
                    .map_err(|_| TextError::UnsupportedFont)?
                    .units_per_em(),
            )?;
        }
        let charstrings = entries
            .iter()
            .find_map(|e| {
                if let Entry::CharstringsOffset(offset) = e {
                    Some(*offset)
                } else {
                    None
                }
            })
            .ok_or(TextError::UnsupportedFont)?;
        let index = Index::new(
            bytes.get(charstrings..).ok_or(TextError::UnsupportedFont)?,
            format == TextOutlineFormat::Cff2,
        )
        .map_err(|_| TextError::UnsupportedFont)?;
        if index.count() != self.font.num_glyphs() {
            return Err(TextError::UnsupportedFont);
        }
        let store = if let Some(offset) = entries.iter().find_map(|e| {
            if let Entry::VariationStoreOffset(n) = e {
                Some(*n)
            } else {
                None
            }
        }) {
            if format != TextOutlineFormat::Cff2 || font.fvar().is_err() {
                return Err(TextError::UnsupportedFont);
            }
            let data = bytes
                .get(offset.checked_add(2).ok_or(TextError::UnsupportedFont)?..)
                .ok_or(TextError::UnsupportedFont)?;
            let store = ItemVariationStore::read(FontData::new(data))
                .map_err(|_| TextError::UnsupportedFont)?;
            if store
                .variation_region_list()
                .map_err(|_| TextError::UnsupportedFont)?
                .axis_count() as usize
                != font
                    .fvar()
                    .map_err(|_| TextError::UnsupportedFont)?
                    .axis_count() as usize
            {
                return Err(TextError::UnsupportedFont);
            }
            Some(store)
        } else {
            None
        };
        checked_private_ranges(
            &entries,
            bytes,
            format == TextOutlineFormat::Cff2,
            store.clone(),
            self.font.normalized_coords(),
        )?;
        if let Some(offset) = entries.iter().find_map(|e| {
            if let Entry::FdArrayOffset(n) = e {
                Some(*n)
            } else {
                None
            }
        }) {
            let fd = Index::new(
                bytes.get(offset..).ok_or(TextError::UnsupportedFont)?,
                format == TextOutlineFormat::Cff2,
            )
            .map_err(|_| TextError::UnsupportedFont)?;
            if fd.count() > MAXIMUM_FONT_DICTS {
                return Err(TextError::Capacity);
            }
            if format == TextOutlineFormat::Cff
                && !entries.iter().any(|e| matches!(e, Entry::Ros { .. }))
            {
                return Err(TextError::UnsupportedFont);
            }
            if fd.count() == 0 {
                return Err(TextError::UnsupportedFont);
            }
            let selection = entries.iter().find_map(|e| {
                if let Entry::FdSelectOffset(offset) = e {
                    Some(*offset)
                } else {
                    None
                }
            });
            if let Some(offset) = selection {
                if format == TextOutlineFormat::Cff2 && fd.count() == 1 {
                    return Err(TextError::UnsupportedFont);
                }
                checked_fdselect(
                    bytes.get(offset..).ok_or(TextError::UnsupportedFont)?,
                    format == TextOutlineFormat::Cff2,
                    fd.count(),
                    index.count(),
                )?;
            } else if fd.count() > 1 || format == TextOutlineFormat::Cff {
                return Err(TextError::UnsupportedFont);
            }
            for i in 0..fd.count() {
                let dict_entries = checked_dict(
                    fd.get(i as usize).ok_or(TextError::UnsupportedFont)?,
                    if format == TextOutlineFormat::Cff {
                        DictContext::CffFont
                    } else {
                        DictContext::Cff2Font
                    },
                )?;
                if format == TextOutlineFormat::Cff2
                    && !dict_entries
                        .iter()
                        .any(|e| matches!(e, Entry::PrivateDictRange(_)))
                {
                    return Err(TextError::UnsupportedFont);
                }
                checked_private_ranges(
                    &dict_entries,
                    bytes,
                    format == TextOutlineFormat::Cff2,
                    store.clone(),
                    self.font.normalized_coords(),
                )?;
            }
        } else if format == TextOutlineFormat::Cff2
            || entries
                .iter()
                .any(|e| matches!(e, Entry::FdSelectOffset(_) | Entry::Ros { .. }))
        {
            return Err(TextError::UnsupportedFont);
        }
        Ok(format)
    }

    #[cfg(feature = "postscript-outlines")]
    pub(super) fn outline_postscript(
        &self,
        id: u32,
    ) -> Result<Vec<crate::drawing::PathCommand>, TextError> {
        use skrifa::{
            instance::{LocationRef, Size},
            outline::{DrawSettings, OutlineGlyphCollection, OutlineGlyphFormat},
        };
        let format = match self.selected_outline_format()? {
            TextOutlineFormat::Cff => OutlineGlyphFormat::Cff,
            TextOutlineFormat::Cff2 => OutlineGlyphFormat::Cff2,
            TextOutlineFormat::TrueType => return Err(TextError::UnsupportedFont),
        };
        let font = self.sfnt_ref()?;
        let outlines =
            OutlineGlyphCollection::with_format(&font, format).ok_or(TextError::UnsupportedFont)?;
        let glyph = outlines
            .get(read_fonts::types::GlyphId::new(id))
            .ok_or(TextError::InvalidInput)?;
        let mut pen = super::BoundedOutlinePen {
            commands: Vec::new(),
            overflow: false,
            invalid: false,
        };
        let draw = glyph.draw(
            DrawSettings::unhinted(
                Size::unscaled(),
                LocationRef::new(self.font.normalized_coords()),
            ),
            &mut pen,
        );
        if pen.invalid {
            return Err(TextError::Shaping);
        }
        if pen.overflow {
            return Err(TextError::Capacity);
        }
        draw.map_err(|_| TextError::Shaping)?;
        Ok(pen.commands)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_dictionary_role_arity_duplicates_and_operand_bounds() {
        let read = |bytes: &[u8]| checked_dict(bytes, DictContext::CffTop).map(|_| ());
        assert_eq!(read(&[141, 12, 6]), Ok(())); // Type 2
        assert_eq!(read(&[140, 12, 6]), Err(TextError::UnsupportedFont));
        assert_eq!(read(&[139, 12, 5]), Ok(())); // fill PaintType
        assert_eq!(read(&[140, 12, 5]), Err(TextError::UnsupportedFont));
        assert_eq!(read(&[139, 6]), Err(TextError::UnsupportedFont)); // private operator in Top
        assert_eq!(read(&[139, 17, 139, 17]), Err(TextError::UnsupportedFont));
        assert_eq!(read(&[139]), Err(TextError::UnsupportedFont));
        assert_eq!(
            read(&[255, 0, 0, 0, 0, 17]),
            Err(TextError::UnsupportedFont)
        );
        assert_eq!(read(&[30, 0x0a, 0x5f, 17]), Err(TextError::UnsupportedFont)); // fractional offset
        let mut exact = vec![139; 48];
        exact.push(14); // XUID array
        assert_eq!(read(&exact), Ok(()));
        let mut excess = vec![139; 49];
        excess.push(14);
        assert_eq!(read(&excess), Err(TextError::Capacity));
        let excess = vec![139; 4097];
        assert_eq!(read(&excess), Err(TextError::Capacity));
        assert_eq!(
            checked_dict(&[139, 12, 7], DictContext::CffFont).map(|_| ()),
            Err(TextError::UnsupportedFont)
        );
        assert_eq!(
            checked_dict(&[139, 12, 38], DictContext::Cff2Font).map(|_| ()),
            Err(TextError::UnsupportedFont)
        );
    }
    #[test]
    fn all_fdselect_wire_formats_cover_every_glyph_without_defaulting() {
        assert_eq!(checked_fdselect(&[0, 0, 0, 1, 1], false, 2, 4), Ok(()));
        assert_eq!(
            checked_fdselect(&[0, 0, 0, 2, 1], false, 2, 4),
            Err(TextError::UnsupportedFont)
        );
        assert_eq!(
            checked_fdselect(&[0, 0, 0, 1], false, 2, 4),
            Err(TextError::UnsupportedFont)
        );
        let normal = [3, 0, 2, 0, 0, 0, 0, 2, 1, 0, 4];
        assert_eq!(checked_fdselect(&normal, false, 2, 4), Ok(()));
        for n in 0..normal.len() {
            assert_eq!(
                checked_fdselect(&normal[..n], false, 2, 4),
                Err(TextError::UnsupportedFont)
            );
        }
        let wide = [
            4, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 0, 0, 0, 4,
        ];
        assert_eq!(checked_fdselect(&wide, true, 2, 4), Ok(()));
        assert_eq!(
            checked_fdselect(&wide, false, 2, 4),
            Err(TextError::UnsupportedFont)
        );
        assert_eq!(
            checked_fdselect(&[3, 0, 0, 0, 4], false, 2, 4),
            Err(TextError::UnsupportedFont)
        );
    }
    #[test]
    fn private_dictionary_roles_numbers_and_whole_ranges_are_checked() {
        let read = |data: &[u8], cff2| {
            checked_private_ranges(
                &[Entry::PrivateDictRange(0..data.len())],
                data,
                cff2,
                None,
                &[],
            )
        };
        assert_eq!(read(&[], false), Ok(()));
        assert_eq!(read(&[139, 140, 6], false), Ok(()));
        assert_eq!(read(&[139, 6], false), Err(TextError::UnsupportedFont));
        assert_eq!(read(&[139, 17], false), Err(TextError::UnsupportedFont));
        assert_eq!(read(&[139, 20], false), Ok(()));
        assert_eq!(read(&[139, 20], true), Err(TextError::UnsupportedFont));
        assert_eq!(read(&[141, 12, 14], false), Err(TextError::UnsupportedFont));
        assert_eq!(read(&[139, 22], true), Err(TextError::UnsupportedFont));
        assert_eq!(
            read(&[140, 139, 140, 23, 10], true),
            Err(TextError::UnsupportedFont)
        );
        assert_eq!(read(&[139, 19], false), Err(TextError::UnsupportedFont));
        assert_eq!(
            checked_private_ranges(
                &[Entry::PrivateDictRange(0..3)],
                &[139, 20],
                false,
                None,
                &[]
            ),
            Err(TextError::UnsupportedFont)
        );
        assert_eq!(read(&vec![139; 4097], false), Err(TextError::Capacity));
        let mut excess = vec![139; 49];
        excess.push(6);
        assert_eq!(read(&excess, true), Err(TextError::Capacity));
    }
    #[test]
    fn decimal_source_grammar_and_exact_matrix_operands_are_independent() {
        assert_eq!(
            source_operands(&[30, 0xff, 17]).unwrap(),
            vec![vec![(0., true)]]
        );
        assert_eq!(
            source_operands(&[30, 0xaf, 17]).unwrap(),
            vec![vec![(0., true)]]
        );
        assert_eq!(
            source_operands(&[30, 0x1c, 0x5f, 17]).unwrap(),
            vec![vec![(1e-5, true)]]
        );
        assert_eq!(
            source_operands(&[30, 0x05, 0xff, 17]),
            Err(TextError::UnsupportedFont)
        );
        assert_eq!(
            source_operands(&[30, 0x1b, 0x05, 0xff, 17]),
            Err(TextError::UnsupportedFont)
        );
        assert_eq!(
            source_operands(&[30, 0x1b, 0xef, 17]),
            Err(TextError::UnsupportedFont)
        );
        assert_eq!(
            source_operands(&[30, 0xd0, 0xff, 17]),
            Err(TextError::UnsupportedFont)
        );
        assert_eq!(
            source_operands(&[30, 0xf0, 17]),
            Err(TextError::UnsupportedFont)
        );
        assert_eq!(
            source_operands(&[30, 0x1b, 0x99, 0x9f, 17]),
            Err(TextError::UnsupportedFont)
        );
    }
    #[test]
    fn exact_source_dict_byte_boundary_preserves_role_validation_before_overflow() {
        // The byte bound is separate from the duplicate-operator policy: at
        // exactly 4096 bytes this syntactically complete stream reaches that
        // policy; one more byte fails the whole byte preflight first.
        let data = [139, 0].repeat(2048);
        assert_eq!(data.len(), 4096);
        assert!(matches!(
            checked_dict(&data, DictContext::CffTop),
            Err(TextError::UnsupportedFont)
        ));
        let mut excess = data;
        excess.push(139);
        assert!(matches!(
            checked_dict(&excess, DictContext::CffTop),
            Err(TextError::Capacity)
        ));
    }

    // ---- W246 R2: backend/source tokenizer agreement and panic freedom ----

    /// The root review's crafted 26-byte Top DICT.
    const CRAFTED_TOP: [u8; 26] = [
        30, 0x1c, 0x99, 0x99, 0x99, 0x10, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99,
        0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x9f, 0x0e,
    ];

    fn every_dict_path(data: &[u8]) {
        for context in [
            DictContext::CffTop,
            DictContext::CffFont,
            DictContext::Cff2Top,
            DictContext::Cff2Font,
        ] {
            let _ = checked_dict(data, context);
        }
        let _ = cff2_matrix_profile(data, 1000);
        for cff2 in [false, true] {
            let _ = checked_private_ranges(
                &[Entry::PrivateDictRange(0..data.len())],
                data,
                cff2,
                None,
                &[],
            );
        }
    }

    #[test]
    fn crafted_bcd_exponent_dictionaries_refuse_in_every_context() {
        assert_eq!(
            source_operands(&CRAFTED_TOP),
            Err(TextError::UnsupportedFont)
        );
        assert_eq!(
            aligned_dict(&CRAFTED_TOP).map(|_| ()),
            Err(TextError::UnsupportedFont)
        );
        for context in [
            DictContext::CffTop,
            DictContext::CffFont,
            DictContext::Cff2Top,
            DictContext::Cff2Font,
        ] {
            assert_eq!(
                checked_dict(&CRAFTED_TOP, context).map(|_| ()),
                Err(TextError::UnsupportedFont)
            );
        }
        assert_eq!(
            cff2_matrix_profile(&CRAFTED_TOP, 1000),
            Err(TextError::UnsupportedFont)
        );
        // Font DICT, CFF2 Top and Private variants of the same desynchronisation.
        let font = [
            30, 0x1c, 0x99, 0x99, 0x99, 0x99, 0x12, 0x99, 0x99, 0x12, 0x99, 0x99, 0x99, 0x9f, 0x12,
        ];
        let cff2_top = [30, 0x1c, 0x99, 0x99, 0x99, 0x11, 0x99, 0x18, 0x9f, 0x11];
        let private = [30, 0x1c, 0x99, 0x99, 0x99, 0x10, 0x99, 0x9f, 0x0a];
        for data in [&font[..], &cff2_top, &private] {
            assert_eq!(source_operands(data), Err(TextError::UnsupportedFont));
            every_dict_path(data);
        }
        assert_eq!(
            checked_dict(&font, DictContext::CffFont).map(|_| ()),
            Err(TextError::UnsupportedFont)
        );
        assert_eq!(
            checked_dict(&cff2_top, DictContext::Cff2Top).map(|_| ()),
            Err(TextError::UnsupportedFont)
        );
        assert_eq!(
            checked_private_ranges(
                &[Entry::PrivateDictRange(0..private.len())],
                &private,
                false,
                None,
                &[]
            ),
            Err(TextError::UnsupportedFont)
        );
    }

    fn bcd(nibbles: &[u8]) -> Vec<u8> {
        let mut all = nibbles.to_vec();
        all.push(15);
        if all.len() % 2 == 1 {
            all.push(15);
        }
        let mut out = vec![30];
        out.extend(all.chunks(2).map(|p| p[0] << 4 | p[1]));
        out
    }

    fn spelled(s: &str) -> Vec<u8> {
        let mut nibbles = Vec::new();
        let mut chars = s.chars().peekable();
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
                _ => unreachable!(),
            });
        }
        bcd(&nibbles)
    }

    #[test]
    fn exponent_boundary_spellings_around_1000() {
        let read = |s: &str| {
            let mut data = spelled(s);
            data.push(17);
            source_operands(&data)
        };
        for s in [
            "1E-999",
            "1E-1000",
            "-1E-1000",
            "1.5E-1000",
            ".5E-1000",
            "1E308",
            "1E3",
        ] {
            assert!(read(s).is_ok(), "{s}");
        }
        // Not finite in f64: refused by the existing finite-number rule.
        for s in ["1E309", "1E999", "1E1000"] {
            assert_eq!(read(s), Err(TextError::UnsupportedFont), "{s}");
        }
        // Every exponent above 1000 refuses, including the spellings the
        // backend would truncate (a further digit after more than 1000).
        for s in [
            "1E-1001",
            "1E1001",
            "1E-1009",
            "1E-1010",
            "1E-9999",
            "1E-10000",
            "1E-10010",
            "1E-99999",
            "1E-999999",
        ] {
            assert_eq!(read(s), Err(TextError::UnsupportedFont), "{s}");
        }
    }

    #[test]
    fn leading_zero_exponents_are_conservatively_refused() {
        for s in [
            "1E0",
            "1E00",
            "1E05",
            "1E-05",
            "1E-0",
            "1E-0001",
            "1E-00010",
            "1E0001001",
            "1E-0001001",
        ] {
            let mut data = spelled(s);
            data.push(17);
            assert_eq!(
                source_operands(&data),
                Err(TextError::UnsupportedFont),
                "{s}"
            );
        }
    }

    /// For one BCD body, either the source grammar refuses it or the backend
    /// consumes exactly the same bytes (one operand, then the next operator).
    fn agrees_with_backend(nibbles: &[u8]) -> bool {
        let mut data = bcd(nibbles);
        data.push(17);
        match source_operands(&data) {
            Err(_) => true,
            Ok(groups) => {
                let tokens: Result<Vec<_>, _> = dict::tokens(&data).collect();
                groups.len() == 1
                    && groups[0].len() == 1
                    && matches!(
                        tokens.as_deref(),
                        Ok([
                            Token::Operand(Number::Fixed(_), Some(_)),
                            Token::Operator(Operator::CharstringsOffset)
                        ])
                    )
                    && aligned_dict(&data).is_ok()
            }
        }
    }

    #[test]
    fn every_short_bcd_spelling_agrees_with_backend_or_refuses() {
        // Exhaustive over all non-terminator nibbles (0-9, a, b, c, d, e)
        // up to five nibbles: 813,615 bodies.
        let mut checked = 0usize;
        let mut admitted = 0usize;
        let mut body = Vec::new();
        fn walk(body: &mut Vec<u8>, depth: usize, checked: &mut usize, admitted: &mut usize) {
            assert!(agrees_with_backend(body), "{body:x?}");
            *checked += 1;
            let mut data = bcd(body);
            data.push(17);
            if source_operands(&data).is_ok() {
                *admitted += 1;
            }
            if depth == 0 {
                return;
            }
            for nibble in 0..15 {
                body.push(nibble);
                walk(body, depth - 1, checked, admitted);
                body.pop();
            }
        }
        walk(&mut body, 5, &mut checked, &mut admitted);
        assert_eq!(checked, (0..=5).map(|n| 15usize.pow(n)).sum::<usize>());
        assert!(admitted > 0);
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    #[test]
    fn seeded_long_exponent_spellings_agree_with_backend_or_refuse() {
        let mut rng = Rng(0x5246_0003);
        for _ in 0..200_000 {
            let mut body = Vec::new();
            if rng.below(2) == 0 {
                body.push(14);
            }
            for _ in 0..rng.below(4) {
                body.push(rng.below(10) as u8);
            }
            if rng.below(2) == 0 {
                body.push(10);
                for _ in 0..rng.below(4) {
                    body.push(rng.below(10) as u8);
                }
            }
            body.push(if rng.below(2) == 0 { 11 } else { 12 });
            // Exponent digits, biased to small digits so values cross 1000.
            for _ in 0..rng.below(12) {
                body.push(if rng.below(3) == 0 {
                    0
                } else {
                    rng.below(10) as u8
                });
            }
            assert!(agrees_with_backend(&body), "{body:x?}");
        }
    }

    #[test]
    fn mismatched_source_and_backend_groups_refuse_before_use() {
        let data = [139, 140, 6, 141, 17]; // BlueValues 0 1; CharStrings 2
        let good = vec![vec![(0., false), (1., false)], vec![(2., false)]];
        assert!(align_dict(&data, good.clone()).is_ok());
        let cases = [
            vec![vec![(0., false), (1., false)]], // missing group
            vec![vec![(0., false), (1., false)], vec![(2., false)], vec![]], // extra group
            vec![vec![(0., false)], vec![(1., false), (2., false)]], // shifted operands
            vec![
                vec![(0., false), (1., false), (9., false)],
                vec![(2., false)],
            ], // extra operand
            vec![vec![(0., true), (1., false)], vec![(2., false)]], // kind mismatch
            vec![vec![(0., false), (5., false)], vec![(2., false)]], // value mismatch
            vec![],
        ];
        for source in cases {
            assert_eq!(
                align_dict(&data, source.clone()).map(|_| ()),
                Err(TextError::UnsupportedFont),
                "{source:?}"
            );
        }
        // Trailing operands and unknown operators refuse after alignment too.
        assert_eq!(
            align_dict(&[139], vec![]).map(|_| ()),
            Err(TextError::UnsupportedFont)
        );
        assert_eq!(
            align_dict(&[139, 25], vec![vec![(0., false)]]).map(|_| ()),
            Err(TextError::UnsupportedFont)
        );
    }

    #[test]
    fn truncated_dictionaries_are_results_in_every_context() {
        let dictionaries: [&[u8]; 6] = [
            &[
                0xf8, 0x1b, 0x02, 0xf8, 0x1c, 0x03, 0xf8, 0x18, 0x04, 0x8b, 0x8b, 0xf8, 0x56, 0xf7,
                0x5c, 0x05, 0xf7, 0x1d, 0x0f, 0x8e, 0xf7, 0x52, 0x12, 0xf7, 0x24, 0x11,
            ],
            &[0xf7, 0x03, 0x0c, 0x24, 0xc4, 0x11, 0x9c, 0x18],
            &[
                0x1e, 0x1c, 0x3f, 0x8b, 0x1e, 0x25, 0xc5, 0xff, 0x1e, 0x1c, 0x3f, 0x8b, 0x8b, 0x0c,
                0x07, 0xf7, 0x12, 0x0c, 0x24, 0xd3, 0x11, 0xab, 0x18,
            ],
            &[0xf8, 0x1f, 0x0c, 0x26, 0x8e, 0xf7, 0xb3, 0x12],
            &[
                0x1c, 0x00, 0x10, 0x1d, 0x00, 0x00, 0x00, 0x20, 0x8b, 0x0c, 0x06,
            ],
            &CRAFTED_TOP,
        ];
        for data in dictionaries {
            for end in 0..=data.len() {
                every_dict_path(&data[..end]);
                every_dict_path(&data[end..]);
            }
        }
        // Truncation inside a multi-byte operand or two-byte operator refuses.
        for data in [
            &[28u8, 0][..],
            &[29, 0, 0, 0],
            &[247],
            &[251],
            &[12],
            &[30, 0x11],
        ] {
            assert_eq!(
                checked_dict(data, DictContext::CffTop).map(|_| ()),
                Err(TextError::UnsupportedFont),
                "{data:?}"
            );
        }
    }

    #[test]
    fn seeded_dictionary_mutations_never_panic() {
        let seeds: [&[u8]; 4] = [
            &CRAFTED_TOP,
            &[0xf7, 0x03, 0x0c, 0x24, 0xc4, 0x11, 0x9c, 0x18],
            &[
                0x1e, 0x1c, 0x3f, 0x8b, 0x1e, 0x25, 0xc5, 0xff, 0x1e, 0x1c, 0x3f, 0x8b, 0x8b, 0x0c,
                0x07, 0xf7, 0x12, 0x0c, 0x24, 0xd3, 0x11, 0xab, 0x18,
            ],
            &[0x8b, 0x8c, 0x06, 0x8b, 0x8c, 0x8d, 0x8b, 0x8c, 0x17, 0x0a],
        ];
        let mut rng = Rng(0x5246_0004);
        for case in 0..40_000 {
            let mut data = seeds[case % seeds.len()].to_vec();
            for _ in 0..1 + rng.below(4) {
                let at = rng.below(data.len() as u64 + 1) as usize;
                match rng.below(4) {
                    0 if at < data.len() => data[at] ^= 1 << rng.below(8),
                    1 => data.truncate(at),
                    // Digit-only nibble bytes are the desynchronising class.
                    2 => data.insert(at, (rng.below(10) as u8) << 4 | rng.below(10) as u8),
                    _ => data.insert(at, rng.next() as u8),
                }
            }
            let result = std::panic::catch_unwind(|| every_dict_path(&data));
            assert!(result.is_ok(), "{data:x?}");
        }
    }
}
