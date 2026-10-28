//! Raccourcis non-Steam de Steam (`shortcuts.vdf`).
//!
//! L'overlay Steam n'est injecté que par le client Steam : pour un jeu ajouté
//! hors de Steam, il faut un raccourci non-Steam et confier le lancement à
//! Steam. Quark lit donc ces raccourcis lui-même, au lieu de demander un AppID
//! recopié à la main. La distinction est importante : un AppID du **magasin**
//! n'a pas de licence pour un jeu non possédé, et Steam répond alors
//! « no license » en refermant le jeu aussitôt.

use std::path::{Path, PathBuf};

/// Un raccourci non-Steam, tel que Steam l'a enregistré.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    /// Identifiant à passer à `steam -applaunch`.
    pub app_id: u32,
    pub name: String,
    /// Cible du raccourci (souvent le `.exe`, parfois un script).
    pub exe: String,
    pub start_dir: String,
    pub launch_options: String,
}

impl Shortcut {
    pub fn label(&self) -> String {
        format!("{} · {}", self.name, self.app_id)
    }
}

/// Ce que Steam fera réellement de l'identifiant choisi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayCheck {
    /// Le raccourci existe : Steam lance exactement cette cible.
    Shortcut { name: String, exe: String },
    /// AppID du magasin, jeu installé : Steam lance le jeu du magasin.
    StoreInstalled,
    /// Ni raccourci ni jeu installé : Steam répond « no license » et se
    /// referme aussitôt. C'est la cause d'un jeu qui « se ferme
    /// instantanément ».
    NoLicense,
    /// Aucun raccourci lisible (Steam absent, autre profil…) : rien à vérifier.
    Unknown,
}

impl OverlayCheck {
    /// Vrai quand l'identifiant ne peut pas fonctionner : inutile de lancer
    /// Steam pour rien.
    pub fn is_broken(&self) -> bool {
        *self == OverlayCheck::NoLicense
    }
}

// ---------------------------------------------------------------------------
// VDF binaire
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Value {
    String(String),
    Int(i32),
    UInt64(u64),
    Map(Vec<(String, Value)>),
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(count)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn i32(&mut self) -> Option<i32> {
        self.take(4)
            .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Option<u64> {
        let b = self.take(8)?;
        Some(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    /// Chaîne terminée par un octet nul (UTF-8 tolérant : un octet invalide
    /// donne un caractère de remplacement plutôt qu'un échec).
    fn cstr(&mut self) -> Option<String> {
        let rest = self.data.get(self.pos..)?;
        let end = rest.iter().position(|b| *b == 0)?;
        let text = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.pos += end + 1;
        Some(text)
    }

    /// Chaîne large (UTF-16LE) terminée par deux octets nuls. Rare dans les
    /// raccourcis, mais un fichier abîmé ne doit pas faire dérailler la lecture.
    fn wstr(&mut self) -> Option<String> {
        let mut units: Vec<u16> = Vec::new();
        loop {
            let b = self.take(2)?;
            let unit = u16::from_le_bytes([b[0], b[1]]);
            if unit == 0 {
                break;
            }
            units.push(unit);
        }
        Some(String::from_utf16_lossy(&units))
    }
}

/// Lit les paires clé/valeur d'une carte binaire, jusqu'au `0x08` qui la
/// referme. Toute donnée inattendue arrête proprement la lecture de cette
/// carte : mieux vaut un raccourci manquant qu'un plantage.
fn parse_map(reader: &mut Reader, depth: usize) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    if depth > 24 {
        return out;
    }
    while let Some(kind) = reader.u8() {
        match kind {
            0x08 => break,
            0x00 => {
                let Some(name) = reader.cstr() else { break };
                let children = parse_map(reader, depth + 1);
                out.push((name, Value::Map(children)));
            }
            0x01 => {
                let Some(key) = reader.cstr() else { break };
                let Some(value) = reader.cstr() else { break };
                out.push((key, Value::String(value)));
            }
            0x02 => {
                let Some(key) = reader.cstr() else { break };
                let Some(value) = reader.i32() else { break };
                out.push((key, Value::Int(value)));
            }
            0x03 | 0x04 | 0x06 => {
                // Flottant, pointeur, couleur : 4 octets, sans intérêt ici.
                let Some(_key) = reader.cstr() else { break };
                if reader.take(4).is_none() {
                    break;
                }
            }
            0x05 => {
                let Some(key) = reader.cstr() else { break };
                let Some(value) = reader.wstr() else { break };
                out.push((key, Value::String(value)));
            }
            0x07 => {
                let Some(key) = reader.cstr() else { break };
                let Some(value) = reader.u64() else { break };
                out.push((key, Value::UInt64(value)));
            }
            0x0A => {
                let Some(key) = reader.cstr() else { break };
                let Some(raw) = reader.u64() else { break };
                out.push((key, Value::Int(raw as i64 as i32)));
            }
            _ => break,
        }
    }
    out
}

/// Clés possibles pour un champ (`appid` / `AppId`, `AppName` / `appname`…).
fn field<'a>(fields: &'a [(String, Value)], names: &[&str]) -> Option<&'a Value> {
    fields
        .iter()
        .find(|(key, _)| names.iter().any(|name| key.eq_ignore_ascii_case(name)))
        .map(|(_, value)| value)
}

fn field_string(fields: &[(String, Value)], names: &[&str]) -> String {
    match field(fields, names) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Int(number)) => number.to_string(),
        Some(Value::UInt64(number)) => number.to_string(),
        Some(Value::Map(_)) | None => String::new(),
    }
}

fn field_u32(fields: &[(String, Value)], names: &[&str]) -> Option<u32> {
    match field(fields, names)? {
        Value::Int(number) => Some(*number as u32),
        Value::UInt64(number) => Some(*number as u32),
        Value::String(text) => text.trim().parse().ok(),
        Value::Map(_) => None,
    }
}

/// Décalages de départ plausibles : le fichier commence normalement par
/// `\0shortcuts\0`, parfois suivi d'un marqueur et d'un compte. Plutôt que de
/// parier sur une variante, on essaie les départs connus et on garde celui qui
/// donne le plus de raccourcis.
fn candidate_starts(bytes: &[u8]) -> Vec<usize> {
    let mut starts = vec![0usize];
    if bytes.first() == Some(&0) {
        if let Some(rest) = bytes.get(1..) {
            if let Some(end) = rest.iter().position(|b| *b == 0) {
                if &rest[..end] == b"shortcuts" {
                    let after = 1 + end + 1;
                    starts.push(after);
                    // Marqueur 0x0B puis compte sur 4 octets, selon la version.
                    if bytes.get(after) == Some(&0x0B) {
                        starts.push(after + 1);
                        starts.push(after + 5);
                    }
                }
            }
        }
    }
    starts.retain(|start| *start < bytes.len().max(1));
    starts
}

fn shortcuts_at(bytes: &[u8], start: usize) -> Vec<Shortcut> {
    let mut reader = Reader {
        data: bytes,
        pos: start,
    };
    let root = parse_map(&mut reader, 0);
    let children = match root.iter().find(|(key, _)| key == "shortcuts") {
        Some((_, Value::Map(children))) => children.clone(),
        _ => root,
    };
    let mut out = Vec::new();
    for (_, value) in &children {
        let Value::Map(fields) = value else { continue };
        let Some(app_id) = field_u32(fields, &["appid", "app_id"]) else {
            continue;
        };
        if app_id == 0 {
            continue;
        }
        let name = field_string(fields, &["AppName", "name"]);
        if name.trim().is_empty() {
            continue;
        }
        out.push(Shortcut {
            app_id,
            name,
            exe: field_string(fields, &["Exe", "exe"]),
            start_dir: field_string(fields, &["StartDir"]),
            launch_options: field_string(fields, &["LaunchOptions"]),
        });
    }
    out
}

/// Raccourcis contenus dans un `shortcuts.vdf` (octets bruts).
pub fn parse_shortcuts(bytes: &[u8]) -> Vec<Shortcut> {
    let mut best: Vec<Shortcut> = Vec::new();
    for start in candidate_starts(bytes) {
        let found = shortcuts_at(bytes, start);
        if found.len() > best.len() {
            best = found;
        }
    }
    best.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    best.dedup_by_key(|shortcut| shortcut.app_id);
    best
}

// ---------------------------------------------------------------------------
// Détection sur le disque
// ---------------------------------------------------------------------------

/// Racines Steam existantes, sans doublons (`.steam/steam` pointe souvent sur
/// `.local/share/Steam`).
fn roots() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for candidate in crate::proton::steam_root_candidates() {
        if !candidate.is_dir() {
            continue;
        }
        let key = candidate.canonicalize().unwrap_or_else(|_| candidate.clone());
        if out.iter().any(|seen| *seen == key) {
            continue;
        }
        out.push(key);
    }
    out
}

fn userdata_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root.join("userdata")) {
        for entry in entries.flatten() {
            let path = entry.path();
            // Les profils sont numériques ; `0` est le profil anonyme.
            if path.is_dir()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.chars().all(|c| c.is_ascii_digit()))
                    .unwrap_or(false)
            {
                out.push(path);
            }
        }
    }
    out
}

/// Tous les raccourcis non-Steam de tous les profils Steam.
pub fn detect() -> Vec<Shortcut> {
    let mut out: Vec<Shortcut> = Vec::new();
    for root in roots() {
        for profile in userdata_dirs(&root) {
            let file = profile.join("config/shortcuts.vdf");
            let Ok(bytes) = std::fs::read(&file) else {
                continue;
            };
            for shortcut in parse_shortcuts(&bytes) {
                if out.iter().any(|seen| seen.app_id == shortcut.app_id) {
                    continue;
                }
                out.push(shortcut);
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Bibliothèques Steam d'une racine : celle de la racine et celles listées
/// dans `libraryfolders.vdf` (disques secondaires).
fn library_steamapps(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.join("steamapps")];
    if let Ok(text) = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
        for line in text.lines() {
            let Some(path) = quoted_value(line, "path") else {
                continue;
            };
            out.push(PathBuf::from(path).join("steamapps"));
        }
    }
    out
}

/// Valeur d'une ligne `"clé" "valeur"` d'un VDF texte.
fn quoted_value(line: &str, key: &str) -> Option<String> {
    let mut parts = line.split('"');
    // Entre la clé et la valeur il n'y a que des blancs : c'est le morceau
    // suivant qui porte la valeur.
    parts.next()?;
    let name = parts.next()?;
    if !name.eq_ignore_ascii_case(key) {
        return None;
    }
    parts.next()?;
    let value = parts.next()?;
    Some(value.replace("\\\\", "\\"))
}

/// AppIDs des jeux Steam installés (`appmanifest_<appid>.acf`).
pub fn installed_app_ids() -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    for root in roots() {
        for steamapps in library_steamapps(&root) {
            let Ok(entries) = std::fs::read_dir(&steamapps) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let Some(rest) = name.strip_prefix("appmanifest_") else {
                    continue;
                };
                let Some(number) = rest.strip_suffix(".acf") else {
                    continue;
                };
                if let Ok(app_id) = number.parse::<u32>() {
                    if !out.contains(&app_id) {
                        out.push(app_id);
                    }
                }
            }
        }
    }
    out
}

/// Ce que Steam fera de `app_id` : raccourci, jeu du magasin, ou « no license ».
pub fn check(shortcuts: &[Shortcut], installed: &[u32], app_id: u32) -> OverlayCheck {
    if app_id == 0 {
        return OverlayCheck::Unknown;
    }
    if let Some(shortcut) = shortcuts.iter().find(|shortcut| shortcut.app_id == app_id) {
        return OverlayCheck::Shortcut {
            name: shortcut.name.clone(),
            exe: shortcut.exe.clone(),
        };
    }
    if installed.contains(&app_id) {
        return OverlayCheck::StoreInstalled;
    }
    if shortcuts.is_empty() {
        // Rien à comparer : on ne peut pas affirmer que c'est faux.
        return OverlayCheck::Unknown;
    }
    OverlayCheck::NoLicense
}

/// Vrai si la cible d'un raccourci désigne le même exécutable que le jeu.
/// Steam peut stocker des guillemets, des antislashs et un chemin Windows.
pub fn same_exe(shortcut_exe: &str, game_exe: &str) -> bool {
    let normalize = |raw: &str| -> String {
        let trimmed = raw.trim().trim_matches('"').trim();
        let unified = trimmed.replace('\\', "/");
        // Certains raccourcis portent `chemin %command%` : on ne garde que la
        // première cible.
        let first = unified
            .split("%command%")
            .next()
            .unwrap_or(&unified)
            .trim();
        // Deux antislashs doublés donnent `//` : un seul séparateur suffit
        // pour comparer deux chemins.
        let mut collapsed = String::with_capacity(first.len());
        let mut last_slash = false;
        for ch in first.chars() {
            if ch == '/' {
                if last_slash {
                    continue;
                }
                last_slash = true;
            } else {
                last_slash = false;
            }
            collapsed.push(ch);
        }
        collapsed
    };
    // Un lecteur Windows (`Z:`) ne change pas le fichier visé.
    let strip_drive = |path: String| -> String {
        let bytes = path.as_bytes();
        if bytes.len() > 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
            path[2..].to_string()
        } else {
            path
        }
    };
    let a = strip_drive(normalize(shortcut_exe));
    let b = strip_drive(normalize(game_exe));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a.eq_ignore_ascii_case(&b) {
        return true;
    }
    // Chemin relatif d'un côté, absolu de l'autre : on compare la fin, mais
    // seulement sur une frontière de dossier et avec au moins deux éléments —
    // deux jeux différents peuvent partager un `Jeu.exe`.
    let at_boundary = |long: &str, short: &str| -> bool {
        let long = long.to_lowercase();
        let short = short.to_lowercase();
        short.matches('/').count() >= 2
            && long.len() > short.len()
            && long.ends_with(&short)
            && long.as_bytes()[long.len() - short.len() - 1] == b'/'
    };
    at_boundary(&a, &b) || at_boundary(&b, &a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cstr(out: &mut Vec<u8>, text: &str) {
        out.extend_from_slice(text.as_bytes());
        out.push(0);
    }

    fn entry(out: &mut Vec<u8>, app_id: u32, name: &str, exe: &str) {
        out.push(0x00);
        cstr(out, "0");
        out.push(0x02);
        cstr(out, "appid");
        out.extend_from_slice(&(app_id as i32).to_le_bytes());
        out.push(0x01);
        cstr(out, "AppName");
        cstr(out, name);
        out.push(0x01);
        cstr(out, "Exe");
        cstr(out, exe);
        out.push(0x01);
        cstr(out, "StartDir");
        cstr(out, "/jeux");
        out.push(0x08);
    }

    fn sample(with_count: bool) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(0x00);
        cstr(&mut out, "shortcuts");
        if with_count {
            out.push(0x0B);
            out.extend_from_slice(&2u32.to_le_bytes());
        }
        entry(&mut out, 3_546_210_478, "The Witcher 3 (Quark)", "/jeux/witcher3/witcher3.exe");
        entry(&mut out, 2_654_321_098, "Celeste (Quark)", "/jeux/celeste/Celeste.exe");
        out.push(0x08);
        out
    }

    #[test]
    fn lit_les_raccourcis_des_deux_variantes_d_entete() {
        for with_count in [false, true] {
            let found = parse_shortcuts(&sample(with_count));
            assert_eq!(found.len(), 2, "variante compte={with_count}");
            assert_eq!(found[0].name, "Celeste (Quark)");
            assert_eq!(found[1].app_id, 3_546_210_478);
            assert_eq!(found[1].exe, "/jeux/witcher3/witcher3.exe");
        }
    }

    #[test]
    fn un_raccourci_avec_tags_reste_lisible() {
        // Les vrais fichiers ont un sous-dossier `tags` au milieu des champs :
        // la lecture doit continuer après lui.
        let mut out = Vec::new();
        out.push(0x00);
        cstr(&mut out, "shortcuts");
        out.push(0x0B);
        out.extend_from_slice(&1u32.to_le_bytes());
        out.push(0x00);
        cstr(&mut out, "0");
        out.push(0x02);
        cstr(&mut out, "appid");
        out.extend_from_slice(&(3_546_210_478u32 as i32).to_le_bytes());
        out.push(0x00);
        cstr(&mut out, "tags");
        out.push(0x01);
        cstr(&mut out, "0");
        cstr(&mut out, "rpg");
        out.push(0x08);
        out.push(0x01);
        cstr(&mut out, "AppName");
        cstr(&mut out, "The Witcher 3");
        out.push(0x01);
        cstr(&mut out, "Exe");
        cstr(&mut out, "/jeux/w3.exe");
        out.push(0x08);
        out.push(0x08);
        let found = parse_shortcuts(&out);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "The Witcher 3");
        assert_eq!(found[0].exe, "/jeux/w3.exe");
    }

    #[test]
    fn un_appid_en_texte_est_accepte() {
        let mut out = Vec::new();
        out.push(0x00);
        cstr(&mut out, "shortcuts");
        out.push(0x00);
        cstr(&mut out, "0");
        out.push(0x01);
        cstr(&mut out, "appid");
        cstr(&mut out, "3546210479");
        out.push(0x01);
        cstr(&mut out, "AppName");
        cstr(&mut out, "Jeu");
        out.push(0x08);
        out.push(0x08);
        let found = parse_shortcuts(&out);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].app_id, 3_546_210_479);
    }

    #[test]
    fn un_fichier_tronque_ne_plante_pas() {
        let full = sample(false);
        for cut in [0, 3, 12, 20, full.len() / 2, full.len() - 1] {
            let _ = parse_shortcuts(&full[..cut.min(full.len())]);
        }
        let _ = parse_shortcuts(b"");
        let _ = parse_shortcuts(&[0x00, 0x01, 0xFF, 0x00, 0x02]);
    }

    #[test]
    fn diagnostic_du_lancement() {
        let shortcuts = parse_shortcuts(&sample(false));
        // Raccourci réel : Steam lance cette cible.
        match check(&shortcuts, &[], 3_546_210_478) {
            OverlayCheck::Shortcut { name, .. } => assert_eq!(name, "The Witcher 3 (Quark)"),
            other => panic!("attendu un raccourci, obtenu {other:?}"),
        }
        // AppID du magasin, jeu installé : Steam lance le jeu du magasin.
        assert_eq!(check(&shortcuts, &[292_030], 292_030), OverlayCheck::StoreInstalled);
        // AppID inconnu : c'est le « no license » qui referme le jeu.
        assert_eq!(check(&shortcuts, &[], 292_030), OverlayCheck::NoLicense);
        assert!(check(&shortcuts, &[], 292_030).is_broken());
        // Aucun raccourci lisible : on ne bloque pas.
        assert_eq!(check(&[], &[], 292_030), OverlayCheck::Unknown);
        assert_eq!(check(&shortcuts, &[], 0), OverlayCheck::Unknown);
    }

    #[test]
    fn compare_les_cibles_de_raccourci() {
        assert!(same_exe("\"/jeux/a/Jeu.exe\"", "/jeux/a/Jeu.exe"));
        assert!(same_exe("Z:\\\\Jeux\\\\A\\\\Jeu.EXE", "/jeux/a/jeu.exe"));
        assert!(same_exe("/jeux/a/Jeu.exe %command%", "/jeux/a/Jeu.exe"));
        // Même nom de fichier, dossier différent : deux jeux distincts.
        assert!(!same_exe("/jeux/b/Jeu.exe", "/jeux/a/Jeu.exe"));
        // Chemin relatif : la fin suffit pour reconnaître le même fichier.
        assert!(same_exe("jeux/a/Jeu.exe", "/home/matheo/jeux/a/Jeu.exe"));
        assert!(!same_exe("", "/jeux/a/Jeu.exe"));
    }

    #[test]
    fn lit_une_ligne_de_libraryfolders() {
        let line = "\t\t\"path\"\t\t\"/mnt/jeux/SteamLibrary\"";
        assert_eq!(
            quoted_value(line, "path").as_deref(),
            Some("/mnt/jeux/SteamLibrary")
        );
        assert_eq!(quoted_value(line, "contentid"), None);
        // Chemin dans un VDF : les antislashs sont doublés.
        let windows = "\"path\"\t\t\"C:\\\\Jeux\\\\SteamLibrary\"";
        assert_eq!(
            quoted_value(windows, "path").as_deref(),
            Some("C:\\Jeux\\SteamLibrary")
        );
    }
}
