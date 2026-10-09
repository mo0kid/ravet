use ravet_data::{
    crypto::{LegacyIntegerPublicKey, sha256},
    xml::{Element, MAXIMUM_BYTES},
};

#[test]
fn attribute_trees_roundtrip_unicode_entities_and_children() {
    let input = "<?xml version=\"1.0\"?><STATE name=\"A &amp; B &quot;音&quot;\"><PARAM id=\"gain\" value=\"0.5\"/></STATE>";
    let parsed = Element::parse(input).unwrap();
    assert_eq!(parsed.attribute("name"), Some("A & B \"音\""));
    assert_eq!(Element::parse(&parsed.encode().unwrap()).unwrap(), parsed);
}

#[test]
fn malformed_unbounded_and_entity_documents_are_rejected() {
    for input in [
        "",
        "<A/><B/>",
        "<A><B></A>",
        "<A x=\"1\" x=\"2\"/>",
        "<!DOCTYPE A [<!ENTITY x SYSTEM 'file:///tmp/private'>]><A/>",
        "<A>&x;</A>",
        "<A>text</A>",
    ] {
        assert!(Element::parse(input).is_err(), "{input}");
    }
    assert!(Element::parse(&" ".repeat(MAXIMUM_BYTES + 1)).is_err());
    let nested = format!("{}{}", "<A>".repeat(17), "</A>".repeat(17));
    assert!(Element::parse(&nested).is_err());
    let children = format!("<A>{}</A>", "<B/>".repeat(1024));
    assert!(Element::parse(&children).is_err());
}

#[test]
fn hash_matches_standard_vector_and_legacy_integer_verification_uses_little_endian() {
    let digest = sha256(b"abc");
    assert_eq!(
        digest,
        [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad
        ]
    );
    // Exponent one isolates byte interpretation without a private key.
    let key = LegacyIntegerPublicKey::parse(&format!("1,{}", "f".repeat(128))).unwrap();
    let signature: String = digest.iter().rev().map(|v| format!("{v:02x}")).collect();
    assert!(key.verify_sha256_le(b"abc", &signature));
    assert!(!key.verify_sha256_le(b"abcd", &signature));
    assert!(!key.verify_sha256_le(b"abc", "invalid"));
    for invalid in ["", "0,ff", "1,0", "1,1", "x,ff"] {
        assert!(LegacyIntegerPublicKey::parse(invalid).is_none());
    }
}
