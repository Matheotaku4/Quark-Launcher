//! Note ProtonDB d'un jeu (`/api/v1/reports/summaries/<appid>.json`).
//!
//! L'API est publique et renvoie une note de compatibilité (platinum, gold,
//! silver, bronze, borked) calculée à partir des rapports des joueurs. Quark
//! l'affiche sur la fiche du jeu, avec un lien vers la page ProtonDB.

use serde::Deserialize;

/// Note de compatibilité ProtonDB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Platinum,
    Gold,
    Silver,
    Bronze,
    Borked,
    Pending,
}

impl Tier {
    pub fn parse(raw: &str) -> Tier {
        match raw.trim().to_ascii_lowercase().as_str() {
            "platinum" => Tier::Platinum,
            "gold" => Tier::Gold,
            "silver" => Tier::Silver,
            "bronze" => Tier::Bronze,
            "borked" => Tier::Borked,
            _ => Tier::Pending,
        }
    }

    /// Nom affiché, en français comme le reste de l'interface.
    pub fn label(self) -> &'static str {
        match self {
            Tier::Platinum => "Platine",
            Tier::Gold => "Or",
            Tier::Silver => "Argent",
            Tier::Bronze => "Bronze",
            Tier::Borked => "Injouable",
            Tier::Pending => "En attente",
        }
    }

    /// Couleur de la pastille (les teintes officielles de ProtonDB).
    pub fn color(self) -> (u8, u8, u8) {
        match self {
            Tier::Platinum => (0xB4, 0xC7, 0xDC),
            Tier::Gold => (0xCF, 0xB5, 0x3B),
            Tier::Silver => (0xA6, 0xA6, 0xA6),
            Tier::Bronze => (0xCD, 0x7F, 0x32),
            Tier::Borked => (0xE0, 0x4B, 0x4B),
            Tier::Pending => (0x7A, 0x80, 0x86),
        }
    }

    /// Page ProtonDB du jeu.
    pub fn url(app_id: u32) -> String {
        format!("https://www.protondb.com/app/{app_id}")
    }
}

/// Résumé renvoyé par ProtonDB.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub tier: Tier,
    /// « strong », « moderate », « low »… (vide si absent).
    pub confidence: String,
    /// Nombre de rapports pris en compte.
    pub total: u32,
    /// Meilleure note jamais rapportée, plus flatteuse que la moyenne.
    pub best: Option<Tier>,
}

impl Report {
    /// « Or · 379 rapports · confiance forte ».
    pub fn label(&self) -> String {
        let mut parts = vec![self.tier.label().to_string()];
        if self.total > 0 {
            parts.push(format!("{} rapports", self.total));
        }
        if !self.confidence.is_empty() {
            parts.push(format!("confiance {}", self.confidence));
        }
        parts.join(" · ")
    }
}

#[derive(Debug, Deserialize)]
struct Raw {
    #[serde(default)]
    tier: String,
    #[serde(default)]
    confidence: String,
    #[serde(default)]
    total: u32,
    #[serde(default, rename = "bestReportedTier")]
    best_reported_tier: String,
}

/// Interroge ProtonDB. Un appid sans rapport renvoie une erreur lisible plutôt
/// qu'une note inventée.
pub fn fetch(client: &reqwest::blocking::Client, app_id: u32) -> Result<Report, String> {
    let url = format!("https://www.protondb.com/api/v1/reports/summaries/{app_id}.json");
    let response = client
        .get(&url)
        .send()
        .map_err(|err| format!("ProtonDB injoignable ({err})"))?;
    let status = response.status();
    if status.as_u16() == 404 {
        return Err("aucun rapport ProtonDB pour ce jeu".to_string());
    }
    if !status.is_success() {
        return Err(format!("ProtonDB a répondu {}", status.as_u16()));
    }
    let raw: Raw = response
        .json()
        .map_err(|err| format!("réponse ProtonDB illisible ({err})"))?;
    if raw.tier.trim().is_empty() && raw.total == 0 {
        return Err("aucun rapport ProtonDB pour ce jeu".to_string());
    }
    Ok(Report {
        tier: Tier::parse(&raw.tier),
        confidence: raw.confidence,
        total: raw.total,
        best: Some(raw.best_reported_tier)
            .filter(|tier| !tier.trim().is_empty())
            .map(|tier| Tier::parse(&tier)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_notes_connues_sont_reconnues() {
        assert_eq!(Tier::parse("platinum"), Tier::Platinum);
        assert_eq!(Tier::parse(" GOLD "), Tier::Gold);
        assert_eq!(Tier::parse("borked"), Tier::Borked);
        // Valeur inconnue ou vide : en attente, jamais une note inventée.
        assert_eq!(Tier::parse(""), Tier::Pending);
        assert_eq!(Tier::parse("nvidia-proprietary"), Tier::Pending);
    }

    #[test]
    fn le_resume_reprend_ceux_de_protondb() {
        let raw: Raw = serde_json::from_str(
            r#"{"bestReportedTier":"platinum","confidence":"strong","score":0.92,
                "tier":"platinum","total":379,"trendingTier":"platinum"}"#,
        )
        .unwrap();
        let report = Report {
            tier: Tier::parse(&raw.tier),
            confidence: raw.confidence.clone(),
            total: raw.total,
            best: Some(Tier::parse(&raw.best_reported_tier)),
        };
        assert_eq!(report.tier, Tier::Platinum);
        assert_eq!(report.best, Some(Tier::Platinum));
        assert_eq!(report.label(), "Platine · 379 rapports · confiance strong");
    }

    #[test]
    fn le_lien_pointe_sur_la_page_du_jeu() {
        assert_eq!(Tier::url(367_520), "https://www.protondb.com/app/367520");
    }
}
