use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, OsRng, rand_core::RngCore},
};
use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

fn key() -> [u8; 32] {
    let value = key_material(
        std::env::var("MASTER_ENCRYPTION_KEY").ok().as_deref(),
        std::env::var("MASTER_ENCRYPTION_KEY_FILE").ok().as_deref(),
    );
    Sha256::digest(value.as_bytes()).into()
}

fn key_material(master_key: Option<&str>, key_file: Option<&str>) -> String {
    let value = master_key
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| key_file.and_then(|path| std::fs::read_to_string(path).ok()))
        .unwrap_or_else(|| "rtmp-manager-development-key-change-me".to_owned());
    value.trim().to_owned()
}

pub fn encrypt(value: &str) -> Result<String> {
    let cipher = Aes256Gcm::new_from_slice(&key()).expect("valid AES key");
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), value.as_bytes())
        .map_err(|_| anyhow::anyhow!("failed to encrypt secret"))?;
    let mut payload = nonce.to_vec();
    payload.extend(ciphertext);
    Ok(format!("enc:v1:{}", URL_SAFE_NO_PAD.encode(payload)))
}

pub fn decrypt(value: &str) -> Result<String> {
    if !value.starts_with("enc:v1:") {
        bail!("encrypted value has invalid format");
    }
    let payload = URL_SAFE_NO_PAD
        .decode(&value[7..])
        .context("invalid encrypted value")?;
    if payload.len() < 13 {
        bail!("encrypted value is too short");
    }
    let cipher = Aes256Gcm::new_from_slice(&key()).expect("valid AES key");
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&payload[..12]), &payload[12..])
        .map_err(|_| anyhow::anyhow!("failed to decrypt secret"))?;
    String::from_utf8(plaintext).context("decrypted value is not UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trips_without_exposing_plaintext() {
        let encrypted = encrypt("private-stream-key").unwrap();
        assert!(!encrypted.contains("private-stream-key"));
        assert_eq!(decrypt(&encrypted).unwrap(), "private-stream-key");
    }

    #[test]
    fn key_file_is_used_only_when_env_key_is_absent() {
        let path =
            std::env::temp_dir().join(format!("rtmp-manager-key-{}", crate::util::now_unix_ms()));
        std::fs::write(&path, b"file-key\n").unwrap();
        let path = path.to_str().unwrap();
        assert_eq!(key_material(None, Some(path)), "file-key");
        assert_eq!(key_material(Some("env-key"), Some(path)), "env-key");
        std::fs::remove_file(path).unwrap();
    }
}
