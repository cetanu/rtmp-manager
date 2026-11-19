use crate::config::AppConfig;
use anyhow::{Result, bail};
use aws_lc_rs::{
    pbkdf2,
    rand::{SecureRandom, SystemRandom},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{io::Write, num::NonZeroU32};

const ITERATIONS: NonZeroU32 = NonZeroU32::new(600_000).unwrap();

pub fn hash_passphrase(passphrase: &str) -> Result<String> {
    if passphrase.chars().count() < 12 || passphrase.len() > 1024 {
        bail!("Backup passphrase must be at least 12 characters and at most 1024 bytes");
    }
    let mut salt = [0; 16];
    SystemRandom::new()
        .fill(&mut salt)
        .map_err(|_| anyhow::anyhow!("Unable to generate backup salt"))?;
    let mut hash = [0; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        ITERATIONS,
        &salt,
        passphrase.as_bytes(),
        &mut hash,
    );
    Ok(format!(
        "{}:{}",
        STANDARD.encode(salt),
        STANDARD.encode(hash)
    ))
}

fn verify_passphrase(stored: &str, passphrase: &str) -> bool {
    let Some((salt, hash)) = stored.split_once(':') else {
        return false;
    };
    let (Ok(salt), Ok(hash)) = (STANDARD.decode(salt), STANDARD.decode(hash)) else {
        return false;
    };
    salt.len() == 16
        && hash.len() == 32
        && passphrase.len() <= 1024
        && pbkdf2::verify(
            pbkdf2::PBKDF2_HMAC_SHA256,
            ITERATIONS,
            &salt,
            passphrase.as_bytes(),
            &hash,
        )
        .is_ok()
}

pub fn encrypted_export(config: &AppConfig, passphrase: String) -> Result<Vec<u8>> {
    if !verify_passphrase(&config.web_auth.backup_passphrase_hash, &passphrase) {
        bail!("Incorrect backup passphrase");
    }
    let encryptor = age::Encryptor::with_user_passphrase(passphrase.into());
    let mut writer = encryptor.wrap_output(Vec::new())?;
    serde_json::to_writer_pretty(&mut writer, config)?;
    writer.flush()?;
    Ok(writer.finish()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn settings_preserve_hidden_passphrase_and_reject_short_replacements() {
        let config = AppConfig::default();
        let form = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str("web_auth%5Bbackup_passphrase%5D=a+separate+backup+passphrase")
            .unwrap();
        let updated = config.merge_form(form).unwrap();
        assert!(verify_passphrase(
            &updated.web_auth.backup_passphrase_hash,
            "a separate backup passphrase"
        ));
        let blank = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str("web_auth%5Bbackup_passphrase%5D=")
            .unwrap();
        assert_eq!(updated.merge_form(blank).unwrap(), updated);
        let short = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str("web_auth%5Bbackup_passphrase%5D=short")
            .unwrap();
        assert!(updated.merge_form(short).is_err());
    }

    #[test]
    fn passphrase_verification_and_encrypted_round_trip() {
        let passphrase = "a separate backup passphrase";
        let hash = hash_passphrase(passphrase).unwrap();
        assert_ne!(hash, hash_passphrase(passphrase).unwrap());
        assert!(verify_passphrase(&hash, passphrase));
        assert!(!verify_passphrase(&hash, "incorrect"));
        assert!(!verify_passphrase("", passphrase));
        assert!(hash_passphrase("short").is_err());
        let mut config = AppConfig::default();
        config.web_auth.backup_passphrase_hash = hash;
        config.web_auth.password = "private-dashboard-password".into();
        assert!(encrypted_export(&config, "incorrect".into()).is_err());
        let bytes = encrypted_export(&config, passphrase.into()).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("private-dashboard-password"));
        let decryptor = age::Decryptor::new(bytes.as_slice()).unwrap();
        let identity = age::scrypt::Identity::new(passphrase.to_owned().into());
        let mut reader = decryptor
            .decrypt(std::iter::once(&identity as &dyn age::Identity))
            .unwrap();
        let mut json = String::new();
        reader.read_to_string(&mut json).unwrap();
        assert_eq!(serde_json::from_str::<AppConfig>(&json).unwrap(), config);
        assert!(
            !serde_json::to_string(&config.redacted())
                .unwrap()
                .contains(&config.web_auth.backup_passphrase_hash)
        );
    }
}
