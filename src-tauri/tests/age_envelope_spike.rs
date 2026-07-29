use std::io::{Cursor, Read, Write};

use age::{
    scrypt, secrecy::SecretString, DecryptError, Decryptor, EncryptError, Encryptor, Identity,
    Recipient,
};

const TEST_PASSPHRASE: &str = "RPD_TEST_portable_migration_passphrase";
const WRONG_PASSPHRASE: &str = "RPD_TEST_wrong_passphrase";
const PLAINTEXT: &[u8] = b"RPD_TEST portable migration framing payload";
const TEST_WORK_FACTOR: u8 = 10;
const MAX_ACCEPTED_WORK_FACTOR: u8 = 12;
const EXCESSIVE_WORK_FACTOR: u8 = 63;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpikeReadError {
    AuthenticationFailed,
    ExcessiveWork,
    InvalidFormat,
    Io,
}

fn passphrase(value: &str) -> SecretString {
    SecretString::from(value.to_owned())
}

fn encrypt_with_fixed_work_factor() -> Result<Vec<u8>, EncryptError> {
    let mut recipient = scrypt::Recipient::new(passphrase(TEST_PASSPHRASE));
    recipient.set_work_factor(TEST_WORK_FACTOR);
    let encryptor = Encryptor::with_recipients(std::iter::once(&recipient as &dyn Recipient))?;
    let mut encrypted = Vec::new();
    let mut writer = encryptor.wrap_output(&mut encrypted)?;
    writer.write_all(PLAINTEXT)?;
    writer.finish()?;
    Ok(encrypted)
}

fn decrypt_all(
    encrypted: &[u8],
    password: &str,
    max_work_factor: u8,
) -> Result<Vec<u8>, SpikeReadError> {
    let decryptor =
        Decryptor::new_buffered(Cursor::new(encrypted)).map_err(classify_decrypt_error)?;
    let mut identity = scrypt::Identity::new(passphrase(password));
    identity.set_max_work_factor(max_work_factor);
    let mut reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn Identity))
        .map_err(classify_decrypt_error)?;
    let mut plaintext = Vec::new();
    reader
        .read_to_end(&mut plaintext)
        .map_err(|_| SpikeReadError::AuthenticationFailed)?;
    Ok(plaintext)
}

fn classify_decrypt_error(error: DecryptError) -> SpikeReadError {
    match error {
        DecryptError::DecryptionFailed
        | DecryptError::InvalidMac
        | DecryptError::KeyDecryptionFailed
        | DecryptError::NoMatchingKeys => SpikeReadError::AuthenticationFailed,
        DecryptError::ExcessiveWork { .. } => SpikeReadError::ExcessiveWork,
        DecryptError::InvalidHeader | DecryptError::UnknownFormat => SpikeReadError::InvalidFormat,
        DecryptError::Io(_) => SpikeReadError::Io,
        #[allow(unreachable_patterns)]
        _ => SpikeReadError::InvalidFormat,
    }
}

fn rewrite_scrypt_work_factor(mut encrypted: Vec<u8>, from: u8, to: u8) -> Vec<u8> {
    let header_end = encrypted
        .windows(4)
        .position(|window| window == b"\n---")
        .and_then(|start| {
            encrypted[start + 1..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map(|end| start + 2 + end)
        })
        .expect("age header terminator");
    let header = std::str::from_utf8(&encrypted[..header_end]).expect("text age header");
    let needle = format!(" {from}\n");
    let replacement = format!(" {to}\n");
    let rewritten = header.replacen(&needle, &replacement, 1);
    assert_ne!(
        rewritten, header,
        "test fixture must rewrite an scrypt work factor"
    );
    encrypted.splice(..header_end, rewritten.bytes());
    encrypted
}

#[test]
fn age_envelope_spike_passphrase_round_trip_streams_to_authenticated_eof() {
    let encrypted = encrypt_with_fixed_work_factor().expect("encrypt");
    let plaintext =
        decrypt_all(&encrypted, TEST_PASSPHRASE, MAX_ACCEPTED_WORK_FACTOR).expect("decrypt");
    assert_eq!(plaintext, PLAINTEXT);
}

#[test]
fn age_envelope_spike_rejects_excessive_scrypt_work_before_derivation() {
    let encrypted = encrypt_with_fixed_work_factor().expect("encrypt");
    let malicious = rewrite_scrypt_work_factor(encrypted, TEST_WORK_FACTOR, EXCESSIVE_WORK_FACTOR);
    let error = decrypt_all(&malicious, TEST_PASSPHRASE, MAX_ACCEPTED_WORK_FACTOR).unwrap_err();
    assert_eq!(error, SpikeReadError::ExcessiveWork);
}

#[test]
fn age_envelope_spike_wrong_password_and_truncation_use_one_public_class() {
    let encrypted = encrypt_with_fixed_work_factor().expect("encrypt");
    let wrong_password =
        decrypt_all(&encrypted, WRONG_PASSPHRASE, MAX_ACCEPTED_WORK_FACTOR).unwrap_err();

    let mut truncated = encrypted;
    truncated.truncate(truncated.len().saturating_sub(8));
    let truncated_error =
        decrypt_all(&truncated, TEST_PASSPHRASE, MAX_ACCEPTED_WORK_FACTOR).unwrap_err();

    assert_eq!(wrong_password, SpikeReadError::AuthenticationFailed);
    assert_eq!(truncated_error, SpikeReadError::AuthenticationFailed);
}
