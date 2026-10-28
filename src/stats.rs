//! Statistiques de jeu, calculées à partir de l'historique des sessions.
//!
//! Quark garde chaque session terminée (`Game::sessions` : début + durée) en
//! plus du temps total : c'est ce qui permet les heures par semaine, le
//! calendrier de chaleur et la série de jours consécutifs, sans aucun service
//! externe.

use crate::config::{Library, PlaySession, now_ts};
use crate::session::civil_from_days;

/// Noms de mois abrégés, pour les libellés du calendrier.
pub const MONTHS: [&str; 12] = [
    "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
    "déc.",
];

/// Numéro du jour depuis l'époque Unix (UTC).
pub fn day_of(ts: i64) -> i64 {
    ts.div_euclid(86_400)
}

/// Jour de la semaine d'un numéro de jour (0 = lundi, 6 = dimanche).
pub fn weekday(day: i64) -> usize {
    // Le 1er janvier 1970 était un jeudi (3 en comptant lundi = 0).
    (day + 3).rem_euclid(7) as usize
}

pub fn weekday_label(day: usize) -> &'static str {
    ["L", "M", "M", "J", "V", "S", "D"][day.min(6)]
}

/// « 28 oct. » pour un numéro de jour.
pub fn date_label(day: i64) -> String {
    let (_, month, dom) = civil_from_days(day);
    format!("{dom} {}", MONTHS[(month as usize).clamp(1, 12) - 1])
}

/// « oct. 2026 » : libellé court pour l'axe du calendrier.
pub fn month_short(day: i64) -> String {
    let (year, month, _) = civil_from_days(day);
    format!("{} {year}", MONTHS[(month as usize).clamp(1, 12) - 1])
}

/// Durée par jour : la session est comptée le jour où elle a commencé.
pub fn per_day(sessions: &[PlaySession]) -> std::collections::HashMap<i64, u64> {
    let mut days = std::collections::HashMap::new();
    for session in sessions {
        *days.entry(day_of(session.start)).or_insert(0) += session.secs;
    }
    days
}

/// Lundi du jour donné.
pub fn week_start(day: i64) -> i64 {
    day - weekday(day) as i64
}

/// Durée par semaine (lundi de la semaine, durée), de la plus ancienne à la
/// plus récente, `count` semaines en tout.
pub fn weeks(sessions: &[PlaySession], count: usize) -> Vec<(i64, u64)> {
    let count = count.max(1);
    let current = week_start(day_of(now_ts()));
    let first = current - (count as i64 - 1) * 7;
    let mut buckets = vec![0u64; count];
    for session in sessions {
        let start = week_start(day_of(session.start));
        if start < first || start > current {
            continue;
        }
        let index = ((start - first) / 7) as usize;
        if let Some(bucket) = buckets.get_mut(index) {
            *bucket += session.secs;
        }
    }
    (0..count)
        .map(|index| (first + index as i64 * 7, buckets[index]))
        .collect()
}

/// Calendrier de chaleur : `weeks` colonnes de 7 jours, la dernière colonne
/// étant la semaine en cours.
pub struct Heatmap {
    /// `cells[week][weekday]`, en secondes jouées.
    pub cells: Vec<Vec<u64>>,
    /// Numéro de jour du lundi de la première colonne.
    pub first_day: i64,
    /// Durée maximale d'une case, pour l'échelle de couleur.
    pub max: u64,
}

pub fn heatmap(sessions: &[PlaySession], weeks: usize) -> Heatmap {
    let weeks = weeks.max(1);
    let current = week_start(day_of(now_ts()));
    let first_day = current - (weeks as i64 - 1) * 7;
    let mut cells = vec![vec![0u64; 7]; weeks];
    let mut max = 0u64;
    for session in sessions {
        let day = day_of(session.start);
        if day < first_day || day > current + 6 {
            continue;
        }
        let week = ((day - first_day) / 7) as usize;
        let slot = weekday(day);
        if let Some(cell) = cells.get_mut(week).and_then(|week| week.get_mut(slot)) {
            *cell += session.secs;
            max = max.max(*cell);
        }
    }
    Heatmap {
        cells,
        first_day,
        max,
    }
}

/// Jours consécutifs avec au moins une session, en remontant depuis
/// aujourd'hui. Un jour sans session terminé la série ; aujourd'hui peut être
/// vide sans la casser (on joue rarement avant midi).
pub fn streak(sessions: &[PlaySession]) -> u32 {
    let days = per_day(sessions);
    if days.is_empty() {
        return 0;
    }
    let today = day_of(now_ts());
    let mut cursor = if days.contains_key(&today) {
        today
    } else {
        today - 1
    };
    let mut count = 0;
    while days.contains_key(&cursor) {
        count += 1;
        cursor -= 1;
    }
    count
}

/// Jeux les plus joués de la bibliothèque (nom, secondes), décroissant.
pub fn top_games(lib: &Library, limit: usize) -> Vec<(String, u64)> {
    let mut games: Vec<(String, u64)> = lib
        .games
        .iter()
        .filter(|game| game.playtime_secs > 0)
        .map(|game| (game.name.clone(), game.playtime_secs))
        .collect();
    games.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    games.truncate(limit.max(1));
    games
}

/// « 2 h 15 » : durée lisible pour une case de calendrier.
pub fn human(secs: u64) -> String {
    if secs < 60 {
        format!("{} s", secs)
    } else if secs < 3600 {
        format!("{} min", secs / 60)
    } else {
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        if minutes == 0 {
            format!("{hours} h")
        } else {
            format!("{hours} h {minutes:02}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day_offset: i64, secs: u64) -> PlaySession {
        PlaySession {
            start: now_ts() + day_offset * 86_400,
            secs,
        }
    }

    #[test]
    fn le_lundi_commence_la_semaine() {
        // 1er janvier 1970 = jeudi.
        assert_eq!(weekday(0), 3);
        assert_eq!(weekday(4), 0);
        assert_eq!(week_start(4), 4);
        assert_eq!(week_start(6), 4);
    }

    #[test]
    fn les_heures_par_semaine_regroupent_les_sessions() {
        // 8 semaines = 56 jours : la session d'il y a 70 jours est hors fenêtre.
        let sessions = vec![at(0, 3600), at(-1, 1800), at(-70, 7200)];
        let buckets = weeks(&sessions, 8);
        assert_eq!(buckets.len(), 8);
        let total: u64 = buckets.iter().map(|(_, secs)| *secs).sum();
        assert_eq!(total, 5400);
    }

    #[test]
    fn la_serie_compte_les_jours_consecutifs() {
        assert_eq!(streak(&[]), 0);
        // Deux jours de suite jusqu'à aujourd'hui, puis un trou.
        assert_eq!(streak(&[at(0, 60), at(-1, 60), at(-3, 60)]), 2);
        // Rien aujourd'hui : hier compte encore, la série n'est pas cassée.
        assert_eq!(streak(&[at(-1, 60), at(-2, 60)]), 2);
        // Trou d'un jour : la série s'arrête.
        assert_eq!(streak(&[at(0, 60), at(-2, 60)]), 1);
    }

    #[test]
    fn la_chaleur_place_les_jours_dans_la_bonne_case() {
        let map = heatmap(&[at(0, 120)], 4);
        assert_eq!(map.cells.len(), 4);
        let today = day_of(now_ts());
        assert!(map.first_day <= today);
        assert_eq!(today - map.first_day, 3 * 7 + weekday(today) as i64);
        assert_eq!(map.max, 120);
        assert_eq!(map.cells[3][weekday(today)], 120);
    }

    #[test]
    fn les_libelles_de_date_sont_lisibles() {
        // 28 octobre 2026 = jour 20754 (1 793 196 300 / 86 400).
        assert_eq!(date_label(20_754), "28 oct.");
        assert_eq!(month_short(20_754), "oct. 2026");
        assert_eq!(human(7_500), "2 h 05");
    }
}
