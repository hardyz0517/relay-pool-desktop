use base64::{engine::general_purpose, Engine as _};

use crate::application::credentials::{
    CredentialError, CredentialVault, EncryptedSecret, SecretBytes,
};

use super::{crypto, mask::mask_secret};

pub(crate) struct DataKeyVault([u8; 32]);

impl DataKeyVault {
    pub(crate) fn new(data_key: [u8; 32]) -> Self {
        Self(data_key)
    }
}

impl CredentialVault for DataKeyVault {
    fn encrypt(
        &self,
        aad: &str,
        plaintext: SecretBytes,
    ) -> Result<EncryptedSecret, CredentialError> {
        let value = String::from_utf8(plaintext.as_bytes().to_vec())
            .map_err(|_| CredentialError::SecretValidationFailed)?;
        let payload =
            crypto::encrypt_secret(&self.0, &value, aad).map_err(|_| CredentialError::Internal)?;
        let ciphertext = general_purpose::STANDARD
            .decode(payload.ciphertext)
            .map_err(|_| CredentialError::Internal)?;
        let nonce = general_purpose::STANDARD
            .decode(payload.nonce)
            .map_err(|_| CredentialError::Internal)?;
        Ok(EncryptedSecret {
            ciphertext,
            nonce,
            masked_value: mask_secret(&value),
        })
    }

    fn decrypt(
        &self,
        aad: &str,
        encrypted: &EncryptedSecret,
    ) -> Result<SecretBytes, CredentialError> {
        let payload = crypto::EncryptedPayload {
            ciphertext: general_purpose::STANDARD.encode(&encrypted.ciphertext),
            nonce: general_purpose::STANDARD.encode(&encrypted.nonce),
            aad: aad.to_string(),
            value_hash: String::new(),
        };
        crypto::decrypt_secret(&self.0, &payload)
            .map(SecretBytes::from)
            .map_err(|_| CredentialError::SecretValidationFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AAD: &str = "station_key:key-1:api_key";

    #[test]
    fn vault_round_trip_preserves_secret_and_exposes_only_masked_value() {
        let vault = DataKeyVault::new([7; 32]);
        let secret = "sk-p8-secret-plaintext-canary";

        let encrypted = vault
            .encrypt(AAD, SecretBytes::from(secret.to_string()))
            .expect("encrypt secret");

        assert_eq!(encrypted.masked_value, "sk-...nary");
        assert_ne!(encrypted.ciphertext.as_slice(), secret.as_bytes());
        let decrypted = vault.decrypt(AAD, &encrypted).expect("decrypt secret");
        assert_eq!(decrypted.as_bytes(), secret.as_bytes());
    }

    #[test]
    fn vault_rejects_aad_mismatch() {
        let vault = DataKeyVault::new([11; 32]);
        let encrypted = vault
            .encrypt(AAD, SecretBytes::from("sk-p8-aad-canary".to_string()))
            .expect("encrypt secret");

        let result = vault.decrypt("station_key:key-2:api_key", &encrypted);

        assert!(matches!(
            result,
            Err(CredentialError::SecretValidationFailed)
        ));
    }
}
