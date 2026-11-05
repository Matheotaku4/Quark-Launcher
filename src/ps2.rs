//! Jeux PlayStation 2 : détection de PCSX2, lecture du numéro de série d'un
//! ISO, jaquettes communautaires et progression RetroAchievements.
//!
//! Un jeu PS2 reste un [`crate::config::Game`] ordinaire : `exe` pointe sur
//! l'image disque et `ps2` porte les informations propres à l'émulateur. Le
//! lancement passe par PCSX2 au lieu de Proton, sans prefix ni `wine`.

use crate::artwork::AssetPlan;
use crate::config::{Artwork, AssetKind, Game, Settings};
use crate::providers::{AchievementInfo, AchievementRow};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Identifiant du système PlayStation 2 dans l'API RetroAchievements.
pub const RA_PS2_CONSOLE_ID: u32 = 21;
/// Les jaquettes de xlenore/ps2-covers sont classées par numéro de série.
pub const COVER_BASE: &str =
    "https://raw.githubusercontent.com/xlenore/ps2-covers/main/covers/default";

/// Vrai pour une image disque que PCSX2 sait ouvrir. Le numéro de série n'est
/// lu automatiquement que dans les ISO non compressés.
pub fn is_disc_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .as_deref(),
        Some("iso" | "chd" | "cso" | "bin" | "cue" | "gz")
    )
}

/// Image dont on sait lire la table ISO9660 sans décompression.
pub fn is_readable_iso(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("iso"))
}

/// Titre présentable déduit du nom de fichier. Les suffixes de région et de
/// version entre parenthèses sont retirés, comme dans l'analyse de dossiers.
pub fn title_from_path(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default();
    let cleaned = crate::scan::clean_title(&stem.replace(['_', '.'], " "));
    if cleaned.trim().is_empty() {
        "Jeu PlayStation 2".to_string()
    } else {
        cleaned
    }
}

// ---------------------------------------------------------------------------
// Émulateur
// ---------------------------------------------------------------------------

/// Chemin de PCSX2 : réglage explicite, PATH, puis exports Flatpak.
pub fn detect_emulator(settings: &Settings) -> Option<PathBuf> {
    let explicit = settings.pcsx2_path.trim();
    if !explicit.is_empty() {
        let path = PathBuf::from(crate::config::expand_tilde(explicit));
        return path.is_file().then_some(path);
    }
    let mut names: Vec<PathBuf> = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for name in ["pcsx2-qt", "pcsx2", "PCSX2", "PCSX2-Qt"] {
                names.push(dir.join(name));
            }
        }
    }
    for root in [
        "/usr/bin",
        "/usr/local/bin",
        "/var/lib/flatpak/exports/bin",
        "/app/bin",
    ] {
        for name in ["pcsx2-qt", "pcsx2", "net.pcsx2.PCSX2"] {
            names.push(Path::new(root).join(name));
        }
    }
    if let Some(data) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir())
    {
        names.push(data.join("flatpak/exports/bin/net.pcsx2.PCSX2"));
    }
    names.into_iter().find(|path| path.is_file())
}

/// Étiquette lisible de l'émulateur, pour la fiche et le journal.
pub fn emulator_label(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    format!("PCSX2 · {name}")
}

/// Vérifie que PCSX2 peut être lancé, sans exiger que le jeu existe.
pub fn check_emulator(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!(
            "PCSX2 introuvable ({}). Installe-le ou indique son chemin dans Paramètres → \
             Boutiques & outils.",
            path.display()
        ));
    }
    Ok(())
}

/// Arguments passés à PCSX2. Le chemin de l'image est un argument positionnel.
pub fn launch_args(image: &Path) -> Vec<std::ffi::OsString> {
    vec![image.as_os_str().to_owned()]
}

// ---------------------------------------------------------------------------
// Numéro de série dans un ISO
// ---------------------------------------------------------------------------

/// Lit `SYSTEM.CNF` dans un ISO9660 et renvoie le numéro de série PS2
/// (`SLUS-20062`). Les images 2048 octets/secteur et 2352 (RAW) sont acceptées.
pub fn serial_from_iso(path: &Path) -> Result<String, String> {
    if !is_readable_iso(path) {
        return Err(
            "La lecture du numéro de série n'est automatique que pour les fichiers .iso. \
             Saisis-le à la main pour les images .chd, .cso, .bin/.cue ou .gz."
                .to_string(),
        );
    }
    let mut file = File::open(path).map_err(|error| format!("ISO illisible : {error}"))?;
    let layout = SectorLayout::detect(&mut file)
        .ok_or_else(|| "Ce fichier ne ressemble pas à un ISO9660 PS2.".to_string())?;
    let pvd = layout.read_sector(&mut file, 16)?;
    let root = directory_record(&pvd, 156)
        .ok_or_else(|| "ISO sans répertoire racine lisible.".to_string())?;
    let directory = layout.read_extent(&mut file, root.extent, root.size.min(1024 * 1024))?;
    let cnf = find_record(&directory, b"SYSTEM.CNF")
        .ok_or_else(|| "SYSTEM.CNF absent de l'ISO : ce n'est pas un jeu PS2.".to_string())?;
    let content = layout.read_extent(&mut file, cnf.extent, cnf.size.min(64 * 1024))?;
    serial_from_system_cnf(&content)
        .ok_or_else(|| "SYSTEM.CNF ne contient pas de numéro de série PS2.".to_string())
}

/// Extrait le numéro de série d'un `SYSTEM.CNF` PS2, même entouré d'autres
/// lignes (`BOOT2 = cdrom0:\SLUS_200.62;1`).
pub fn serial_from_system_cnf(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    for line in text.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_uppercase();
        if key != "BOOT2" && key != "BOOT" {
            continue;
        }
        if let Some(serial) = serial_from_boot_value(value) {
            return Some(serial);
        }
    }
    None
}

/// Normalise `cdrom0:\SLUS_200.62;1` en `SLUS-20062`.
pub fn serial_from_boot_value(value: &str) -> Option<String> {
    let boot = value
        .split(';')
        .next()?
        .rsplit(['\\', '/', ':'])
        .next()?
        .trim();
    let compact: String = boot
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase();
    if compact.len() != 9 {
        return None;
    }
    let (prefix, digits) = compact.split_at(4);
    if !prefix.chars().all(|ch| ch.is_ascii_alphabetic())
        || !digits.chars().all(|ch| ch.is_ascii_digit())
    {
        return None;
    }
    Some(format!("{prefix}-{digits}"))
}

/// Vrai pour un numéro de série PS2 plausible (`SLUS-20062`).
pub fn is_serial(serial: &str) -> bool {
    let compact: String = serial
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect();
    compact.len() == 9
        && compact[..4].chars().all(|ch| ch.is_ascii_alphabetic())
        && compact[4..].chars().all(|ch| ch.is_ascii_digit())
}

/// URL de la jaquette par défaut du dépôt ps2-covers.
pub fn cover_url(serial: &str) -> Option<String> {
    if !is_serial(serial) {
        return None;
    }
    let compact: String = serial
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase();
    let serial = format!("{}-{}", &compact[..4], &compact[4..]);
    Some(format!("{COVER_BASE}/{serial}.jpg"))
}

/// Télécharge la jaquette dans le dossier d'un jeu précis.
pub fn fetch_cover_for(client: &Client, game_id: &str, serial: &str) -> (Artwork, Vec<String>) {
    let Some(url) = cover_url(serial) else {
        return (
            Artwork::default(),
            vec!["numéro de série PS2 invalide".to_string()],
        );
    };
    let mut plan = AssetPlan::default();
    plan.add(AssetKind::Cover, url);
    crate::artwork::apply_plan(client, &crate::config::artwork_root().join(game_id), &plan)
}

// ---------------------------------------------------------------------------
// RetroAchievements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaGame {
    pub id: u64,
    pub title: String,
    pub achievements: u32,
}

/// Vrai quand le compte RetroAchievements est configuré.
pub fn ra_configured(settings: &Settings) -> bool {
    !settings.ra_username.trim().is_empty() && !settings.ra_api_key.trim().is_empty()
}

/// Cherche une fiche PS2 par titre dans la liste officielle du système.
///
/// RetroAchievements n'expose pas de recherche par numéro de série : le titre
/// reste la seule clé disponible. L'utilisateur peut corriger l'identifiant
/// proposé dans la fiche du jeu.
pub fn search_ra_game(
    client: &Client,
    settings: &Settings,
    title: &str,
) -> Result<Option<RaGame>, String> {
    if !ra_configured(settings) {
        return Err(
            "Renseigne le pseudo et la clé Web API RetroAchievements dans les paramètres."
                .to_string(),
        );
    }
    #[derive(Deserialize)]
    struct Entry {
        #[serde(rename = "ID")]
        id: u64,
        #[serde(rename = "Title", default)]
        title: String,
        #[serde(rename = "NumAchievements", default)]
        achievements: u32,
    }
    let response = client
        .get("https://retroachievements.org/API/API_GetGameList.php")
        .query(&[
            ("i", RA_PS2_CONSOLE_ID.to_string()),
            ("f", "1".to_string()),
            ("y", settings.ra_api_key.trim().to_string()),
        ])
        .send()
        .map_err(|_| "RetroAchievements injoignable.".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "RetroAchievements a refusé la requête (HTTP {}). Vérifie la clé Web API.",
            response.status().as_u16()
        ));
    }
    let entries: Vec<Entry> = crate::net::json(response, crate::net::MAX_JSON, "liste RA")
        .map_err(|_| {
            "Liste RetroAchievements illisible. Vérifie le pseudo et la clé Web API.".to_string()
        })?;
    let wanted = crate::config::normalized_search(title);
    let wanted = wanted.trim();
    if wanted.is_empty() {
        return Ok(None);
    }
    let mut best: Option<(&Entry, u8)> = None;
    for entry in &entries {
        let candidate = crate::config::normalized_search(&entry.title);
        let score = if candidate == wanted {
            3
        } else if candidate.contains(wanted) || wanted.contains(&candidate) {
            2
        } else {
            continue;
        };
        if best.is_none_or(|(_, best_score)| score > best_score) {
            best = Some((entry, score));
        }
    }
    Ok(best.map(|(entry, _)| RaGame {
        id: entry.id,
        title: entry.title.clone(),
        achievements: entry.achievements,
    }))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RaGameInfo {
    #[serde(rename = "NumAchievements")]
    total: u32,
    /// Objet quand des succès existent, tableau vide (`[]`) sinon : PHP ne
    /// distingue pas les deux. La conversion est faite à la lecture.
    #[serde(rename = "Achievements", default)]
    achievements: serde_json::Value,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RaAchievement {
    #[serde(rename = "ID")]
    id: u64,
    #[serde(rename = "Title")]
    title: String,
    #[serde(rename = "Description")]
    description: String,
    #[serde(rename = "BadgeName")]
    badge: String,
    #[serde(rename = "DateEarned")]
    date_earned: Option<String>,
    #[serde(rename = "DateEarnedHardcore")]
    date_earned_hardcore: Option<String>,
    #[serde(rename = "NumAwarded")]
    awarded: Option<u64>,
    #[serde(rename = "NumDistinctPlayers")]
    players: Option<u64>,
    #[serde(rename = "NumDistinctPlayersCasual")]
    players_casual: Option<u64>,
}

/// Progression RetroAchievements d'un jeu, dans la forme commune aux succès.
pub fn fetch_ra_achievements(
    client: &Client,
    settings: &Settings,
    game_id: u64,
) -> Result<AchievementInfo, String> {
    if !ra_configured(settings) {
        return Err(
            "RetroAchievements n'est pas configuré : Paramètres → Boutiques & outils.".to_string(),
        );
    }
    let response = client
        .get("https://retroachievements.org/API/API_GetGameInfoAndUserProgress.php")
        .query(&[
            ("g", game_id.to_string()),
            ("u", settings.ra_username.trim().to_string()),
            ("y", settings.ra_api_key.trim().to_string()),
        ])
        .send()
        .map_err(|_| "RetroAchievements injoignable.".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "RetroAchievements a refusé la requête (HTTP {}). Vérifie le pseudo et la clé.",
            response.status().as_u16()
        ));
    }
    let info: RaGameInfo = crate::net::json(response, crate::net::MAX_JSON, "succès RA")
        .map_err(|_| "Réponse RetroAchievements illisible.".to_string())?;
    let achievements = info
        .achievements
        .as_object()
        .map(|map| {
            map.values()
                .filter_map(|value| serde_json::from_value::<RaAchievement>(value.clone()).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut rows: Vec<AchievementRow> = achievements
        .into_iter()
        .map(|achievement| {
            let unlocked = achievement
                .date_earned
                .as_deref()
                .is_some_and(|date| !date.trim().is_empty())
                || achievement
                    .date_earned_hardcore
                    .as_deref()
                    .is_some_and(|date| !date.trim().is_empty());
            let rarity = achievement.awarded.and_then(|awarded| {
                let players = achievement
                    .players
                    .filter(|players| *players > 0)
                    .or(achievement.players_casual.filter(|players| *players > 0))?;
                let percent = awarded as f32 / players as f32 * 100.0;
                (0.0..=100.0).contains(&percent).then_some(percent)
            });
            AchievementRow {
                key: achievement.id.to_string(),
                id: achievement.id.to_string(),
                name: achievement.title,
                description: achievement.description,
                unlocked,
                progress: if unlocked { 1.0 } else { 0.0 },
                session: false,
                rarity,
                icon_url: (!achievement.badge.trim().is_empty()).then(|| {
                    format!(
                        "https://media.retroachievements.org/Badge/{}.png",
                        achievement.badge
                    )
                }),
                icon_path: None,
            }
        })
        .collect();
    rows.sort_by_key(|row| row.name.to_lowercase());
    let mut result = AchievementInfo {
        total: info.total.max(rows.len() as u32),
        unlocked: 0,
        rows,
    };
    result.normalize();
    Ok(result)
}

/// Ouvre la fiche d'un jeu sur RetroAchievements dans le navigateur.
pub fn open_ra_page(game_id: u64) {
    let url = format!("https://retroachievements.org/game/{game_id}");
    let _ = Command::new("xdg-open").arg(url).spawn();
}

// ---------------------------------------------------------------------------
// Lecture des secteurs ISO9660
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct SectorLayout {
    size: u64,
    data_offset: u64,
}

#[derive(Debug, Clone, Copy)]
struct DirectoryRecord {
    extent: u32,
    size: u32,
}

impl SectorLayout {
    fn detect(file: &mut File) -> Option<Self> {
        for (size, data_offset) in [(2048u64, 0u64), (2352, 24), (2336, 8)] {
            let offset = size.checked_mul(16)?.checked_add(data_offset)?;
            if file.seek(SeekFrom::Start(offset)).is_err() {
                continue;
            }
            let mut header = [0u8; 7];
            if file.read_exact(&mut header).is_err() {
                continue;
            }
            if header[0] == 1 && &header[1..6] == b"CD001" {
                return Some(Self { size, data_offset });
            }
        }
        None
    }

    fn read_sector(&self, file: &mut File, lba: u32) -> Result<Vec<u8>, String> {
        let offset = self
            .size
            .checked_mul(lba as u64)
            .and_then(|offset| offset.checked_add(self.data_offset))
            .ok_or_else(|| "ISO corrompu : secteur hors limites.".to_string())?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| error.to_string())?;
        let mut buffer = vec![0u8; 2048];
        file.read_exact(&mut buffer)
            .map_err(|error| error.to_string())?;
        Ok(buffer)
    }

    fn read_extent(&self, file: &mut File, lba: u32, size: u32) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(size as usize);
        let mut sector = lba;
        while out.len() < size as usize {
            let data = self.read_sector(file, sector)?;
            let remaining = size as usize - out.len();
            out.extend_from_slice(&data[..remaining.min(data.len())]);
            sector = sector.saturating_add(1);
        }
        Ok(out)
    }
}

fn directory_record(sector: &[u8], offset: usize) -> Option<DirectoryRecord> {
    let length = *sector.get(offset)? as usize;
    if length < 34 || offset + length > sector.len() {
        return None;
    }
    let extent = u32::from_le_bytes(sector[offset + 2..offset + 6].try_into().ok()?);
    let size = u32::from_le_bytes(sector[offset + 10..offset + 14].try_into().ok()?);
    Some(DirectoryRecord { extent, size })
}

fn find_record(directory: &[u8], wanted: &[u8]) -> Option<DirectoryRecord> {
    let mut offset = 0usize;
    while offset < directory.len() {
        let length = directory[offset] as usize;
        if length == 0 {
            // Fin d'un secteur : on avance au secteur suivant.
            offset = (offset / 2048 + 1) * 2048;
            continue;
        }
        if length < 34 || offset + length > directory.len() {
            break;
        }
        let name_len = directory[offset + 32] as usize;
        let name_start = offset + 33;
        if name_start + name_len <= directory.len() {
            let raw = &directory[name_start..name_start + name_len];
            // ISO9660 suffixe les fichiers avec « ;1 ».
            let name = raw.split(|byte| *byte == b';').next().unwrap_or(raw);
            if name.eq_ignore_ascii_case(wanted) {
                return directory_record(directory, offset);
            }
        }
        offset += length;
    }
    None
}

/// Exécute PCSX2 pour une image, ou renvoie un message actionnable.
pub fn check_launch(game: &Game, settings: &Settings) -> Result<PathBuf, String> {
    let image = game.exe_path();
    if !image.is_file() {
        return Err(format!("Image PS2 introuvable : {}", image.display()));
    }
    let emulator = detect_emulator(settings).ok_or_else(|| {
        "PCSX2 n'est pas détecté. Installe-le ou indique son chemin dans Paramètres → \
         Boutiques & outils."
            .to_string()
    })?;
    check_emulator(&emulator)?;
    Ok(emulator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_boot2_devient_un_numero_de_serie() {
        assert_eq!(
            serial_from_boot_value(r"cdrom0:\SLUS_200.62;1").as_deref(),
            Some("SLUS-20062")
        );
        assert_eq!(
            serial_from_boot_value(r"cdrom0:\SLES_123.45;1").as_deref(),
            Some("SLES-12345")
        );
        assert_eq!(
            serial_from_system_cnf(b"BOOT2 = cdrom0:\\SLUS_216.21;1\nVER = 1.00\n").as_deref(),
            Some("SLUS-21621")
        );
        assert_eq!(serial_from_boot_value("pas un boot"), None);
        assert_eq!(serial_from_boot_value("cdrom0:\\SLUS_20.62;1"), None);
    }

    #[test]
    fn la_jaquette_ps2_suit_le_numero_de_serie() {
        assert_eq!(
            cover_url("SLUS-20062").as_deref(),
            Some(
                "https://raw.githubusercontent.com/xlenore/ps2-covers/main/covers/default/SLUS-20062.jpg"
            )
        );
        assert!(cover_url("pas-un-serial").is_none());
    }

    #[test]
    fn le_titre_est_nettoye() {
        assert_eq!(
            title_from_path(Path::new("/jeux/Burnout 3 (USA) [SLUS-21050].iso")),
            "Burnout 3"
        );
    }

    #[test]
    fn un_petit_iso_livre_son_serial() {
        let mut image = vec![0u8; 24 * 2048];
        // PVD au secteur 16.
        let pvd = 16 * 2048;
        image[pvd] = 1;
        image[pvd + 1..pvd + 6].copy_from_slice(b"CD001");
        // Enregistrement du répertoire racine : secteur 18, 2048 octets.
        let root = pvd + 156;
        image[root] = 34;
        image[root + 2..root + 6].copy_from_slice(&18u32.to_le_bytes());
        image[root + 10..root + 14].copy_from_slice(&2048u32.to_le_bytes());
        // Entrée SYSTEM.CNF dans le répertoire racine.
        let dir = 18 * 2048;
        let entry = dir;
        image[entry] = 34 + 11;
        image[entry + 2..entry + 6].copy_from_slice(&20u32.to_le_bytes());
        image[entry + 10..entry + 14].copy_from_slice(&27u32.to_le_bytes());
        image[entry + 32] = 11;
        image[entry + 33..entry + 44].copy_from_slice(b"SYSTEM.CNF;");
        // Contenu de SYSTEM.CNF.
        let cnf = 20 * 2048;
        let content = b"BOOT2 = cdrom0:\\SLUS_200.62;1\n";
        image[cnf..cnf + content.len()].copy_from_slice(content);
        image[cnf + content.len()] = 0;

        let path = std::env::temp_dir().join(format!(
            "quark-ps2-{}-{}.iso",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|time| time.subsec_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&path, image).unwrap();
        assert_eq!(serial_from_iso(&path).as_deref(), Ok("SLUS-20062"));
        let _ = std::fs::remove_file(path);
    }
}
