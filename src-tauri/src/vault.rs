use std::io::{Read, Write};

use age::{Decryptor, Encryptor, Identity, secrecy::SecretString};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::Sha256;

const PBKDF2_ITERS: u32 = 120_000;

pub fn encrypt_api_key(api_key: &str, password: &str, device_salt: &str) -> Result<String> {
    let mut kdf_salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut kdf_salt);

    let passphrase = derive_passphrase(password, device_salt, &kdf_salt);
    let encryptor = Encryptor::with_user_passphrase(SecretString::from(passphrase));

    let mut encrypted = vec![];
    let mut writer = encryptor
        .wrap_output(&mut encrypted)
        .context("failed to initialize age writer")?;
    writer.write_all(api_key.as_bytes())?;
    writer.finish()?;

    Ok(format!("v2:{}:{}", STANDARD.encode(kdf_salt), STANDARD.encode(encrypted)))
}

pub fn decrypt_api_key(encrypted: &str, password: &str, device_salt: &str) -> Result<String> {
    let mut parts = encrypted.split(':');
    let version_or_salt = parts.next().ok_or_else(|| anyhow!("invalid encrypted value"))?;

    if version_or_salt != "v2" {
        return Err(anyhow!("unsupported key format; re-save key with current version"));
    }

    let salt_b64 = parts.next().ok_or_else(|| anyhow!("missing pbkdf2 salt"))?;
    let payload_b64 = parts.next().ok_or_else(|| anyhow!("missing age payload"))?;

    let salt = STANDARD.decode(salt_b64).context("invalid pbkdf2 salt")?;
    let payload = STANDARD.decode(payload_b64).context("invalid age payload")?;

    let passphrase = derive_passphrase(password, device_salt, &salt);
    let decryptor = Decryptor::new(payload.as_slice()).context("invalid age envelope")?;

    let passphrase_identity = age::scrypt::Identity::new(SecretString::from(passphrase));
    let mut reader = decryptor
        .decrypt(std::iter::once(&passphrase_identity as &dyn Identity))
        .map_err(|_| anyhow!("failed to decrypt key (wrong password/device salt?)"))?;

    let mut out = vec![];
    reader.read_to_end(&mut out)?;
    String::from_utf8(out).context("decrypted api key not utf-8")
}

fn derive_passphrase(password: &str, device_salt: &str, kdf_salt: &[u8]) -> String {
    let mut derived = [0u8; 32];
    let material = format!("{password}:{device_salt}");
    pbkdf2_hmac::<Sha256>(material.as_bytes(), kdf_salt, PBKDF2_ITERS, &mut derived);
    STANDARD.encode(derived)
}
