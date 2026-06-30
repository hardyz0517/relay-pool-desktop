use base64::{engine::general_purpose, Engine as _};
use keyring::Entry;

use super::crypto::generate_data_key;

const SERVICE: &str = "relay-pool-desktop";
const USERNAME: &str = "local-data-key-v1";

pub fn load_or_create_data_key() -> Result<[u8; 32], String> {
    let entry = Entry::new(SERVICE, USERNAME)
        .map_err(|error| format!("打开系统凭据失败: {error}"))?;
    match entry.get_password() {
        Ok(encoded) => decode_key(&encoded),
        Err(_) => {
            let key = generate_data_key();
            let encoded = general_purpose::STANDARD.encode(key);
            entry
                .set_password(&encoded)
                .map_err(|error| format!("保存系统凭据失败: {error}"))?;
            Ok(key)
        }
    }
}

fn decode_key(encoded: &str) -> Result<[u8; 32], String> {
    let bytes = general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| format!("解析系统凭据失败: {error}"))?;
    bytes
        .try_into()
        .map_err(|_| "系统凭据长度不正确。".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_key_rejects_invalid_length() {
        let result = decode_key("abc");

        assert!(result.is_err());
    }
}
