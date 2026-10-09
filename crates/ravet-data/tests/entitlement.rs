use ed25519_dalek::{Signer, SigningKey};
use ravet_data::entitlement::*;
const PRODUCT: [u8; 8] = *b"SYNTHET1";
fn key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}
fn claims<'a>(features: &'a [&'a str]) -> EntitlementClaims<'a> {
    EntitlementClaims {
        product: PRODUCT,
        subject: "synthetic-test",
        not_before: 100,
        expires: 200,
        features,
    }
}
fn signed(claims: &EntitlementClaims<'_>) -> Vec<u8> {
    let payload = encode_claims(claims).unwrap();
    let message = signing_message(4, &payload).unwrap();
    assemble_token(4, &payload, &key().sign(&message).to_bytes()).unwrap()
}
fn verifier() -> EntitlementVerifier {
    EntitlementVerifier::new(PRODUCT, &[(4, key().verifying_key().to_bytes())]).unwrap()
}
#[test]
fn canonical_claims_authenticated_product_time_and_stale_cache_checks() {
    let a = claims(&["render", "edit"]);
    let b = claims(&["edit", "render"]);
    assert_eq!(encode_claims(&a).unwrap(), encode_claims(&b).unwrap());
    let token = signed(&a);
    let verifier = verifier();
    let grant = verifier.verify(&token, 100).unwrap();
    assert_eq!(grant.product(), PRODUCT);
    assert_eq!(grant.subject(), "synthetic-test");
    assert!(grant.allows("render", 199));
    assert!(!grant.allows("missing", 100));
    assert!(!grant.allows("render", 200));
    assert!(!grant.valid_at(99));
    assert_eq!(
        verifier.verify(&token, 99).unwrap_err(),
        EntitlementError::NotYetValid
    );
    assert_eq!(
        verifier.verify(&token, 200).unwrap_err(),
        EntitlementError::Expired
    );
    let foreign =
        EntitlementVerifier::new(*b"FOREIGN1", &[(4, key().verifying_key().to_bytes())]).unwrap();
    assert_eq!(
        foreign.verify(&token, 150).unwrap_err(),
        EntitlementError::WrongProduct
    );
}
#[test]
fn tampered_truncated_trailing_unknown_and_wrong_key_tokens_never_grant() {
    let token = signed(&claims(&["render"]));
    let verifier = verifier();
    for length in 0..token.len() {
        assert!(verifier.verify(&token[..length], 150).is_err());
    }
    for index in 0..token.len() {
        let mut bad = token.clone();
        bad[index] ^= 1;
        assert!(verifier.verify(&bad, 150).is_err(), "tampered byte {index}");
    }
    let mut trailing = token.clone();
    trailing.push(0);
    assert!(verifier.verify(&trailing, 150).is_err());
    assert_eq!(
        verifier.verify(&[0; 8193], 150).unwrap_err(),
        EntitlementError::InvalidRecord
    );
    let unknown =
        EntitlementVerifier::new(PRODUCT, &[(5, key().verifying_key().to_bytes())]).unwrap();
    assert_eq!(
        unknown.verify(&token, 150).unwrap_err(),
        EntitlementError::UnknownKey
    );
    let other = SigningKey::from_bytes(&[8; 32]);
    let wrong =
        EntitlementVerifier::new(PRODUCT, &[(4, other.verifying_key().to_bytes())]).unwrap();
    assert_eq!(
        wrong.verify(&token, 150).unwrap_err(),
        EntitlementError::InvalidSignature
    );
}
#[test]
fn malformed_unsigned_data_and_trust_reject_before_token_assembly() {
    let mut weak = [0; 32];
    weak[0] = 1;
    assert!(EntitlementVerifier::new(PRODUCT, &[(1, weak)]).is_err());
    assert!(EntitlementVerifier::new(PRODUCT, &[]).is_err());
    let public = key().verifying_key().to_bytes();
    assert!(EntitlementVerifier::new(PRODUCT, &[(1, public), (1, public)]).is_err());
    assert!(EntitlementVerifier::new(PRODUCT, &[(1, public); 33]).is_err());
    for features in [
        &["duplicate", "duplicate"][..],
        &[""][..],
        &["bad space"][..],
        &["é"][..],
    ] {
        assert!(encode_claims(&claims(features)).is_err());
    }
    let mut invalid = claims(&[]);
    invalid.subject = "control\n";
    assert!(encode_claims(&invalid).is_err());
    invalid.subject = "valid";
    invalid.expires = invalid.not_before;
    assert!(encode_claims(&invalid).is_err());
    let payload = encode_claims(&claims(&["edit", "render"])).unwrap();
    for length in 0..payload.len() {
        assert!(signing_message(4, &payload[..length]).is_err());
    }
    let mut extra = payload.clone();
    extra.push(0);
    assert!(signing_message(4, &extra).is_err());
    let mut wrong_version = payload;
    wrong_version[7] = b'2';
    assert!(assemble_token(4, &wrong_version, &[0; 64]).is_err());
}
