//! Lecture bornée des réponses HTTP.
//!
//! `reqwest::Response::bytes()` / `.text()` / `.json()` lisent tout le corps
//! avant que l'appelant puisse vérifier sa taille : un serveur hostile ou
//! défaillant peut remplir la mémoire. Ces aides imposent la limite pendant la
//! lecture, puis refusent tout dépassement.

use std::io::Read;

pub const MAX_JSON: u64 = 4 * 1024 * 1024;
pub const MAX_HTML: u64 = 8 * 1024 * 1024;
pub const MAX_IMAGE: u64 = 16 * 1024 * 1024;

/// Lit un corps en refusant plus de `max` octets. `Content-Length` est vérifié
/// quand il est présent, mais ne suffit jamais : le flux reste borné.
pub fn bytes(
    response: reqwest::blocking::Response,
    max: u64,
    label: &str,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > max)
    {
        return Err(format!("{label} trop volumineux"));
    }
    let mut body = Vec::new();
    response
        .take(max + 1)
        .read_to_end(&mut body)
        .map_err(|_| format!("{label} illisible"))?;
    if body.len() as u64 > max {
        return Err(format!("{label} trop volumineux"));
    }
    Ok(body)
}

pub fn text(
    response: reqwest::blocking::Response,
    max: u64,
    label: &str,
) -> Result<String, String> {
    let body = bytes(response, max, label)?;
    Ok(String::from_utf8_lossy(&body).into_owned())
}

pub fn json<T: serde::de::DeserializeOwned>(
    response: reqwest::blocking::Response,
    max: u64,
    label: &str,
) -> Result<T, String> {
    let body = bytes(response, max, label)?;
    serde_json::from_slice(&body).map_err(|_| format!("{label} illisible"))
}
