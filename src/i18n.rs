//! Traduction de l'interface, sans dépendance externe.
//!
//! Les textes français restent la source dans le code : une chaîne inconnue
//! s'affiche telle quelle, ce qui évite qu'un nouveau libellé disparaisse avant
//! sa traduction. Le catalogue anglais est embarqué dans le binaire, et la
//! langue choisie est appliquée immédiatement à toute l'interface.

use crate::config::UiLanguage;
use eframe::egui::RichText;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU8, Ordering};

const EN: &str = include_str!("../assets/i18n/en.json");

static LANGUAGE: AtomicU8 = AtomicU8::new(0);

static CATALOG: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    let pairs: HashMap<String, String> = serde_json::from_str(EN).unwrap_or_default();
    pairs
        .into_iter()
        .map(|(key, value)| (normalize_key(&key), value))
        .collect()
});

pub fn set_language(language: UiLanguage) {
    LANGUAGE.store(
        match language {
            UiLanguage::French => 0,
            UiLanguage::English => 1,
        },
        Ordering::Relaxed,
    );
}

pub fn language() -> UiLanguage {
    match LANGUAGE.load(Ordering::Relaxed) {
        1 => UiLanguage::English,
        _ => UiLanguage::French,
    }
}

pub fn is_english() -> bool {
    language() == UiLanguage::English
}

/// Normalise les espaces (les chaînes Rust multilignes sont repliées par le
/// compilateur) pour que la même phrase trouve toujours sa traduction.
fn normalize_key(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Traduit un texte déjà construit. En français, la chaîne est renvoyée
/// inchangée ; en anglais, le catalogue est consulté puis quelques formes
/// dynamiques courantes sont adaptées.
pub fn tr(text: &str) -> String {
    if !is_english() || text.is_empty() {
        return text.to_string();
    }
    let key = normalize_key(text);
    if let Some(value) = CATALOG.get(&key) {
        return value.clone();
    }
    dynamic_english(&key)
}

/// Raccourci pour les textes construits dans l'interface.
pub fn rich(text: impl Into<String>) -> RichText {
    RichText::new(tr(&text.into()))
}

/// Traduit un libellé sans perdre les marqueurs de style quand l'appelant doit
/// continuer à construire un `RichText` (taille, couleur, etc.).
pub fn label(text: &str) -> String {
    tr(text)
}

/// Quelques formes produites par `format!` ne peuvent pas figurer telles
/// quelles dans le catalogue. Ces règles restent volontairement conservatrices :
/// elles ne touchent qu'aux mots français isolés, jamais aux noms de jeux.
fn dynamic_english(text: &str) -> String {
    let mut out = text.to_string();
    let rules: [(&str, &str); 64] = [
        ("Aucun jeu sélectionné", "No game selected"),
        ("Aucun jeu ne correspond", "No game matches"),
        ("Aucun résultat", "No results"),
        ("Aucune vue enregistrée", "No saved views"),
        ("Aucune session enregistrée", "No session recorded"),
        ("Aucune sauvegarde", "No backup"),
        ("Aucun tag", "No tags"),
        ("Aucun téléchargement en cours", "No download in progress"),
        ("Aucun runtime trouvé", "No runtime found"),
        ("Aucun artwork", "No artwork"),
        ("Aucun exécutable", "No executable"),
        ("Aucune correspondance", "No match"),
        ("Aucune proposition", "No suggestion"),
        ("Aucune image", "No image"),
        ("Aucune variable", "No variable"),
        ("Aucune archive", "No archive"),
        ("Aucune mise à jour", "No update"),
        ("Aucune erreur", "No error"),
        ("Aucune installation", "No installation"),
        ("Aucune session", "No session"),
        ("Aucune activité", "No activity"),
        ("Aucune nouveauté", "No news"),
        ("Aucune actualité", "No news"),
        ("Dernière session", "Last session"),
        ("Dernière mise à jour", "Last update"),
        ("Temps de jeu", "Playtime"),
        ("temps de jeu", "playtime"),
        ("de jeu", "played"),
        ("en jeu", "in game"),
        ("En jeu", "In game"),
        (" heures", " hours"),
        (" heure", " hour"),
        (" minutes", " minutes"),
        (" seconde", " second"),
        (" secondes", " seconds"),
        ("Tout", "All"),
        ("Tri : ", "Sort: "),
        ("Grouper : ", "Group: "),
        ("correspondance(s)", "match(es)"),
        ("résultat(s)", "result(s)"),
        ("joueurs simultanés", "concurrent players"),
        ("jeu(x)", "game(s)"),
        ("jeux retirés", "games removed"),
        ("succès débloqués", "achievements unlocked"),
        ("sélectionné", "selected"),
        ("Mémoire résidente", "Resident memory"),
        ("Taille à télécharger", "Download size"),
        ("Taille non communiquée", "Size not provided"),
        ("Taille : calcul en cours", "Size: calculating"),
        ("calcul en cours", "calculating"),
        ("Dossier analysé : ", "Scanned folder: "),
        ("Image disque : ", "Disc image: "),
        ("Émulateur : ", "Emulator: "),
        ("Détecté : ", "Detected: "),
        (
            "Lecture du dossier impossible : ",
            "Could not read folder: ",
        ),
        ("Temps ", "Time "),
        ("Vue «", "View “"),
        (" » appliquée", "” applied"),
        (" » enregistrée", "” saved"),
        (" » supprimée", "” deleted"),
        ("Aucune ", "No "),
        ("Aucun ", "No "),
        ("Aucune", "No"),
        ("Aucun", "No"),
    ];
    for (from, to) in rules {
        if out.contains(from) {
            out = out.replace(from, to);
        }
    }
    // Formes dynamiques très fréquentes : « 7 jeux », « 7 jeu(x) »,
    // « 1 jeu ». On ne remplace jamais « jeu » à l'intérieur d'un titre.
    let words: Vec<&str> = out.split(' ').collect();
    if let Some(first) = words.first().and_then(|word| word.parse::<u64>().ok()) {
        if out.ends_with(" jeux") || out.ends_with(" jeu(x)") {
            out.truncate(out.len() - if out.ends_with(" jeux") { 5 } else { 7 });
            out.push_str(if first == 1 { " game" } else { " games" });
        } else if out.ends_with(" jeu") {
            out.truncate(out.len() - 4);
            out.push_str(if first == 1 { " game" } else { " games" });
        }
    }
    if out.starts_with("1 ")
        && (out.contains(" hours") || out.contains(" minutes") || out.contains(" seconds"))
    {
        out = out.replacen(" hours", " hour", 1);
        out = out.replacen(" minutes", " minute", 1);
        out = out.replacen(" seconds", " second", 1);
    }
    // Formes « N game(s) », « N match(es) », « N result(s) » : le catalogue
    // ne peut pas connaître le nombre, on choisit donc le singulier/pluriel.
    if let Some(count) = out
        .split(' ')
        .next()
        .and_then(|word| word.parse::<u64>().ok())
    {
        for (suffix, singular, plural) in [
            (" game(s)", " game", " games"),
            (" match(es)", " match", " matches"),
            (" result(s)", " result", " results"),
            (" texture(s)", " texture", " textures"),
        ] {
            if let Some(prefix) = out.strip_suffix(suffix) {
                out = format!("{prefix}{}", if count == 1 { singular } else { plural });
                break;
            }
        }
    }
    out
}

/// Résumé concret de la version courante, affiché une fois après une mise à
/// jour. Il est volontairement court : quelques phrases, pas un journal de commit.
pub fn release_summary() -> [&'static str; 6] {
    if is_english() {
        [
            "PlayStation 2 games are launched by PCSX2; Quark reads the serial from the ISO and fetches the cover from ps2-covers.",
            "RetroAchievements progress is shown on the PS2 game page and in the overlay, while PCSX2 keeps handling unlocks.",
            "A DLL overrides tab lists the DLLs in the game folder and writes native or builtin choices into WINEDLLOVERRIDES.",
            "Existing WINEDLLOVERRIDES entries are loaded and preserved, including entries written without a mode.",
            "Placeholder suggestions in Tags and Arguments are now grey instead of looking like entered values.",
            "The wiki is now entirely in English; existing configurations load without a reset.",
        ]
    } else {
        [
            "Les jeux PlayStation 2 sont lancés par PCSX2 ; Quark lit le numéro de série de l'ISO et récupère la jaquette depuis ps2-covers.",
            "La progression RetroAchievements s'affiche dans la fiche PS2 et dans l'overlay, PCSX2 continuant de gérer les déblocages.",
            "Un onglet DLL overrides liste les DLL du dossier du jeu et écrit les choix native ou builtin dans WINEDLLOVERRIDES.",
            "Les entrées WINEDLLOVERRIDES existantes sont reprises et conservées, y compris celles écrites sans mode.",
            "Les suggestions des champs Tags et Arguments sont désormais grises au lieu de ressembler à une valeur saisie.",
            "Le wiki passe entièrement en anglais ; les configurations existantes se chargent sans réinitialisation.",
        ]
    }
}

pub fn release_title() -> String {
    if is_english() {
        format!("What's new in {}", env!("CARGO_PKG_VERSION"))
    } else {
        format!("Nouveautés de la {}", env!("CARGO_PKG_VERSION"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    // La langue est globale au processus : les tests qui la changent doivent
    // s'exécuter l'un après l'autre, sinon l'un lit la langue posée par l'autre.
    static LANGUAGE_LOCK: Mutex<()> = Mutex::new(());

    fn language_lock() -> MutexGuard<'static, ()> {
        LANGUAGE_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    #[test]
    fn le_francais_reste_la_source() {
        let _guard = language_lock();
        set_language(UiLanguage::French);
        assert_eq!(tr("Ajouter un jeu"), "Ajouter un jeu");
    }

    #[test]
    fn le_catalogue_traduit_les_libelles_connus() {
        let _guard = language_lock();
        set_language(UiLanguage::English);
        assert_eq!(tr("Ajouter un jeu"), "Add a game");
        assert_eq!(tr("Paramètres"), "Settings");
        set_language(UiLanguage::French);
    }

    #[test]
    fn les_espaces_multilignes_sont_normalises() {
        let _guard = language_lock();
        assert_eq!(normalize_key("  un   texte\n   replié "), "un texte replié");
    }

    #[test]
    fn le_resume_de_version_suit_la_langue() {
        let _guard = language_lock();
        set_language(UiLanguage::French);
        assert!(release_summary()[0].starts_with("Les jeux PlayStation 2"));
        assert!(release_summary()[1].starts_with("La progression RetroAchievements"));
        set_language(UiLanguage::English);
        assert!(release_summary()[0].starts_with("PlayStation 2 games"));
        assert!(release_summary()[1].starts_with("RetroAchievements progress"));
        assert!(release_title().starts_with("What's new"));
        set_language(UiLanguage::French);
    }

    #[test]
    fn les_durees_suivent_la_langue() {
        let _guard = language_lock();
        set_language(UiLanguage::English);
        assert_eq!(crate::config::human_playtime(483_120), "134.2 h");
        set_language(UiLanguage::French);
        assert_eq!(crate::config::human_playtime(483_120), "134,2 h");
    }

    #[test]
    fn une_chaine_inconnue_ne_disparait_pas() {
        let _guard = language_lock();
        set_language(UiLanguage::English);
        assert_eq!(tr("Hollow Knight"), "Hollow Knight");
        assert_eq!(tr("7 jeux"), "7 games");
        assert_eq!(tr("1 jeu"), "1 game");
        set_language(UiLanguage::French);
    }
}
