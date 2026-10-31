//! Couvertures des jeux de boutiques, téléchargées une fois puis lues du disque.
//!
//! Steam et GOG publient une URL de couverture dans leurs réponses ; Epic ne
//! publie pas d'URL dérivable, donc elle est demandée à Legendary pour le seul
//! jeu concerné puis mémorisée dans la bibliothèque des boutiques. Aucune clé
//! n'est utilisée et aucune requête n'est faite au moment du rendu.

use super::model::{Provider, ProviderGame, ProviderGameKey};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Dossier des couvertures de boutiques, à côté des artworks des jeux Quark.
pub fn cache_dir() -> PathBuf {
    crate::config::artwork_root().join("provider-covers")
}

fn provider_stem(provider: Provider) -> &'static str {
    match provider {
        Provider::Steam => "steam",
        Provider::Epic => "epic",
        Provider::Gog => "gog",
    }
}

/// Nom de fichier stable : l'identifiant de boutique peut contenir des
/// caractères qui n'ont pas leur place dans un nom de fichier.
fn safe_id(product_id: &str) -> String {
    product_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

pub fn cache_path(key: &ProviderGameKey) -> PathBuf {
    cache_dir().join(format!(
        "{}-{}.png",
        provider_stem(key.provider),
        safe_id(&key.product_id)
    ))
}

/// Couverture déjà téléchargée (fichier non vide), ou rien.
pub fn cached(key: &ProviderGameKey) -> Option<PathBuf> {
    let path = cache_path(key);
    let usable = std::fs::metadata(&path)
        .map(|meta| meta.len() > 512)
        .unwrap_or(false);
    usable.then_some(path)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| "dossier de couvertures introuvable".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|_| "impossible de créer le dossier de couvertures".to_string())?;
    let temp = parent.join(format!(
        ".{}-{}.tmp",
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("cover"),
        std::process::id()
    ));
    let mut file = std::fs::File::create(&temp)
        .map_err(|_| "impossible d'écrire la couverture".to_string())?;
    if file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .is_err()
    {
        let _ = std::fs::remove_file(&temp);
        return Err("impossible d'écrire la couverture".to_string());
    }
    drop(file);
    std::fs::rename(&temp, path).map_err(|_| {
        let _ = std::fs::remove_file(&temp);
        "impossible de remplacer la couverture".to_string()
    })
}

/// Télécharge l'image et la réécrit en PNG : le format servi par la boutique
/// (jpeg, webp) n'a plus d'importance, et une image illisible est refusée
/// plutôt que mise en cache.
pub fn download(url: &str, key: &ProviderGameKey) -> Result<PathBuf, String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("couverture de boutique sans adresse valide".to_string());
    }
    let response = crate::artwork::http()
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .map_err(|_| "couverture de boutique injoignable".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "couverture refusée par la boutique (HTTP {})",
            response.status().as_u16()
        ));
    }
    let bytes = response
        .bytes()
        .map_err(|_| "couverture de boutique illisible".to_string())?;
    if bytes.len() > 12 * 1024 * 1024 {
        return Err("couverture de boutique trop volumineuse".to_string());
    }
    let image = image::load_from_memory(&bytes)
        .map_err(|_| "couverture de boutique illisible".to_string())?;
    if image.width() < 96 || image.height() < 96 {
        return Err("couverture de boutique trop petite".to_string());
    }
    let mut buffer = Cursor::new(Vec::new());
    image
        .write_to(&mut buffer, image::ImageFormat::Png)
        .map_err(|_| "couverture de boutique non convertible".to_string())?;
    let path = cache_path(key);
    write_atomic(&path, &buffer.into_inner())?;
    Ok(path)
}

/// Couverture d'un jeu : celle du cache, sinon téléchargée depuis l'URL connue
/// ou demandée à Legendary pour Epic. Retourne aussi l'URL découverte, à
/// mémoriser dans la bibliothèque pour ne pas relancer Legendary.
pub fn ensure(game: &ProviderGame) -> Result<(PathBuf, Option<String>), String> {
    if let Some(path) = cached(&game.key) {
        return Ok((path, None));
    }
    let url = match game.cover_url.clone() {
        Some(url) => Some(url),
        None if game.key.provider == Provider::Epic => {
            Some(super::legendary::epic_cover_url(&game.key.product_id)?)
        }
        None => None,
    };
    let Some(url) = url else {
        return Err("cette boutique ne publie pas de couverture".to_string());
    };
    let path = download(&url, &game.key)?;
    Ok((path, Some(url)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(provider: Provider, id: &str) -> ProviderGameKey {
        ProviderGameKey {
            provider,
            product_id: id.into(),
        }
    }

    #[test]
    fn chaque_boutique_a_un_nom_de_fichier_distinct_et_sur() {
        let steam = cache_path(&key(Provider::Steam, "620"));
        let epic = cache_path(&key(Provider::Epic, "../../etc/passwd"));
        assert!(steam.ends_with("steam-620.png"));
        assert!(epic.ends_with("epic-.._.._etc_passwd.png"));
        assert_ne!(steam, epic);
    }

    #[test]
    fn une_couverture_absente_ou_vide_n_est_pas_consideree_comme_cachee() {
        // Le dossier de cache réel n'est pas touché : on teste seulement la
        // décision sur un chemin qui n'existe pas.
        let missing = cache_path(&key(Provider::Gog, "does-not-exist-42"));
        let _ = std::fs::remove_file(&missing);
        assert!(cached(&key(Provider::Gog, "does-not-exist-42")).is_none());
    }

    #[test]
    fn une_adresse_non_http_est_refusee_avant_toute_requete() {
        let result = download("file:///etc/passwd", &key(Provider::Steam, "1"));
        assert!(result.unwrap_err().contains("adresse"));
    }
}
