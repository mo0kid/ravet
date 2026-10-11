use super::{FontError, MAXIMUM_FONT_TABLES, MAXIMUM_SYSTEM_FONT_BYTES};
/// Owned raw underlying-face tables, never a borrowed CoreText buffer.
pub(super) struct FontTable {
    pub tag: [u8; 4],
    pub bytes: Vec<u8>,
}
pub(super) fn valid_tag(tag: [u8; 4]) -> bool {
    let mut space = false;
    tag[0] != b' '
        && tag.iter().all(|byte| {
            if *byte == b' ' {
                space = true;
                true
            } else {
                !space && (0x21..=0x7e).contains(byte)
            }
        })
}
fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |sum, part| {
        let mut word = [0; 4];
        word[..part.len()].copy_from_slice(part);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}
fn put16(dst: &mut [u8], offset: usize, value: u16) {
    dst[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}
fn put32(dst: &mut [u8], offset: usize, value: u32) {
    dst[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}
/// Exact checked SFNT reconstruction; optional CFF/CFF2 uses OTTO.
/// Input digital signature is deliberately omitted; source identity is not changed.
pub(super) fn assemble(mut tables: Vec<FontTable>) -> Result<Vec<u8>, FontError> {
    if tables.is_empty() || tables.len() > MAXIMUM_FONT_TABLES {
        return Err(FontError::Capacity);
    }
    tables.sort_by_key(|table| table.tag);
    for (index, table) in tables.iter().enumerate() {
        if !valid_tag(table.tag) || (index > 0 && tables[index - 1].tag == table.tag) {
            return Err(FontError::UnsupportedFont);
        }
    }
    tables.retain(|table| table.tag != *b"DSIG");
    for tag in [
        *b"OS/2", *b"cmap", *b"head", *b"hhea", *b"hmtx", *b"maxp", *b"name", *b"post",
    ] {
        if !tables.iter().any(|table| table.tag == tag) {
            return Err(FontError::UnsupportedFont);
        }
    }
    let has = |tag| tables.iter().any(|table| table.tag == tag);
    let signature = if cfg!(feature = "postscript-fonts") && (has(*b"CFF ") || has(*b"CFF2")) {
        if has(*b"CFF ") == has(*b"CFF2") || has(*b"glyf") || has(*b"loca") {
            return Err(FontError::UnsupportedFont);
        }
        let maxp = tables
            .iter()
            .find(|table| table.tag == *b"maxp")
            .ok_or(FontError::UnsupportedFont)?;
        if maxp.bytes.len() < 6 || maxp.bytes[..4] != [0, 0, 0x50, 0] || maxp.bytes[4..6] == [0, 0]
        {
            return Err(FontError::UnsupportedFont);
        }
        u32::from_be_bytes(*b"OTTO")
    } else {
        if !has(*b"glyf") || !has(*b"loca") {
            return Err(FontError::UnsupportedFont);
        }
        0x00010000
    };
    let head = tables
        .iter()
        .position(|table| table.tag == *b"head")
        .ok_or(FontError::UnsupportedFont)?;
    if tables[head].bytes.len() < 54 || tables[head].bytes[12..16] != [0x5f, 0x0f, 0x3c, 0xf5] {
        return Err(FontError::UnsupportedFont);
    }
    let upem = u16::from_be_bytes([tables[head].bytes[18], tables[head].bytes[19]]);
    if upem == 0 {
        return Err(FontError::UnsupportedFont);
    }
    let mut length = 12usize
        .checked_add(tables.len() * 16)
        .ok_or(FontError::Capacity)?;
    for table in &tables {
        length = length
            .checked_add(
                table
                    .bytes
                    .len()
                    .checked_add(3)
                    .ok_or(FontError::Capacity)?
                    & !3,
            )
            .ok_or(FontError::Capacity)?;
        if length > MAXIMUM_SYSTEM_FONT_BYTES {
            return Err(FontError::Capacity);
        }
    }
    tables[head].bytes[8..12].fill(0);
    let mut result = vec![0; length];
    put32(&mut result, 0, signature);
    let count = tables.len() as u16;
    let power = 1u16 << (15 - count.leading_zeros());
    put16(&mut result, 4, count);
    put16(&mut result, 6, power * 16);
    put16(&mut result, 8, power.ilog2() as u16);
    put16(&mut result, 10, count * 16 - power * 16);
    let mut offset = 12 + tables.len() * 16;
    let mut head_offset = 0;
    for (index, table) in tables.iter().enumerate() {
        let record = 12 + index * 16;
        result[record..record + 4].copy_from_slice(&table.tag);
        put32(&mut result, record + 4, checksum(&table.bytes));
        put32(&mut result, record + 8, offset as u32);
        put32(&mut result, record + 12, table.bytes.len() as u32);
        result[offset..offset + table.bytes.len()].copy_from_slice(&table.bytes);
        if index == head {
            head_offset = offset;
        }
        offset += (table.bytes.len() + 3) & !3;
    }
    let adjustment = 0xb1b0afbau32.wrapping_sub(checksum(&result));
    put32(&mut result, head_offset + 8, adjustment);
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tables() -> Vec<FontTable> {
        [
            *b"OS/2", *b"cmap", *b"glyf", *b"head", *b"hhea", *b"hmtx", *b"loca", *b"maxp",
            *b"name", *b"post",
        ]
        .into_iter()
        .map(|tag| {
            let mut bytes = vec![1, 2, 3];
            if tag == *b"head" {
                bytes = vec![0; 54];
                bytes[12..16].copy_from_slice(&[0x5f, 0x0f, 0x3c, 0xf5]);
                bytes[18..20].copy_from_slice(&1000u16.to_be_bytes());
                bytes[8..12].copy_from_slice(&0xdeadbeefu32.to_be_bytes());
            }
            FontTable { tag, bytes }
        })
        .collect()
    }
    fn word(data: &[u8], offset: usize) -> u32 {
        u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap())
    }
    #[test]
    fn directory_alignment_full_checksum_and_head_zero_convention() {
        let mut source = tables();
        source.push(FontTable {
            tag: *b"DSIG",
            bytes: vec![255; 19],
        });
        source.reverse();
        let data = assemble(source).unwrap();
        assert_eq!(word(&data, 0), 0x10000);
        assert_eq!(&data[4..12], &[0, 10, 0, 128, 0, 3, 0, 32]);
        let mut total = 0u32;
        for bytes in data.as_chunks::<4>().0 {
            total = total.wrapping_add(u32::from_be_bytes(*bytes));
        }
        assert_eq!(total, 0xb1b0afba);
        let mut prior = [0; 4];
        for index in 0..10 {
            let start = 12 + 16 * index;
            let tag: [u8; 4] = data[start..start + 4].try_into().unwrap();
            assert!(tag > prior);
            prior = tag;
            let offset = word(&data, start + 8) as usize;
            let len = word(&data, start + 12) as usize;
            assert_eq!(offset % 4, 0);
            let mut raw = data[offset..offset + len].to_vec();
            if tag == *b"head" {
                raw[8..12].fill(0);
            } else {
                assert_eq!(word(&data, start + 4), 0x01020300);
            }
            let padded = (len + 3) & !3;
            raw.resize(padded, 0);
            let expected = raw
                .as_chunks::<4>()
                .0
                .iter()
                .map(|w| u32::from_be_bytes(*w))
                .fold(0u32, u32::wrapping_add);
            assert_eq!(word(&data, start + 4), expected);
            assert!(data[offset + len..offset + padded].iter().all(|x| *x == 0));
        }
    }
    #[test]
    fn unsupported_and_capacity_failures_publish_nothing() {
        let mut raw = tables();
        raw.push(FontTable {
            tag: *b"head",
            bytes: vec![],
        });
        assert_eq!(assemble(raw), Err(FontError::UnsupportedFont));
        let mut raw = tables();
        raw[0].tag = *b" a b";
        assert_eq!(assemble(raw), Err(FontError::UnsupportedFont));
        let mut raw = tables();
        raw.retain(|t| t.tag != *b"glyf");
        assert_eq!(assemble(raw), Err(FontError::UnsupportedFont));
        let mut raw = tables();
        raw.iter_mut().find(|t| t.tag == *b"head").unwrap().bytes[12] = 0;
        assert_eq!(assemble(raw), Err(FontError::UnsupportedFont));
        let mut raw = tables();
        raw[0].bytes = vec![0; MAXIMUM_SYSTEM_FONT_BYTES];
        assert_eq!(assemble(raw), Err(FontError::Capacity));
        assert!(valid_tag(*b"cvt "));
        assert!(!valid_tag(*b"a b "));
        assert!(!valid_tag(*b"    "));
        assert!(!valid_tag([0xff, b'b', b'c', b'd']));
    }
    #[test]
    fn postscript_sfnt_is_opt_in_unambiguous_and_checksums_are_complete() {
        let mut source = tables();
        source.retain(|table| table.tag != *b"glyf" && table.tag != *b"loca");
        source
            .iter_mut()
            .find(|table| table.tag == *b"maxp")
            .unwrap()
            .bytes = vec![0, 0, 0x50, 0, 0, 4];
        source.push(FontTable {
            tag: *b"CFF ",
            bytes: vec![1, 0, 4, 4],
        });
        let copy = |source: &[FontTable]| {
            source
                .iter()
                .map(|t| FontTable {
                    tag: t.tag,
                    bytes: t.bytes.clone(),
                })
                .collect()
        };
        #[cfg(not(feature = "postscript-fonts"))]
        assert_eq!(assemble(copy(&source)), Err(FontError::UnsupportedFont));
        #[cfg(feature = "postscript-fonts")]
        {
            let bytes = assemble(copy(&source)).unwrap();
            assert_eq!(&bytes[..4], b"OTTO");
            assert_eq!(checksum(&bytes), 0xb1b0afba);
            let count = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
            for i in 0..count {
                let record = 12 + 16 * i;
                let offset =
                    u32::from_be_bytes(bytes[record + 8..record + 12].try_into().unwrap()) as usize;
                let length = u32::from_be_bytes(bytes[record + 12..record + 16].try_into().unwrap())
                    as usize;
                let expected =
                    u32::from_be_bytes(bytes[record + 4..record + 8].try_into().unwrap());
                let mut table = bytes[offset..offset + length].to_vec();
                if bytes[record..record + 4] == *b"head" {
                    table[8..12].fill(0);
                }
                assert_eq!(checksum(&table), expected);
            }
            for tag in [*b"CFF2", *b"glyf", *b"loca"] {
                let mut wrong = copy(&source);
                wrong.push(FontTable {
                    tag,
                    bytes: vec![2, 0, 5, 0, 0],
                });
                assert_eq!(assemble(wrong), Err(FontError::UnsupportedFont));
            }
            let mut wrong = copy(&source);
            wrong.iter_mut().find(|t| t.tag == *b"maxp").unwrap().bytes[2] = 0;
            assert_eq!(assemble(wrong), Err(FontError::UnsupportedFont));
        }
    }
}
