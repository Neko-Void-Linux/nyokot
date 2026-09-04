use hmac::{Hmac, Mac};
use sha2::Sha256;

fn sign(secret: &[u8], body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[test]
fn valid_signature_accepts() {
    let body = br#"{"zen":"keep it simple"}"#;
    let sig = sign(b"secret", body);
    assert!(nyokot::github::verify_signature(b"secret", body, &sig));
}

#[test]
fn rejects_wrong_secret_tampered_body_and_malformed() {
    let body = br#"{"a":1}"#;
    let sig = sign(b"secret", body);
    // wrong secret
    assert!(!nyokot::github::verify_signature(b"other", body, &sig));
    // tampered body (one byte)
    assert!(!nyokot::github::verify_signature(
        b"secret",
        br#"{"a":2}"#,
        &sig
    ));
    // missing prefix
    assert!(!nyokot::github::verify_signature(
        b"secret",
        body,
        &sig[7..]
    ));
    // bad hex
    assert!(!nyokot::github::verify_signature(
        b"secret",
        body,
        "sha256=zzz"
    ));
    // empty header
    assert!(!nyokot::github::verify_signature(b"secret", body, ""));
    // truncated digest
    assert!(!nyokot::github::verify_signature(
        b"secret",
        body,
        "sha256=abcd"
    ));
}
