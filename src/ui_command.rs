//! Palette de commandes (Ctrl+K) : atteindre une page, une action ou un jeu
//! sans quitter le clavier. Le filtrage est une sous-séquence tolérante aux
//! accents et aux lettres intermédiaires, dans l'esprit des palettes des
//! launchers modernes.

use crate::app::{QuarkApp, SettingsTab, Tab};
use crate::config::{ViewMode, fold_char};
use crate::widgets::{self, Icon};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

#[derive(Clone, PartialEq, Eq)]
pub enum PaletteAction {
    Home,
    Library,
    Grid,
    List,
    Store,
    MyGames,
    AddGame,
    Downloads,
    Settings,
    Stats,
    Disk,
    Updates,
    ToggleTheme,
    OpenGame(String),
}

#[derive(Clone)]
struct Entry {
    group: &'static str,
    label: String,
    detail: String,
    keywords: String,
    icon: Icon,
    action: PaletteAction,
}

impl Entry {
    fn new(
        group: &'static str,
        label: impl Into<String>,
        detail: impl Into<String>,
        keywords: impl Into<String>,
        icon: Icon,
        action: PaletteAction,
    ) -> Self {
        Self {
            group,
            label: label.into(),
            detail: detail.into(),
            keywords: keywords.into(),
            icon,
            action,
        }
    }

    /// Les champs sont comparés séparément : concaténer le titre et les mots
    /// clés permettrait à « witcher » de se disperser dans « Hollow Knight ·
    /// Team Cherry » et de proposer un jeu sans rapport.
    fn score(&self, query: &str) -> Option<i32> {
        [&self.label, &self.detail, &self.keywords]
            .into_iter()
            .filter_map(|field| fuzzy_score(query, field))
            .max()
    }
}

fn normalized(text: &str) -> Vec<char> {
    text.chars()
        .flat_map(|ch| ch.to_lowercase())
        .map(fold_char)
        .collect()
}

fn boundary(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '-' | '_' | '/' | ':' | '·' | '(' | '[')
}

/// Score d'une sous-séquence, ou `None` si la requête n'est pas contenue dans
/// l'ordre. Les débuts de mot et les lettres consécutives valent plus cher,
/// pour que « witcher » classe « The Witcher 3 » avant un titre où les lettres
/// sont seulement éparpillées.
pub(crate) fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    let needle = normalized(query.trim());
    if needle.is_empty() {
        return Some(0);
    }
    let haystack = normalized(text);
    if haystack.is_empty() {
        return None;
    }

    let mut score = 0;
    let mut cursor = 0usize;
    let mut previous = None;
    for wanted in &needle {
        let found = haystack[cursor..]
            .iter()
            .position(|ch| ch == wanted)
            .map(|offset| cursor + offset)?;
        if found == 0 {
            score += 18;
        } else if boundary(haystack[found - 1]) {
            score += 10;
        }
        if previous.is_some_and(|last| last + 1 == found) {
            score += 7;
        }
        score += 2;
        previous = Some(found);
        cursor = found + 1;
    }
    if haystack.starts_with(&needle) {
        score += 14;
    }
    // Un titre court qui contient la requête est plus pertinent qu'un long
    // sous-titre où elle se perd.
    score -= (haystack.len() as i32 / 14).min(8);
    Some(score)
}

impl QuarkApp {
    pub(crate) fn open_command_palette(&mut self, ctx: &egui::Context) {
        self.command_palette_open = true;
        self.command_query.clear();
        self.command_last_query.clear();
        self.command_selected = 0;
        ctx.memory_mut(|memory| {
            memory.request_focus(egui::Id::new("quark_command_query"));
        });
    }

    pub(crate) fn close_command_palette(&mut self, ctx: &egui::Context) {
        self.command_palette_open = false;
        ctx.memory_mut(|memory| {
            memory.surrender_focus(egui::Id::new("quark_command_query"));
        });
    }

    fn command_entries(&self) -> Vec<Entry> {
        let mut entries = vec![
            Entry::new(
                "PAGES",
                "Accueil",
                "Reprendre un jeu récent",
                "home accueil recent reprendre",
                Icon::Home,
                PaletteAction::Home,
            ),
            Entry::new(
                "PAGES",
                "Bibliothèque",
                "Tous les jeux de Quark",
                "library bibliotheque jeux collection",
                Icon::Grid,
                PaletteAction::Library,
            ),
            Entry::new(
                "PAGES",
                "Boutique",
                "Catalogue Steam",
                "store boutique steam magasin catalogue",
                Icon::Steam,
                PaletteAction::Store,
            ),
            Entry::new(
                "PAGES",
                "Mes jeux",
                "Boutiques et installations",
                "my games mes jeux epic gog steam installations",
                Icon::Download,
                PaletteAction::MyGames,
            ),
            Entry::new(
                "AFFICHAGE",
                "Vue grille",
                "Couvertures en grand",
                "grid grille couvertures vue",
                Icon::Grid,
                PaletteAction::Grid,
            ),
            Entry::new(
                "AFFICHAGE",
                "Vue liste",
                "Liste détaillée des jeux",
                "list liste vue details",
                Icon::List,
                PaletteAction::List,
            ),
            Entry::new(
                "ACTIONS",
                "Ajouter un jeu",
                "Nom, artworks et exécutable",
                "add ajouter nouveau jeu executable",
                Icon::Plus,
                PaletteAction::AddGame,
            ),
            Entry::new(
                "ACTIONS",
                "Téléchargements",
                "Installations et transferts en cours",
                "download telechargements installations transferts",
                Icon::Download,
                PaletteAction::Downloads,
            ),
            Entry::new(
                "ACTIONS",
                "Paramètres",
                "Apparence et comportement de Quark",
                "settings parametres reglages configuration",
                Icon::Gear,
                PaletteAction::Settings,
            ),
            Entry::new(
                "ACTIONS",
                "Statistiques",
                "Heures, calendrier et séries",
                "stats statistiques heures calendrier series",
                Icon::Log,
                PaletteAction::Stats,
            ),
            Entry::new(
                "ACTIONS",
                "Espace disque",
                "Poids des préfixes et des jeux",
                "disk disque espace stockage prefixes poids",
                Icon::Folder,
                PaletteAction::Disk,
            ),
            Entry::new(
                "ACTIONS",
                "Vérifier les mises à jour",
                "Dernière release GitHub",
                "update mise a jour release github verifier",
                Icon::Refresh,
                PaletteAction::Updates,
            ),
            Entry::new(
                "ACTIONS",
                if self.lib.settings.theme == crate::config::ThemeMode::Amoled {
                    "Passer en thème Graphite"
                } else {
                    "Passer en thème AMOLED"
                },
                "Changer le fond de l'interface",
                "theme apparence amoled graphite clair sombre",
                Icon::Info,
                PaletteAction::ToggleTheme,
            ),
        ];

        let query = self.command_query.trim();
        // Sans requête, seuls les jeux récents sont proposés : la palette
        // reste courte et utile. Une recherche parcourt toute la collection.
        let game_ids = if query.is_empty() {
            self.lib.recent_ids(6)
        } else {
            self.lib.sorted_ids()
        };
        for id in game_ids {
            let Some(game) = self.lib.get(&id) else {
                continue;
            };
            let detail = format!("{} · {}", game.status.label(), game.playtime_label());
            entries.push(Entry::new(
                "JEUX",
                game.name.clone(),
                detail,
                format!(
                    "{} {} {} jeu game",
                    game.developer,
                    game.genres.join(" "),
                    game.tags.join(" ")
                ),
                Icon::Play,
                PaletteAction::OpenGame(id),
            ));
        }
        entries
    }

    fn filtered_entries(&self) -> Vec<Entry> {
        let query = self.command_query.trim();
        let all = self.command_entries();
        if query.is_empty() {
            return all;
        }
        let groups = ["PAGES", "AFFICHAGE", "ACTIONS", "JEUX"];
        let mut filtered = Vec::new();
        for group in groups {
            let mut matches: Vec<(i32, Entry)> = all
                .iter()
                .filter(|entry| entry.group == group)
                .filter_map(|entry| entry.score(query).map(|score| (score, entry.clone())))
                .collect();
            if group == "JEUX" {
                matches.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.label.cmp(&b.1.label)));
                matches.truncate(12);
            } else {
                matches.sort_by(|a, b| b.0.cmp(&a.0));
            }
            filtered.extend(matches.into_iter().map(|(_, entry)| entry));
        }
        filtered
    }

    fn run_palette_action(&mut self, ctx: &egui::Context, action: PaletteAction) {
        match action {
            PaletteAction::Home => {
                self.tab = Tab::Library;
                self.set_view(ViewMode::List);
                self.home = true;
            }
            PaletteAction::Library => {
                self.tab = Tab::Library;
                self.home = false;
            }
            PaletteAction::Grid => {
                self.tab = Tab::Library;
                self.home = false;
                self.set_view(ViewMode::Grid);
            }
            PaletteAction::List => {
                self.tab = Tab::Library;
                self.home = false;
                self.set_view(ViewMode::List);
            }
            PaletteAction::Store => {
                self.tab = Tab::Store;
                self.home = false;
            }
            PaletteAction::MyGames => {
                self.tab = Tab::MyGames;
                self.home = false;
            }
            PaletteAction::AddGame => self.open_add_dialog(),
            PaletteAction::Downloads => self.downloads_open = true,
            PaletteAction::Settings => {
                self.settings_tab = SettingsTab::Appearance;
                self.settings_open = true;
            }
            PaletteAction::Stats => {
                self.settings_tab = SettingsTab::Stats;
                self.settings_open = true;
            }
            PaletteAction::Disk => {
                self.settings_tab = SettingsTab::Disk;
                self.settings_open = true;
            }
            PaletteAction::Updates => {
                self.settings_tab = SettingsTab::About;
                self.settings_open = true;
                self.check_updates(ctx, false);
            }
            PaletteAction::ToggleTheme => {
                self.lib.settings.theme =
                    if self.lib.settings.theme == crate::config::ThemeMode::Amoled {
                        crate::config::ThemeMode::Graphite
                    } else {
                        crate::config::ThemeMode::Amoled
                    };
                self.touch();
            }
            PaletteAction::OpenGame(id) => {
                self.tab = Tab::Library;
                self.home = false;
                self.select(&id);
                if self.lib.settings.view == ViewMode::Grid {
                    self.overlay = Some(id);
                }
            }
        }
    }

    /// Dessine la palette au-dessus de l'interface. Elle consomme ses touches
    /// avant le champ de recherche pour qu'Entrée et les flèches pilotent la
    /// sélection, pas le curseur de texte.
    pub(crate) fn command_palette(&mut self, ctx: &egui::Context) {
        if !self.command_palette_open {
            return;
        }
        let escape =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let down =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
        let up =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
        let enter =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        if escape {
            self.close_command_palette(ctx);
            return;
        }
        if self.command_query != self.command_last_query {
            self.command_last_query = self.command_query.clone();
            self.command_selected = 0;
        }
        let entries = self.filtered_entries();
        if entries.is_empty() {
            self.command_selected = 0;
        } else if self.command_selected >= entries.len() {
            self.command_selected = entries.len() - 1;
        }
        if down && !entries.is_empty() {
            self.command_selected = (self.command_selected + 1) % entries.len();
        }
        if up && !entries.is_empty() {
            self.command_selected = (self.command_selected + entries.len() - 1) % entries.len();
        }
        let mut chosen = enter
            .then(|| {
                entries
                    .get(self.command_selected)
                    .map(|entry| entry.action.clone())
            })
            .flatten();

        let pal = self.pal;
        let screen = ctx.screen_rect();
        let width = (screen.width() - 48.0).clamp(360.0, 680.0);
        let mut close = false;
        egui::Area::new(egui::Id::new("quark_command_backdrop"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, response) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_black_alpha(176));
                if response.clicked() {
                    close = true;
                }
            });

        egui::Area::new(egui::Id::new("quark_command_palette"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_CENTER, Vec2::new(0.0, -12.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(pal.panel)
                    .stroke(Stroke::new(1.0_f32, pal.border))
                    .corner_radius(12.0)
                    .inner_margin(egui::Margin::symmetric(0, 8))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            let (icon_rect, _) =
                                ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
                            widgets::paint_icon(ui, icon_rect, Icon::Search, pal.text_dim);
                            ui.add_space(6.0);
                            let response = ui.add_sized(
                                Vec2::new((width - 92.0).max(180.0), 30.0),
                                egui::TextEdit::singleline(&mut self.command_query)
                                    .id(egui::Id::new("quark_command_query"))
                                    .hint_text(crate::i18n::tr(
                                        "Rechercher un jeu ou une commande…",
                                    ))
                                    .frame(false)
                                    .font(FontId::proportional(14.0)),
                            );
                            if response.changed() {
                                self.command_selected = 0;
                            }
                            ui.label(
                                crate::i18n::rich("ESC")
                                    .size(10.0)
                                    .monospace()
                                    .color(pal.text_faint),
                            );
                            ui.add_space(12.0);
                        });
                        ui.add_space(4.0);
                        let (line, _) =
                            ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover());
                        ui.painter().rect_filled(line, 0.0, pal.border);
                        ui.add_space(4.0);

                        egui::ScrollArea::vertical()
                            .id_salt("quark_command_scroll")
                            .max_height(390.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                if entries.is_empty() {
                                    ui.add_space(22.0);
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            crate::i18n::rich("Aucun résultat")
                                                .size(12.5)
                                                .color(pal.text_dim),
                                        );
                                    });
                                    ui.add_space(22.0);
                                    return;
                                }
                                let mut last_group = "";
                                for (index, entry) in entries.iter().enumerate() {
                                    if entry.group != last_group {
                                        last_group = entry.group;
                                        ui.add_space(if index == 0 { 2.0 } else { 8.0 });
                                        ui.horizontal(|ui| {
                                            ui.add_space(18.0);
                                            ui.label(
                                                crate::i18n::rich(entry.group)
                                                    .size(9.5)
                                                    .strong()
                                                    .color(pal.text_faint),
                                            );
                                        });
                                        ui.add_space(2.0);
                                    }
                                    let selected = index == self.command_selected;
                                    let (rect, response) = ui.allocate_exact_size(
                                        Vec2::new(width, 40.0),
                                        Sense::click(),
                                    );
                                    // Le survol ne prend la sélection que si la
                                    // souris bouge : sinon elle resterait bloquée
                                    // sur la ligne sous le pointeur et les flèches
                                    // ne pourraient plus descendre.
                                    let pointer_moved = ui
                                        .ctx()
                                        .input(|input| input.pointer.delta().length_sq() > 0.0);
                                    if response.hovered() && pointer_moved {
                                        self.command_selected = index;
                                    }
                                    let highlighted = selected || response.hovered();
                                    let painter = ui.painter_at(rect);
                                    if highlighted {
                                        painter.rect_filled(
                                            rect.shrink2(Vec2::new(8.0, 1.0)),
                                            6.0,
                                            if selected {
                                                pal.surface_hover
                                            } else {
                                                pal.surface
                                            },
                                        );
                                    }
                                    if selected {
                                        painter.rect_filled(
                                            Rect::from_min_size(
                                                Pos2::new(rect.left() + 8.0, rect.top() + 8.0),
                                                Vec2::new(3.0, 24.0),
                                            ),
                                            1.5,
                                            pal.accent,
                                        );
                                    }
                                    let icon_rect = Rect::from_center_size(
                                        Pos2::new(rect.left() + 27.0, rect.center().y),
                                        Vec2::splat(17.0),
                                    );
                                    widgets::paint_icon(
                                        ui,
                                        icon_rect,
                                        entry.icon,
                                        if highlighted {
                                            pal.accent
                                        } else {
                                            pal.text_dim
                                        },
                                    );
                                    let label_color =
                                        if highlighted { pal.text } else { pal.text_dim };
                                    let label = crate::ui_library::single_line(
                                        &entry.label,
                                        13.0,
                                        label_color,
                                        (width - 240.0).max(140.0),
                                    );
                                    let galley = painter.layout_job(label);
                                    painter.galley(
                                        Pos2::new(rect.left() + 44.0, rect.center().y - 8.0),
                                        galley,
                                        label_color,
                                    );
                                    let detail = crate::ui_library::single_line(
                                        &entry.detail,
                                        10.5,
                                        pal.text_faint,
                                        (width - 300.0).max(100.0),
                                    );
                                    let detail = painter.layout_job(detail);
                                    painter.galley(
                                        Pos2::new(
                                            rect.right() - detail.size().x - 18.0,
                                            rect.center().y - 6.0,
                                        ),
                                        detail,
                                        pal.text_faint,
                                    );
                                    if response.clicked() {
                                        chosen = Some(entry.action.clone());
                                    }
                                }
                                ui.add_space(4.0);
                            });

                        ui.add_space(4.0);
                        let (line, _) =
                            ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover());
                        ui.painter().rect_filled(line, 0.0, pal.border);
                        ui.horizontal(|ui| {
                            ui.add_space(18.0);
                            ui.label(
                                crate::i18n::rich("↑↓ naviguer   Entrée ouvrir   Échap fermer")
                                    .size(10.0)
                                    .color(pal.text_faint),
                            );
                        });
                        ui.add_space(3.0);
                    });
            });

        if close {
            self.close_command_palette(ctx);
            return;
        }
        if let Some(action) = chosen {
            self.run_palette_action(ctx, action);
            self.close_command_palette(ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_recherche_ignore_les_accents_et_les_lettres_intermediaires() {
        assert!(fuzzy_score("parametres", "Paramètres").is_some());
        assert!(fuzzy_score("biblio", "Bibliothèque").is_some());
        assert!(fuzzy_score("witcher", "The Witcher 3: Wild Hunt").is_some());
        assert!(fuzzy_score("xyz", "The Witcher 3").is_none());
    }

    #[test]
    fn un_titre_commencant_par_la_requete_est_mieux_classe() {
        let start = fuzzy_score("portal", "Portal 2").unwrap();
        let later = fuzzy_score("portal", "The Portal 2").unwrap();
        assert!(start > later);
    }

    #[test]
    fn la_requete_vide_accepte_tout() {
        assert_eq!(fuzzy_score("", "n'importe quoi"), Some(0));
    }

    #[test]
    fn les_champs_d_un_jeu_ne_se_melangent_pas() {
        let entry = Entry::new(
            "JEUX",
            "Hollow Knight",
            "À jouer · Aucune session",
            "Team Cherry Metroidvania Action",
            Icon::Play,
            PaletteAction::Library,
        );
        assert!(entry.score("witcher").is_none());
        assert!(entry.score("hollow").is_some());
        assert!(entry.score("cherry").is_some());
    }
}
