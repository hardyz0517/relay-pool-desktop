use base64::{engine::general_purpose, Engine as _};

use crate::application::credentials::{
    CredentialError, CredentialVault, EncryptedSecret, SecretBytes,
};

use super::{crypto, mask::mask_secret, DeviceKeyResolver};

pub(crate) struct DataKeyVault {
    resolver: DeviceKeyResolver,
}

impl DataKeyVault {
    pub(crate) fn new(resolver: DeviceKeyResolver) -> Self {
        Self { resolver }
    }

    #[cfg(test)]
    pub(crate) fn for_test(data_key: [u8; 32]) -> Self {
        Self::new(DeviceKeyResolver::for_test(data_key))
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
        let payload = self
            .resolver
            .with_active_key(|key| crypto::encrypt_secret(key, &value, aad))
            .map_err(|_| CredentialError::Internal)?
            .map_err(|_| CredentialError::Internal)?;
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
            key_id: self.resolver.active_key_id().as_str().to_string(),
            encryption_version: self.resolver.encryption_version(),
            value_hash: payload.value_hash,
        })
    }

    fn decrypt(
        &self,
        aad: &str,
        key_id: &str,
        encryption_version: u16,
        encrypted: &EncryptedSecret,
    ) -> Result<SecretBytes, CredentialError> {
        let payload = crypto::EncryptedPayload {
            ciphertext: general_purpose::STANDARD.encode(&encrypted.ciphertext),
            nonce: general_purpose::STANDARD.encode(&encrypted.nonce),
            aad: aad.to_string(),
            value_hash: encrypted.value_hash.clone(),
        };
        self.resolver
            .with_key(key_id, encryption_version, |key| {
                crypto::decrypt_secret(key, &payload)
            })
            .map_err(|_| CredentialError::SecretValidationFailed)?
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
        let vault = DataKeyVault::for_test([7; 32]);
        let secret = "sk-p8-secret-plaintext-canary";

        let encrypted = vault
            .encrypt(AAD, SecretBytes::from(secret.to_string()))
            .expect("encrypt secret");

        assert_eq!(encrypted.masked_value, "sk-p********nary");
        assert_ne!(encrypted.ciphertext.as_slice(), secret.as_bytes());
        let decrypted = vault
            .decrypt(
                AAD,
                &encrypted.key_id,
                encrypted.encryption_version,
                &encrypted,
            )
            .expect("decrypt secret");
        assert_eq!(decrypted.as_bytes(), secret.as_bytes());
    }

    #[test]
    fn vault_rejects_aad_mismatch() {
        let vault = DataKeyVault::for_test([11; 32]);
        let encrypted = vault
            .encrypt(AAD, SecretBytes::from("sk-p8-aad-canary".to_string()))
            .expect("encrypt secret");

        let result = vault.decrypt(
            "station_key:key-2:api_key:v1",
            &encrypted.key_id,
            encrypted.encryption_version,
            &encrypted,
        );

        assert!(matches!(
            result,
            Err(CredentialError::SecretValidationFailed)
        ));
    }
}
