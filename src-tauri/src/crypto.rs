//! Cifrado del archivo de cuentas.
//!
//! Formato propio (`accounts.dat`):
//! `NRAM` | versión (1) | modo (1) | payload
//!  - modo 0: DPAPI del usuario actual de Windows
//!  - modo 1: contraseña maestra -> Argon2id (salt 16) + XSalsa20-Poly1305 (nonce 24)
//!  - modo 2: texto plano (solo si el usuario lo pide expresamente)
//!
//! También se pueden leer los `AccountData.json` de versiones anteriores para migrarlos.

use argon2::{Algorithm, Argon2, Params, Version};
use crypto_secretbox::aead::{Aead, KeyInit};
use crypto_secretbox::{Nonce, XSalsa20Poly1305};
use rand::RngCore;
use sha2::{Digest, Sha512};

pub const MAGIC: &[u8; 4] = b"NRAM";
pub const VERSION: u8 = 1;
const DPAPI_ENTROPY: &[u8] = b"Nuulz.RobloxAccountManager.v4";

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultMode {
    Windows,
    Password,
    Plain,
}

impl VaultMode {
    fn byte(self) -> u8 {
        match self {
            VaultMode::Windows => 0,
            VaultMode::Password => 1,
            VaultMode::Plain => 2,
        }
    }
}

/// Clave derivada de la contraseña maestra (se queda en memoria mientras la app está abierta).
#[derive(Clone)]
pub struct MasterKey {
    salt: [u8; 16],
    key: [u8; 32],
}

impl MasterKey {
    pub fn derive(password: &str, salt: Option<[u8; 16]>) -> Result<Self, String> {
        let salt = salt.unwrap_or_else(|| {
            let mut s = [0u8; 16];
            rand::thread_rng().fill_bytes(&mut s);
            s
        });
        let params = Params::new(64 * 1024, 3, 1, Some(32)).map_err(|e| e.to_string())?;
        let mut key = [0u8; 32];
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(password.as_bytes(), &salt, &mut key)
            .map_err(|e| e.to_string())?;
        Ok(Self { salt, key })
    }
}

pub fn header_mode(data: &[u8]) -> Option<VaultMode> {
    if data.len() < 6 || &data[..4] != MAGIC {
        return None;
    }
    match data[5] {
        0 => Some(VaultMode::Windows),
        1 => Some(VaultMode::Password),
        2 => Some(VaultMode::Plain),
        _ => None,
    }
}

pub fn password_salt(data: &[u8]) -> Option<[u8; 16]> {
    if header_mode(data)? != VaultMode::Password || data.len() < 6 + 16 {
        return None;
    }
    data[6..22].try_into().ok()
}

pub fn seal(plain: &[u8], mode: VaultMode, key: Option<&MasterKey>) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(plain.len() + 64);
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.push(mode.byte());
    match mode {
        VaultMode::Plain => out.extend_from_slice(plain),
        VaultMode::Windows => out.extend_from_slice(&dpapi_protect(plain, DPAPI_ENTROPY)?),
        VaultMode::Password => {
            let key = key.ok_or("Falta la contraseña maestra")?;
            let mut nonce = [0u8; 24];
            rand::thread_rng().fill_bytes(&mut nonce);
            let cipher = XSalsa20Poly1305::new((&key.key).into());
            let ct = cipher
                .encrypt(Nonce::from_slice(&nonce), plain)
                .map_err(|_| "Error de cifrado".to_string())?;
            out.extend_from_slice(&key.salt);
            out.extend_from_slice(&nonce);
            out.extend_from_slice(&ct);
        }
    }
    Ok(out)
}

pub fn open(data: &[u8], key: Option<&MasterKey>) -> Result<Vec<u8>, String> {
    let mode = header_mode(data).ok_or("Archivo de cuentas con formato desconocido")?;
    let body = &data[6..];
    match mode {
        VaultMode::Plain => Ok(body.to_vec()),
        VaultMode::Windows => dpapi_unprotect(body, Some(DPAPI_ENTROPY)),
        VaultMode::Password => {
            let key = key.ok_or("Se requiere la contraseña maestra")?;
            if body.len() < 16 + 24 + 16 {
                return Err("Archivo de cuentas dañado".into());
            }
            let nonce = &body[16..40];
            XSalsa20Poly1305::new((&key.key).into())
                .decrypt(Nonce::from_slice(nonce), &body[40..])
                .map_err(|_| "Contraseña incorrecta".to_string())
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Compatibilidad con archivos de versiones anteriores (solo lectura, para migrar)
// ---------------------------------------------------------------------------------------------

/// Cabecera fija que escribían las versiones anteriores al cifrar con contraseña.
const LEGACY_HEADER_LEN: usize = 64;
const LEGACY_HEADER_PREFIX: &[u8] = b"Roblox Account Manager created by ";
/// Entropía DPAPI usada por las versiones anteriores. Necesaria para poder leer sus archivos.
const LEGACY_ENTROPY: &[u8] = &[
    0x52, 0x4f, 0x42, 0x4c, 0x4f, 0x58, 0x20, 0x41, 0x43, 0x43, 0x4f, 0x55, 0x4e, 0x54, 0x20, 0x4d, 0x41, 0x4e, 0x41,
    0x47, 0x45, 0x52, 0x20, 0x7c, 0x20, 0x3a, 0x29, 0x20, 0x7c, 0x20, 0x42, 0x52, 0x4f, 0x55, 0x47, 0x48, 0x54, 0x20,
    0x54, 0x4f, 0x20, 0x59, 0x4f, 0x55, 0x20, 0x42, 0x55, 0x59, 0x20, 0x69, 0x63, 0x33, 0x77, 0x30, 0x6c, 0x66,
];

pub fn legacy_is_password_locked(data: &[u8]) -> bool {
    data.len() > LEGACY_HEADER_LEN && data.starts_with(LEGACY_HEADER_PREFIX)
}

/// Lee un AccountData.json antiguo sin contraseña (DPAPI o JSON plano).
pub fn legacy_open_unlocked(data: &[u8]) -> Result<Vec<u8>, String> {
    if let Ok(plain) = dpapi_unprotect(data, Some(LEGACY_ENTROPY)) {
        return Ok(plain);
    }
    let trimmed = data.iter().skip_while(|b| b.is_ascii_whitespace()).copied().next();
    if trimmed == Some(b'[') || data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Ok(data.to_vec());
    }
    Err("No se pudo descifrar el archivo antiguo (¿fue creado en otro PC o usuario?)".into())
}

/// Lee un AccountData.json antiguo protegido con contraseña.
pub fn legacy_open_password(data: &[u8], password: &str) -> Result<Vec<u8>, String> {
    if !legacy_is_password_locked(data) || data.len() < LEGACY_HEADER_LEN + 16 + 24 + 16 {
        return Err("El archivo no está protegido con contraseña".into());
    }
    let salt = &data[LEGACY_HEADER_LEN..LEGACY_HEADER_LEN + 16];
    let nonce = &data[LEGACY_HEADER_LEN + 16..LEGACY_HEADER_LEN + 40];
    let ct = &data[LEGACY_HEADER_LEN + 40..];
    let hashed = Sha512::digest(password.as_bytes());
    // Las versiones anteriores usaban libsodium "moderate"; según la build era Argon2id o Argon2i.
    for alg in [Algorithm::Argon2id, Algorithm::Argon2i] {
        let params = Params::new(128 * 1024, 6, 1, Some(32)).map_err(|e| e.to_string())?;
        let mut key = [0u8; 32];
        if Argon2::new(alg, Version::V0x13, params)
            .hash_password_into(&hashed, salt, &mut key)
            .is_err()
        {
            continue;
        }
        if let Ok(plain) = XSalsa20Poly1305::new((&key).into()).decrypt(Nonce::from_slice(nonce), ct) {
            return Ok(plain);
        }
    }
    Err("Contraseña incorrecta".into())
}

#[cfg(not(windows))]
fn dpapi_protect(_data: &[u8], _entropy: &[u8]) -> Result<Vec<u8>, String> {
    Err("El cifrado con la cuenta de Windows solo existe en Windows".into())
}

#[cfg(not(windows))]
fn dpapi_unprotect(_data: &[u8], _entropy: Option<&[u8]>) -> Result<Vec<u8>, String> {
    Err("El cifrado con la cuenta de Windows solo existe en Windows".into())
}

// ---------------------------------------------------------------------------------------------
// DPAPI
// ---------------------------------------------------------------------------------------------

#[cfg(windows)]
fn dpapi_protect(data: &[u8], entropy: &[u8]) -> Result<Vec<u8>, String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let ent = CRYPT_INTEGER_BLOB { cbData: entropy.len() as u32, pbData: entropy.as_ptr() as *mut u8 };
        let mut output = CRYPT_INTEGER_BLOB::default();
        CryptProtectData(&input, PCWSTR::null(), Some(&ent), None, None, 0, &mut output)
            .map_err(|e| format!("DPAPI: {e}"))?;
        let out = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Ok(out)
    }
}

#[cfg(windows)]
fn dpapi_unprotect(data: &[u8], entropy: Option<&[u8]>) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let ent = entropy.map(|e| CRYPT_INTEGER_BLOB { cbData: e.len() as u32, pbData: e.as_ptr() as *mut u8 });
        let mut output = CRYPT_INTEGER_BLOB::default();
        CryptUnprotectData(
            &input,
            None,
            ent.as_ref().map(|e| e as *const _),
            None,
            None,
            0,
            &mut output,
        )
        .map_err(|e| format!("DPAPI: {e}"))?;
        let out = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let key = MasterKey::derive("hunter22", None).unwrap();
        let sealed = seal(b"[1,2,3]", VaultMode::Password, Some(&key)).unwrap();
        assert_eq!(header_mode(&sealed), Some(VaultMode::Password));
        let salt = password_salt(&sealed).unwrap();
        let again = MasterKey::derive("hunter22", Some(salt)).unwrap();
        assert_eq!(open(&sealed, Some(&again)).unwrap(), b"[1,2,3]");
        let wrong = MasterKey::derive("nope", Some(salt)).unwrap();
        assert!(open(&sealed, Some(&wrong)).is_err());
    }

    #[test]
    fn windows_roundtrip() {
        let sealed = seal(b"hola", VaultMode::Windows, None).unwrap();
        assert_eq!(open(&sealed, None).unwrap(), b"hola");
    }
}
