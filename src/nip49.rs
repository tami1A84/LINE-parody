use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;
use base64::{Engine as _, engine::general_purpose};
use rand::Rng;
use rand::rngs::OsRng;

const PBKDF2_ROUNDS: u32 = 100_000;

/// Encrypts plaintext using NIP-49 spec.
/// Returns a tuple of `(encrypted_base64_string, salt_base64_string)`.
pub fn encrypt(
    plaintext: &[u8],
    passphrase: &str,
) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
    let mut salt_bytes = [0u8; 16];
    OsRng.fill(&mut salt_bytes);
    let salt_base64 = general_purpose::STANDARD.encode(salt_bytes);

    let mut derived_key_bytes = [0u8; 32];
    pbkdf2_hmac::<Sha256>(
        passphrase.as_bytes(),
        &salt_bytes,
        PBKDF2_ROUNDS,
        &mut derived_key_bytes,
    );
    let cipher_key = Key::from_slice(&derived_key_bytes);
    let cipher = ChaCha20Poly1305::new(cipher_key);

    let mut nonce_bytes: [u8; 12] = [0u8; 12];
    OsRng.fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext_with_tag = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("NIP-49 encryption error: {e:?}"))?;

    let mut encoded_data = ciphertext_with_tag;
    encoded_data.extend_from_slice(nonce_bytes.as_ref());

    let nip49_encoded = format!("#nip49:{}", general_purpose::STANDARD.encode(&encoded_data));

    Ok((nip49_encoded, salt_base64))
}

/// Encrypts plaintext using NIP-49 spec with a pre-existing salt.
/// Returns the encrypted_base64_string.
pub fn encrypt_with_salt(
    plaintext: &[u8],
    passphrase: &str,
    salt_base64: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let salt_bytes = general_purpose::STANDARD.decode(salt_base64)?;

    let mut derived_key_bytes = [0u8; 32];
    pbkdf2_hmac::<Sha256>(
        passphrase.as_bytes(),
        &salt_bytes,
        PBKDF2_ROUNDS,
        &mut derived_key_bytes,
    );
    let cipher_key = Key::from_slice(&derived_key_bytes);
    let cipher = ChaCha20Poly1305::new(cipher_key);

    let mut nonce_bytes: [u8; 12] = [0u8; 12];
    OsRng.fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext_with_tag = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("NIP-49 encryption error: {e:?}"))?;

    let mut encoded_data = ciphertext_with_tag;
    encoded_data.extend_from_slice(nonce_bytes.as_ref());

    let nip49_encoded = format!("#nip49:{}", general_purpose::STANDARD.encode(&encoded_data));

    Ok(nip49_encoded)
}


/// Decrypts a NIP-49 encoded string.
pub fn decrypt(
    nip49_encoded: &str,
    passphrase: &str,
    salt_base64: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    if !nip49_encoded.starts_with("#nip49:") {
        return Err("Invalid NIP-49 format".into());
    }

    let salt_bytes = general_purpose::STANDARD.decode(salt_base64)?;

    let mut derived_key_bytes = [0u8; 32];
    pbkdf2_hmac::<Sha256>(
        passphrase.as_bytes(),
        &salt_bytes,
        PBKDF2_ROUNDS,
        &mut derived_key_bytes,
    );
    let cipher_key = Key::from_slice(&derived_key_bytes);
    let cipher = ChaCha20Poly1305::new(cipher_key);

    let decoded_bytes = general_purpose::STANDARD.decode(&nip49_encoded[7..])?;
    if decoded_bytes.len() < 12 {
        return Err("Invalid NIP-49 payload".into());
    }

    let (ciphertext_and_tag, nonce_bytes) = decoded_bytes.split_at(decoded_bytes.len() - 12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let decrypted_bytes = cipher
        .decrypt(nonce, ciphertext_and_tag)
        .map_err(|_| "Incorrect passphrase or corrupted data")?;

    Ok(decrypted_bytes)
}
