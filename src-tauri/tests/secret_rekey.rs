use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use relay_pool_desktop_lib::{
    canonical_secret_aad, BufferedSecretRekeyWriter, DeviceKeyId, DeviceKeyResolver,
    SecretKeyMaterial, SecretRecordSelector, SecretRekeyErrorCode, SecretRekeyPolicy,
    SecretRekeyRowPolicy, SecretRekeyService, VersionedEncryptedSecret,
    CURRENT_SECRET_ENCRYPTION_VERSION,
};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

const SOURCE_KEY: [u8; 32] = [7; 32];
const TARGET_KEY: [u8; 32] = [11; 32];
const WRONG_KEY: [u8; 32] = [19; 32];
const SOURCE_KEY_ID: &str = "source-device-key";
const TARGET_KEY_ID: &str = "target-device-key";
const CANARY: &[u8] = b"sk-p8-secret-plaintext-canary";

#[test]
fn rekey_streams_source_rows_to_target_key_with_fresh_nonce_per_row() {
    let rows = vec![
        encrypted_row(
            "secret-1",
            selector("station_key", "key-1", "api_key"),
            CANARY,
        ),
        encrypted_row(
            "secret-2",
            selector("station_key", "key-2", "api_key"),
            CANARY,
        ),
    ];
    let original_rows = rows.clone();
    let service = service(source_resolver(SOURCE_KEY), target_resolver(TARGET_KEY));
    let mut writer = BufferedSecretRekeyWriter::create_new();

    let report = service
        .rekey(
            rows.clone(),
            &SecretRekeyPolicy::include_all(),
            &mut writer,
            None,
        )
        .expect("rekey");

    assert_eq!(report.from_key_id, SOURCE_KEY_ID);
    assert_eq!(report.to_key_id, TARGET_KEY_ID);
    assert_eq!(report.included_rows, 2);
    assert_eq!(report.code, "ok");
    assert_eq!(rows, original_rows, "source input remains read-only");
    assert_eq!(writer.rows().len(), 2);
    assert_ne!(writer.rows()[0].nonce, original_rows[0].nonce);
    assert_ne!(writer.rows()[1].nonce, original_rows[1].nonce);
    assert_ne!(writer.rows()[0].nonce, writer.rows()[1].nonce);
    for row in writer.rows() {
        assert_eq!(row.key_id, TARGET_KEY_ID);
        assert_eq!(row.encryption_version, CURRENT_SECRET_ENCRYPTION_VERSION);
        assert_eq!(decrypt_row(row, TARGET_KEY), CANARY);
    }
}

#[test]
fn rekey_policy_is_explicit_include_drop_or_reset_without_callbacks() {
    let include = selector("station_key", "key-1", "api_key");
    let drop = selector("station_credentials", "station-1", "access_token");
    let reset = selector("station_credentials", "station-1", "cookie");
    let rows = vec![
        encrypted_row("include", include.clone(), b"include-secret"),
        encrypted_row("drop", drop.clone(), b"drop-secret"),
        encrypted_row("reset", reset.clone(), b"reset-secret"),
    ];
    let policy = SecretRekeyPolicy::include_all()
        .set(drop, SecretRekeyRowPolicy::Drop)
        .set(reset, SecretRekeyRowPolicy::Reset);
    let service = service(source_resolver(SOURCE_KEY), target_resolver(TARGET_KEY));
    let mut writer = BufferedSecretRekeyWriter::create_new();

    let report = service
        .rekey(rows, &policy, &mut writer, None)
        .expect("policy rekey");

    assert_eq!(report.included_rows, 1);
    assert_eq!(report.dropped_rows, 1);
    assert_eq!(report.reset_rows, 1);
    assert_eq!(writer.rows().len(), 1);
    assert_eq!(writer.rows()[0].selector, include);
}

#[test]
fn rekey_rejects_wrong_aad_without_leaking_plaintext() {
    let mut row = encrypted_row(
        "secret-1",
        selector("station_key", "key-1", "api_key"),
        CANARY,
    );
    row.selector = selector("station_key", "key-1", "password");

    let error = rekey_single_error(row, source_resolver(SOURCE_KEY));

    assert_eq!(error.code(), SecretRekeyErrorCode::SourceDecryptFailed);
    assert_eq!(error.row_index(), Some(0));
    assert!(!format!("{error:?}").contains("plaintext-canary"));
    assert!(!error.to_string().contains("plaintext-canary"));
}

#[test]
fn rekey_rejects_wrong_key_material_and_unknown_key_id() {
    let row = encrypted_row(
        "secret-1",
        selector("station_key", "key-1", "api_key"),
        CANARY,
    );

    let wrong_material = rekey_single_error(row.clone(), source_resolver(WRONG_KEY));
    assert_eq!(
        wrong_material.code(),
        SecretRekeyErrorCode::SourceDecryptFailed
    );

    let mut unknown_key = row;
    unknown_key.key_id = "missing-source-key".to_string();
    let unknown = rekey_single_error(unknown_key, source_resolver(SOURCE_KEY));
    assert_eq!(unknown.code(), SecretRekeyErrorCode::UnknownSourceKey);
}

#[test]
fn rekey_rejects_invalid_nonce_and_unknown_encryption_version() {
    let mut invalid_nonce = encrypted_row(
        "secret-1",
        selector("station_key", "key-1", "api_key"),
        CANARY,
    );
    invalid_nonce.nonce.truncate(4);
    assert_eq!(
        rekey_single_error(invalid_nonce, source_resolver(SOURCE_KEY)).code(),
        SecretRekeyErrorCode::InvalidNonce
    );

    let mut unsupported = encrypted_row(
        "secret-2",
        selector("station_key", "key-2", "api_key"),
        CANARY,
    );
    unsupported.encryption_version = CURRENT_SECRET_ENCRYPTION_VERSION + 1;
    assert_eq!(
        rekey_single_error(unsupported, source_resolver(SOURCE_KEY)).code(),
        SecretRekeyErrorCode::UnsupportedEncryptionVersion
    );
}

#[test]
fn rekey_stops_at_middle_row_failure_after_writing_prior_rows() {
    let mut bad = encrypted_row("bad", selector("station_key", "bad", "api_key"), CANARY);
    bad.ciphertext[0] ^= 0x01;
    let rows = vec![
        encrypted_row("ok-1", selector("station_key", "ok-1", "api_key"), b"ok-1"),
        bad,
        encrypted_row("ok-2", selector("station_key", "ok-2", "api_key"), b"ok-2"),
    ];
    let service = service(source_resolver(SOURCE_KEY), target_resolver(TARGET_KEY));
    let mut writer = BufferedSecretRekeyWriter::create_new();

    let error = service
        .rekey(rows, &SecretRekeyPolicy::include_all(), &mut writer, None)
        .unwrap_err();

    assert_eq!(error.code(), SecretRekeyErrorCode::SourceDecryptFailed);
    assert_eq!(error.row_index(), Some(1));
    assert_eq!(writer.rows().len(), 1);
}

#[test]
fn rekey_fails_closed_for_destination_states_and_cancellation() {
    let rows = vec![encrypted_row(
        "secret-1",
        selector("station_key", "key-1", "api_key"),
        CANARY,
    )];
    let service = service(source_resolver(SOURCE_KEY), target_resolver(TARGET_KEY));

    let mut existing = BufferedSecretRekeyWriter::existing_destination();
    assert_eq!(
        service
            .rekey(
                rows.clone(),
                &SecretRekeyPolicy::include_all(),
                &mut existing,
                None
            )
            .unwrap_err()
            .code(),
        SecretRekeyErrorCode::DestinationExists
    );

    let mut read_only = BufferedSecretRekeyWriter::read_only_destination();
    assert_eq!(
        service
            .rekey(
                rows.clone(),
                &SecretRekeyPolicy::include_all(),
                &mut read_only,
                None
            )
            .unwrap_err()
            .code(),
        SecretRekeyErrorCode::InputReadOnly
    );

    let mut not_activatable = BufferedSecretRekeyWriter::not_activatable();
    assert_eq!(
        service
            .rekey(
                rows.clone(),
                &SecretRekeyPolicy::include_all(),
                &mut not_activatable,
                None
            )
            .unwrap_err()
            .code(),
        SecretRekeyErrorCode::OutputNotActivatable
    );

    let mut write_failure = BufferedSecretRekeyWriter::fail_on_write_index(0);
    assert_eq!(
        service
            .rekey(
                rows.clone(),
                &SecretRekeyPolicy::include_all(),
                &mut write_failure,
                None
            )
            .unwrap_err()
            .code(),
        SecretRekeyErrorCode::WriteFailed
    );

    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let mut writer = BufferedSecretRekeyWriter::create_new();
    let cancelled = service
        .rekey(
            rows,
            &SecretRekeyPolicy::include_all(),
            &mut writer,
            Some(&cancellation),
        )
        .unwrap_err();
    assert_eq!(cancelled.code(), SecretRekeyErrorCode::Cancelled);
    assert_eq!(writer.rows().len(), 0);
}

fn rekey_single_error(
    row: VersionedEncryptedSecret,
    source: DeviceKeyResolver,
) -> relay_pool_desktop_lib::SecretRekeyError {
    let service = service(source, target_resolver(TARGET_KEY));
    let mut writer = BufferedSecretRekeyWriter::create_new();
    service
        .rekey(
            vec![row],
            &SecretRekeyPolicy::include_all(),
            &mut writer,
            None,
        )
        .unwrap_err()
}

fn service(source: DeviceKeyResolver, target: DeviceKeyResolver) -> SecretRekeyService {
    SecretRekeyService::new(source, target, CURRENT_SECRET_ENCRYPTION_VERSION)
}

fn source_resolver(material: [u8; 32]) -> DeviceKeyResolver {
    resolver(SOURCE_KEY_ID, material)
}

fn target_resolver(material: [u8; 32]) -> DeviceKeyResolver {
    resolver(TARGET_KEY_ID, material)
}

fn resolver(id: &str, material: [u8; 32]) -> DeviceKeyResolver {
    DeviceKeyResolver::active(
        DeviceKeyId::new(id),
        SecretKeyMaterial::from_bytes(material),
        CURRENT_SECRET_ENCRYPTION_VERSION,
    )
}

fn selector(scope: &str, owner_id: &str, kind: &str) -> SecretRecordSelector {
    SecretRecordSelector::new(scope, owner_id, kind)
}

fn encrypted_row(
    id: &str,
    selector: SecretRecordSelector,
    plaintext: &[u8],
) -> VersionedEncryptedSecret {
    let aad = selector.aad(CURRENT_SECRET_ENCRYPTION_VERSION);
    let cipher = Aes256Gcm::new_from_slice(&SOURCE_KEY).expect("cipher");
    let nonce = [id.as_bytes()[0]; 12];
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            aes_gcm::aead::Payload {
                msg: plaintext,
                aad: aad.as_bytes(),
            },
        )
        .expect("encrypt fixture");
    VersionedEncryptedSecret {
        id: id.to_string(),
        selector,
        key_id: SOURCE_KEY_ID.to_string(),
        encryption_version: CURRENT_SECRET_ENCRYPTION_VERSION,
        ciphertext,
        nonce: nonce.to_vec(),
        value_hash: hash(plaintext),
    }
}

fn decrypt_row(row: &VersionedEncryptedSecret, key: [u8; 32]) -> Vec<u8> {
    assert_eq!(
        row.aad(),
        canonical_secret_aad(
            &row.selector.scope,
            &row.selector.owner_id,
            &row.selector.kind,
            row.encryption_version
        )
    );
    let cipher = Aes256Gcm::new_from_slice(&key).expect("cipher");
    cipher
        .decrypt(
            Nonce::from_slice(&row.nonce),
            aes_gcm::aead::Payload {
                msg: row.ciphertext.as_slice(),
                aad: row.aad().as_bytes(),
            },
        )
        .expect("decrypt target")
}

fn hash(value: &[u8]) -> String {
    base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        Sha256::digest(value),
    )
}
