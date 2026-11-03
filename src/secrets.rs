use base64::{Engine, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    Key, XChaCha20Poly1305, XNonce,
    aead::{Aead, Generate, KeyInit},
};
use std::{fs, io, io::Write, path::Path};

// The key lives in its own file so a shared, synced or backed-up settings.json alone
// cannot reveal saved passwords. It does not protect against someone with the whole folder.
const KEY_FILE: &str = "secret.key";
const NONCE_LEN: usize = 24;

pub fn seal(config_dir: &Path, plaintext: &str) -> Result<String, String> {
    let cipher = cipher(config_dir, true)?;
    let nonce = XNonce::try_generate().map_err(|error| format!("Cannot create nonce: {error}"))?;
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| "Cannot encrypt password.".to_owned())?;

    let mut sealed = nonce.to_vec();
    sealed.extend(ciphertext);
    Ok(STANDARD.encode(sealed))
}

pub fn open(config_dir: &Path, sealed: &str) -> Result<String, String> {
    let cipher = cipher(config_dir, false)?;
    let sealed = STANDARD
        .decode(sealed)
        .map_err(|_| "Saved password is corrupt.".to_owned())?;
    if sealed.len() < NONCE_LEN {
        return Err("Saved password is corrupt.".to_owned());
    }

    let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
    let nonce = XNonce::try_from(nonce).map_err(|_| "Saved password is corrupt.".to_owned())?;
    let plaintext = cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|_| "Saved password cannot be decrypted on this device.".to_owned())?;
    String::from_utf8(plaintext).map_err(|_| "Saved password is corrupt.".to_owned())
}

fn cipher(config_dir: &Path, create_key: bool) -> Result<XChaCha20Poly1305, String> {
    let path = config_dir.join(KEY_FILE);
    match fs::read(&path) {
        Ok(bytes) => {
            let key = Key::try_from(bytes.as_slice())
                .map_err(|_| format!("{} is corrupt.", path.display()))?;
            return Ok(XChaCha20Poly1305::new(&key));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound && create_key => {}
        Err(error) => return Err(format!("Cannot read {}: {error}", path.display())),
    }

    let key = Key::try_generate().map_err(|error| format!("Cannot create key: {error}"))?;
    fs::create_dir_all(config_dir)
        .map_err(|error| format!("Cannot create {}: {error}", config_dir.display()))?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&path)
        .and_then(|mut file| file.write_all(&key))
        .map_err(|error| format!("Cannot write {}: {error}", path.display()))?;
    Ok(XChaCha20Poly1305::new(&key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn unique_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pad_commander_secrets_{name}_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn sealed_password_round_trips() {
        let dir = unique_dir("round_trip");

        let sealed = seal(&dir, "hunter2").expect("seal");

        assert_ne!(sealed, "hunter2");
        assert_eq!(open(&dir, &sealed).expect("open"), "hunter2");
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let dir = unique_dir("tampered");
        let sealed = seal(&dir, "hunter2").expect("seal");
        let mut bytes = STANDARD.decode(&sealed).expect("decode");
        let last = bytes.len() - 1;
        bytes[last] ^= 1;

        assert!(open(&dir, &STANDARD.encode(bytes)).is_err());
    }

    #[test]
    fn password_from_another_key_is_rejected() {
        let first = unique_dir("first_key");
        let second = unique_dir("second_key");
        let sealed = seal(&first, "hunter2").expect("seal");
        seal(&second, "other").expect("create second key");

        assert!(open(&second, &sealed).is_err());
    }

    #[test]
    fn open_without_key_file_fails() {
        let dir = unique_dir("missing_key");

        assert!(open(&dir, "AAAA").is_err());
    }
}
