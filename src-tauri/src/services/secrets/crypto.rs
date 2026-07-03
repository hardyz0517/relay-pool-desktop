use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedPayload {
    pub ciphertext: String,
    pub nonce: String,
    pub aad: String,
    pub value_hash: String,
}

pub fn generate_data_key() -> [u8; 32] {
    let mut key = [0_u8; 32];
    OsRng.fill_bytes(&mut key);
    key
}

pub fn encrypt_secret(
    key: &[u8; 32],
    plaintext: &str,
    aad: &str,
) -> Result<EncryptedPayload, String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|error| format!("初始化加密器失败: {error}"))?;
    let mut nonce_bytes = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            nonce,
            aes_gcm::aead::Payload {
                msg: plaintext.as_bytes(),
                aad: aad.as_bytes(),
            },
        )
        .map_err(|error| format!("加密敏感信息失败: {error}"))?;

    Ok(EncryptedPayload {
        ciphertext: general_purpose::STANDARD.encode(ciphertext),
        nonce: general_purpose::STANDARD.encode(nonce_bytes),
        aad: aad.to_string(),
        value_hash: hash_secret(plaintext),
    })
}

pub fn decrypt_secret(key: &[u8; 32], payload: &EncryptedPayload) -> Result<String, String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|error| format!("初始化解密器失败: {error}"))?;
    let nonce_bytes = general_purpose::STANDARD
        .decode(&payload.nonce)
        .map_err(|error| format!("解析 nonce 失败: {error}"))?;
    let ciphertext = general_purpose::STANDARD
        .decode(&payload.ciphertext)
        .map_err(|error| format!("解析密文失败: {error}"))?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&nonce_bytes),
            aes_gcm::aead::Payload {
                msg: &ciphertext,
                aad: payload.aad.as_bytes(),
            },
        )
        .map_err(|_| "解密敏感信息失败，请检查系统凭据是否可用。".to_string())?;

    String::from_utf8(plaintext).map_err(|error| format!("解码敏感信息失败: {error}"))
}

pub fn hash_secret(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    general_purpose::STANDARD.encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_round_trip() {
        let key = generate_data_key();
        let payload = encrypt_secret(&key, "sk-p8-secret-plaintext-canary", "station_key:key-1:api_key")
            .expect("encrypt");

        assert_ne!(payload.ciphertext, "sk-p8-secret-plaintext-canary");
        let decrypted = decrypt_secret(&key, &payload).expect("decrypt");
        assert_eq!(decrypted, "sk-p8-secret-plaintext-canary");
    }

    #[test]
    fn decrypt_rejects_wrong_aad() {
        let key = generate_data_key();
        let mut payload = encrypt_secret(&key, "p8-password-canary", "station:station-1:login_password")
            .expect("encrypt");
        payload.aad = "station:station-2:login_password".to_string();

        let result = decrypt_secret(&key, &payload);

        assert!(result.is_err());
    }
}
