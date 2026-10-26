use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use minisign_verify::{PublicKey, Signature};
use std::{env, fs, process};

fn fail(message: impl AsRef<str>) -> ! {
    eprintln!("Updater verification failed: {}", message.as_ref());
    process::exit(1);
}

fn decode_wrapped_text(value: &str, label: &str) -> String {
    let decoded = BASE64
        .decode(value.trim())
        .unwrap_or_else(|error| fail(format!("invalid base64 {label}: {error}")));
    String::from_utf8(decoded)
        .unwrap_or_else(|error| fail(format!("invalid UTF-8 {label}: {error}")))
}

fn main() {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() != 4 {
        fail("usage: verify_update <payload> <signature> <public-key> <version>");
    }

    let payload = fs::read(&arguments[0])
        .unwrap_or_else(|error| fail(format!("cannot read update payload: {error}")));
    let signature_wrapper = fs::read_to_string(&arguments[1])
        .unwrap_or_else(|error| fail(format!("cannot read update signature: {error}")));
    let public_key_text = decode_wrapped_text(&arguments[2], "public key");
    let signature_text = decode_wrapped_text(&signature_wrapper, "signature");
    let public_key = PublicKey::decode(&public_key_text)
        .unwrap_or_else(|error| fail(format!("invalid minisign public key: {error}")));
    let signature = Signature::decode(&signature_text)
        .unwrap_or_else(|error| fail(format!("invalid minisign signature: {error}")));
    public_key
        .verify(&payload, &signature, true)
        .unwrap_or_else(|error| fail(format!("signature does not match payload: {error}")));

    let signed_version = signature
        .trusted_comment()
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"));
    if signed_version != Some(arguments[3].as_str()) {
        fail(format!(
            "signed version {:?} does not match manifest version {}",
            signed_version, arguments[3]
        ));
    }

    println!("Updater payload signature and signed version are valid.");
}
