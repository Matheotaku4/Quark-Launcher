//! Vues enregistrées de la bibliothèque : barre de sélection, application et
//! fenêtre de gestion. Une vue ne stocke que des réglages, jamais de chemins
//! ni d'objets d'interface.

use crate::app::QuarkApp;
use crate::config::{GameStatus, GroupBy, LibrarySort, SavedView};
use crate::widgets::{self, Icon};
use eframe::egui::{self, Align, Color32, Layout, RichText, Vec2};

impl QuarkApp {
    /// Bouton « Filtres » et son panneau, comme la bibliothèque Steam : statut,
    /// genre et tags sont regroupés ici au lieu d'occuper plusieurs rangées de
    /// pastilles. Les choix se combinent et le panneau reste ouvert tant que le
    /// clic est à l'intérieur.
    pub(crate) fn filter_menu(&mut self, ui: &mut egui::Ui, indent: f32) {
        let pal = self.pal;
        let tags = self.lib.all_tags();
        let genres = self.lib.all_genres();
        let mut status = self.lib.settings.home_status;
        let mut tag = self.lib.settings.home_tag.clone();
        let mut genre = self.lib.settings.home_genre.clone();
        let active = usize::from(status != GameStatus::None)
            + usize::from(!tag.is_empty())
            + usize::from(!genre.is_empty());
        let label = if active == 0 {
            crate::i18n::tr("Filtres")
        } else {
            format!("{} · {active}", crate::i18n::tr("Filtres"))
        };

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(indent);
            let response = widgets::ghost_button(ui, &label, &pal, Icon::Filter);
            egui::Popup::from_toggle_button_response(&response)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .frame(
                    egui::Frame::menu(ui.style())
                        .fill(pal.header)
                        .stroke(egui::Stroke::new(1.0_f32, pal.border))
                        .corner_radius(8.0)
                        .inner_margin(12.0),
                )
                .show(|ui| {
                    ui.set_min_width(300.0);
                    ui.set_max_width(340.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            crate::i18n::rich("FILTRES")
                                .size(10.0)
                                .strong()
                                .color(pal.accent),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if active > 0
                                && widgets::ghost_button(ui, "Tout effacer", &pal, Icon::None)
                                    .clicked()
                            {
                                status = GameStatus::None;
                                tag.clear();
                                genre.clear();
                            }
                        });
                    });
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .id_salt("quark_filter_menu_scroll")
                        .max_height(430.0)
                        .show(ui, |ui| {
                            ui.label(
                                crate::i18n::rich("Statut")
                                    .size(9.5)
                                    .strong()
                                    .color(pal.text_faint),
                            );
                            ui.add_space(3.0);
                            ui.horizontal_wrapped(|ui| {
                                if widgets::chip_colored(
                                    ui,
                                    "Tous",
                                    status == GameStatus::None,
                                    &pal,
                                    pal.accent,
                                ) {
                                    status = GameStatus::None;
                                }
                                for option in GameStatus::ALL.iter().skip(1) {
                                    let selected = status == *option;
                                    let (r, g, b) = option.color();
                                    if widgets::chip_colored(
                                        ui,
                                        &option.label(),
                                        selected,
                                        &pal,
                                        Color32::from_rgb(r, g, b),
                                    ) {
                                        status = if selected { GameStatus::None } else { *option };
                                    }
                                    ui.add_space(4.0);
                                }
                            });
                            if !genres.is_empty() {
                                ui.add_space(10.0);
                                ui.label(
                                    crate::i18n::rich("Genre")
                                        .size(9.5)
                                        .strong()
                                        .color(pal.text_faint),
                                );
                                ui.add_space(3.0);
                                ui.horizontal_wrapped(|ui| {
                                    for (name, count) in &genres {
                                        let selected = genre.eq_ignore_ascii_case(name);
                                        if widgets::chip(
                                            ui,
                                            &format!("{name} · {count}"),
                                            selected,
                                            &pal,
                                        ) {
                                            genre = if selected {
                                                String::new()
                                            } else {
                                                name.clone()
                                            };
                                        }
                                        ui.add_space(4.0);
                                    }
                                });
                            }
                            if !tags.is_empty() {
                                ui.add_space(10.0);
                                ui.label(
                                    crate::i18n::rich("Tags")
                                        .size(9.5)
                                        .strong()
                                        .color(pal.text_faint),
                                );
                                ui.add_space(3.0);
                                ui.horizontal_wrapped(|ui| {
                                    for (name, count) in &tags {
                                        let selected = tag.eq_ignore_ascii_case(name);
                                        if widgets::chip(
                                            ui,
                                            &format!("#{name} · {count}"),
                                            selected,
                                            &pal,
                                        ) {
                                            tag = if selected {
                                                String::new()
                                            } else {
                                                name.clone()
                                            };
                                        }
                                        ui.add_space(4.0);
                                    }
                                });
                            }
                        });
                });
        });

        if status != self.lib.settings.home_status
            || tag != self.lib.settings.home_tag
            || genre != self.lib.settings.home_genre
        {
            self.lib.settings.home_status = status;
            self.lib.settings.home_tag = tag;
            self.lib.settings.home_genre = genre;
            self.touch();
        }
    }

    /// Barre compacte au-dessus des filtres : vues nommées, tri et
    /// regroupement. Elle n'apparaît que dans la colonne de la bibliothèque.
    pub(crate) fn library_view_bar(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        let views = self.lib.settings.saved_views.clone();
        let active = views
            .iter()
            .find(|view| view.matches(&self.lib.settings, &self.filter))
            .map(|view| view.name.clone())
            .unwrap_or_else(|| "Choisir une vue…".to_string());
        let mut apply: Option<usize> = None;

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(crate::i18n::rich("VUES").size(10.0).color(pal.text_faint));
            egui::ComboBox::from_id_salt("quark_saved_views")
                .selected_text(crate::i18n::rich(active).size(11.0))
                .width(118.0)
                .show_ui(ui, |ui| {
                    if views.is_empty() {
                        ui.label(
                            crate::i18n::rich("Aucune vue enregistrée")
                                .size(11.0)
                                .color(pal.text_faint),
                        );
                    }
                    for (index, view) in views.iter().enumerate() {
                        if ui
                            .selectable_label(
                                view.matches(&self.lib.settings, &self.filter),
                                &view.name,
                            )
                            .clicked()
                        {
                            apply = Some(index);
                        }
                    }
                });
            if widgets::icon_button(
                ui,
                24.0,
                Icon::Plus,
                &pal,
                "Enregistrer la vue actuelle (filtres, tri et regroupement)",
            )
            .clicked()
            {
                self.saved_view_name.clear();
                self.saved_view_error = None;
                self.saved_views_open = true;
            }
        });

        if self.home {
            if let Some(index) = apply {
                self.apply_saved_view(index);
            }
            return;
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let mut sort = self.lib.settings.library_sort;
            egui::ComboBox::from_id_salt("quark_library_sort")
                .selected_text(crate::i18n::rich(format!("Tri : {}", sort.label())).size(11.0))
                .width(104.0)
                .show_ui(ui, |ui| {
                    for option in LibrarySort::ALL {
                        ui.selectable_value(&mut sort, option, option.label());
                    }
                });
            if sort != self.lib.settings.library_sort {
                self.lib.settings.library_sort = sort;
                self.touch();
            }

            let mut group = self.lib.settings.group_by;
            egui::ComboBox::from_id_salt("quark_library_group")
                .selected_text(crate::i18n::rich(format!("Grouper : {}", group.label())).size(11.0))
                .width(118.0)
                .show_ui(ui, |ui| {
                    for option in GroupBy::ALL {
                        ui.selectable_value(&mut group, option, option.label());
                    }
                })
                .response
                .on_hover_text("Le regroupement s'applique à la vue liste.");
            if group != self.lib.settings.group_by {
                self.lib.settings.group_by = group;
                self.touch();
            }
        });

        if let Some(index) = apply {
            self.apply_saved_view(index);
        }
    }

    pub(crate) fn apply_saved_view(&mut self, index: usize) {
        let Some(view) = self.lib.settings.saved_views.get(index).cloned() else {
            return;
        };
        view.apply_to(&mut self.lib.settings);
        self.filter = view.query;
        self.home = false;
        self.touch();
        self.toast_info(format!("Vue « {} » appliquée.", view.name));
    }

    fn save_current_view(&mut self) {
        let name = self.saved_view_name.trim().to_string();
        if name.is_empty() {
            self.saved_view_error = Some("Donne un nom à la vue.".to_string());
            return;
        }
        let view = SavedView::capture(&name, &self.lib.settings, &self.filter);
        let position = self
            .lib
            .settings
            .saved_views
            .iter()
            .position(|saved| saved.name.eq_ignore_ascii_case(&view.name));
        match position {
            Some(index) => self.lib.settings.saved_views[index] = view,
            None if self.lib.settings.saved_views.len() >= 32 => {
                self.saved_view_error =
                    Some("Limite de 32 vues atteinte : supprime une vue d'abord.".to_string());
                return;
            }
            None => self.lib.settings.saved_views.push(view),
        }
        self.saved_view_name.clear();
        self.saved_view_error = None;
        self.touch();
        self.toast_info(format!("Vue « {} » enregistrée.", name));
    }

    fn delete_saved_view(&mut self, index: usize) {
        if index >= self.lib.settings.saved_views.len() {
            return;
        }
        let view = self.lib.settings.saved_views.remove(index);
        self.touch();
        self.toast_info(format!("Vue « {} » supprimée.", view.name));
    }

    pub(crate) fn saved_views_dialog(&mut self, ctx: &egui::Context) {
        if !self.saved_views_open {
            return;
        }
        let pal = self.pal;
        let views = self.lib.settings.saved_views.clone();
        let mut close = false;
        let mut save = false;
        let mut apply: Option<usize> = None;
        let mut delete: Option<usize> = None;

        let modal = egui::Modal::new(egui::Id::new("quark_saved_views")).show(ctx, |ui| {
            ui.set_min_width(560.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        crate::i18n::rich("Vues enregistrées")
                            .size(18.0)
                            .color(pal.text),
                    );
                    ui.label(
                        crate::i18n::rich(
                            "Une vue garde la recherche, les filtres, le tri et le regroupement \
                             de la bibliothèque.",
                        )
                        .size(11.0)
                        .color(pal.text_faint),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::icon_button(ui, 28.0, Icon::Close, &pal, "Fermer").clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let response = ui.add_sized(
                    Vec2::new(360.0, 30.0),
                    egui::TextEdit::singleline(&mut self.saved_view_name)
                        .hint_text(crate::i18n::tr(
                            "Nom de la vue, par exemple « À jouer ce soir »",
                        ))
                        .font(egui::FontId::proportional(12.5)),
                );
                if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    save = true;
                }
                if widgets::accent_button(
                    ui,
                    Vec2::new(120.0, 30.0),
                    "Enregistrer",
                    &pal,
                    !self.saved_view_name.trim().is_empty(),
                    Icon::None,
                )
                .clicked()
                {
                    save = true;
                }
            });
            if let Some(error) = &self.saved_view_error {
                ui.add_space(4.0);
                ui.label(crate::i18n::rich(error).size(11.0).color(pal.danger));
            }
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            if views.is_empty() {
                ui.label(
                    crate::i18n::rich(
                        "Aucune vue pour l'instant. Filtre la bibliothèque, puis enregistre-la \
                         ici.",
                    )
                    .size(12.0)
                    .color(pal.text_dim),
                );
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("quark_saved_views_scroll")
                    .max_height((ctx.screen_rect().height() - 300.0).max(160.0))
                    .show(ui, |ui| {
                        for (index, view) in views.iter().enumerate() {
                            egui::Frame::new()
                                .fill(pal.surface)
                                .stroke(egui::Stroke::new(1.0_f32, pal.border))
                                .corner_radius(7.0)
                                .inner_margin(egui::Margin::symmetric(10, 8))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(
                                                crate::i18n::rich(&view.name)
                                                    .size(12.5)
                                                    .color(pal.text),
                                            );
                                            let mut details = Vec::new();
                                            if view.status != GameStatus::None {
                                                details.push(view.status.label().to_string());
                                            }
                                            if !view.tag.is_empty() {
                                                details.push(format!("#{}", view.tag));
                                            }
                                            if !view.genre.is_empty() {
                                                details.push(view.genre.clone());
                                            }
                                            if !view.query.is_empty() {
                                                details.push(format!("« {} »", view.query));
                                            }
                                            details.push(format!(
                                                "Groupe : {}",
                                                view.group_by.label()
                                            ));
                                            details.push(view.view.label().to_string());
                                            details.push(format!("Tri : {}", view.sort.label()));
                                            ui.label(
                                                crate::i18n::rich(details.join(" · "))
                                                    .size(10.5)
                                                    .color(pal.text_faint),
                                            );
                                        });
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                if widgets::icon_button(
                                                    ui,
                                                    24.0,
                                                    Icon::Trash,
                                                    &pal,
                                                    "Supprimer cette vue",
                                                )
                                                .clicked()
                                                {
                                                    delete = Some(index);
                                                }
                                                if widgets::ghost_button(
                                                    ui,
                                                    "Appliquer",
                                                    &pal,
                                                    Icon::ArrowRight,
                                                )
                                                .clicked()
                                                {
                                                    apply = Some(index);
                                                }
                                            },
                                        );
                                    });
                                });
                            ui.add_space(6.0);
                        }
                    });
            }
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);
            ui.label(
                crate::i18n::rich(
                    "Astuce : une vue peut aussi être appliquée depuis le menu « Vues » de la \
                     bibliothèque.",
                )
                .size(10.5)
                .color(pal.text_faint),
            );
        });

        if close || modal.should_close() {
            self.saved_views_open = false;
            self.saved_view_error = None;
        }
        if save {
            self.save_current_view();
        }
        if let Some(index) = delete {
            self.delete_saved_view(index);
        }
        if let Some(index) = apply {
            self.apply_saved_view(index);
            self.saved_views_open = false;
        }
    }
}
