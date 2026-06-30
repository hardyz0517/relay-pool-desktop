pub mod crypto;
pub mod keychain;
pub mod mask;

#[derive(Clone)]
pub struct SecretManager {
    data_key: [u8; 32],
}

impl SecretManager {
    pub fn initialize() -> Result<Self, String> {
        Ok(Self {
            data_key: keychain::load_or_create_data_key()?,
        })
    }

    pub fn data_key(&self) -> &[u8; 32] {
        &self.data_key
    }
}
