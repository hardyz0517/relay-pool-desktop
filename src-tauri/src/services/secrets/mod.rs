pub mod crypto;
pub mod keychain;
pub mod mask;
pub mod validation;
pub(crate) mod vault;

use crate::background_tasks::BlockingExecutor;

#[derive(Clone)]
pub struct SecretManager {
    data_key: [u8; 32],
}

impl SecretManager {
    pub async fn initialize(blocking: BlockingExecutor) -> Result<Self, String> {
        let data_key = blocking
            .submit("keyring_data_key_load_or_create", None, None, None, |_| {
                Ok(keychain::load_or_create_data_key())
            })
            .map_err(|error| format!("system credential task failed: {error}"))?
            .result()
            .await
            .map_err(|error| format!("system credential task failed: {error}"))??;
        Ok(Self { data_key })
    }

    pub fn data_key(&self) -> &[u8; 32] {
        &self.data_key
    }
}
