use crate::lease_parser::{Lease, parse_leases};
use crate::ssh_client::SSHClient;
use eframe::egui;
use egui_extras::{Column, TableBuilder};

#[derive(PartialEq)]
enum Tab {
    Editor,
    Instructions,
}

pub struct WhitelistApp {
    // Connection info
    host: String,
    user: String,
    pass: String,

    // State
    client: Option<SSHClient>,
    leases: Vec<Lease>,
    status: String,

    // UI state
    editing_lease: Option<Lease>,
    original_lease: Option<Lease>,
    deleting_lease: Option<Lease>,
    is_adding: bool,
    selected_tab: Tab,
}

impl WhitelistApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            host: "192.168.88.1".to_owned(),
            user: "admin".to_owned(),
            pass: "".to_owned(),
            client: None,
            leases: Vec::new(),
            status: "Не під'єднано".to_owned(),
            editing_lease: None,
            original_lease: None,
            deleting_lease: None,
            is_adding: false,
            selected_tab: Tab::Editor,
        }
    }

    fn connect_and_refresh(&mut self) {
        self.status = "З'єднуюсь...".to_owned();
        match SSHClient::connect(&self.host, &self.user, &self.pass) {
            Ok(client) => {
                self.status = "З'єднано".to_owned();
                self.client = Some(client);
                self.refresh_leases();
            }
            Err(e) => {
                self.status = format!("Помилка під'єднання: {}", e);
            }
        }
    }

    fn refresh_leases(&mut self) {
        if let Some(client) = &mut self.client {
            match client.execute("/ip/dhcp-server/lease/export") {
                Ok(output) => {
                    self.leases = parse_leases(&output);
                    self.status = format!("Завантажено {} адрес", self.leases.len());
                }
                Err(e) => {
                    self.status = format!("Помилка при отриманні адрес: {}", e);
                }
            }
        }
    }

    fn save_lease(&mut self, lease: Lease, is_new: bool) {
        if let Some(client) = &mut self.client {
            let cmd = if is_new {
                format!(
                    "/ip/dhcp-server/lease/add address={} mac-address={} server={} comment=\"{}\" block-access={}",
                    lease.address.unwrap_or("0.0.0.0".to_owned()),
                    lease.mac_address,
                    lease.server,
                    lease.comment.unwrap_or_default(),
                    if lease.block_access { "yes" } else { "no" }
                )
            } else {
                let find_query = if let Some(original) = &self.original_lease {
                    generate_find_query(original)
                } else {
                    format!("[find mac-address=\"{}\"]", lease.mac_address)
                };

                format!(
                    "/ip/dhcp-server/lease/set {} address={} server={} comment=\"{}\" block-access={}",
                    find_query,
                    lease.address.unwrap_or("0.0.0.0".to_owned()),
                    lease.server,
                    lease.comment.unwrap_or_default(),
                    if lease.block_access { "yes" } else { "no" }
                )
            };

            match client.execute(&cmd) {
                Ok(_) => {
                    self.status = "Адреса успішно збережена".to_owned();
                    self.refresh_leases();
                }
                Err(e) => {
                    self.status = format!("Помилка збереження адреси: {}", e);
                }
            }
        }
    }

    fn delete_lease(&mut self, lease: Lease) {
        if let Some(client) = &mut self.client {
            let find_query = generate_find_query(&lease);
            let cmd = format!("/ip/dhcp-server/lease/remove {}", find_query);

            match client.execute(&cmd) {
                Ok(_) => {
                    self.status = "Адресу видалено".to_owned();
                    self.refresh_leases();
                }
                Err(e) => {
                    self.status = format!("Помилка видалення: {}", e);
                }
            }
        }
    }

}

fn generate_find_query(lease: &Lease) -> String {
        let mut parts = vec![format!("mac-address=\"{}\"", lease.mac_address)];

        if let Some(addr) = &lease.address {
            if !addr.is_empty() {
                parts.push(format!("address=\"{}\"", addr));
            }
        }

        parts.push(format!("server=\"{}\"", lease.server));

        if let Some(comment) = &lease.comment {
            if !comment.is_empty() {
                parts.push(format!("comment=\"{}\"", comment));
            }
        }

        parts.push(format!(
            "block-access={}",
            if lease.block_access { "yes" } else { "no" }
        ));

        if let Some(client_id) = &lease.client_id {
            if !client_id.is_empty() {
                parts.push(format!("client-id=\"{}\"", client_id));
            }
        }

        format!("[find {}]", parts.join(" "))
    }

impl eframe::App for WhitelistApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.vertical(|ui| {
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.selected_tab, Tab::Editor, "📋 Список адрес");
                ui.selectable_value(&mut self.selected_tab, Tab::Instructions, "ℹ️ Як знайти MAC");
            });
            ui.separator();
            ui.add_space(5.0);

            match self.selected_tab {
                Tab::Editor => {
                    ui.heading("Редактор білих адрес для MikroTik");

                    // Connection Panel
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Мікротик:");
                            ui.text_edit_singleline(&mut self.host);
                            ui.label("Користувач:");
                            ui.text_edit_singleline(&mut self.user);
                            ui.label("Пароль:");
                            ui.add(egui::TextEdit::singleline(&mut self.pass).password(true));

                            if ui.button("З'єднатися").clicked() {
                                self.connect_and_refresh();
                            }
                        });
                    });

                    ui.add_space(10.0);

                    // Status Bar
                    ui.horizontal(|ui| {
                        ui.label(format!("Статус: {}", self.status));
                        if self.client.is_some() {
                            if ui.button("Оновити").clicked() {
                                self.refresh_leases();
                            }
                            if ui.button("Додати адресу").clicked() {
                                self.editing_lease = Some(Lease::default());
                                self.is_adding = true;
                            }
                        }
                    });

                    ui.add_space(10.0);

                    // Table View
                    if !self.leases.is_empty() {
                        egui::ScrollArea::horizontal().show(ui, |ui| {
                            let table = TableBuilder::new(ui)
                                .striped(true)
                                .resizable(true)
                                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                                .column(Column::auto())
                                .column(Column::auto()) // Actions
                                .column(Column::auto()) // Blocked
                                .column(Column::initial(120.0).at_least(100.0))
                                .column(Column::initial(150.0).at_least(120.0))
                                .column(Column::initial(100.0))
                                .column(Column::initial(200.0).at_least(150.0));

                            let header_bg = egui::Color32::from_gray(220);
                            let header_text = egui::Color32::BLACK;
                            let style_header = |ui: &mut egui::Ui, text: &str| {
                                egui::Frame::NONE
                                    .fill(header_bg)
                                    .corner_radius(2.0)
                                    .inner_margin(egui::Margin::symmetric(4, 2))
                                    .show(ui, |ui| {
                                        ui.label(egui::RichText::new(text).heading().color(header_text));
                                    });
                            };

                            table
                                .header(25.0, |mut header| {
                                    header.col(|ui| {
                                        style_header(ui, "№");
                                    });
                                    header.col(|ui| {
                                        style_header(ui, "Дії");
                                    });
                                    header.col(|ui| {
                                        style_header(ui, "Блок");
                                    });
                                    header.col(|ui| {
                                        style_header(ui, "IP-адреса");
                                    });
                                    header.col(|ui| {
                                        style_header(ui, "MAC-адреса");
                                    });
                                    header.col(|ui| {
                                        style_header(ui, "Сервер");
                                    });
                                    header.col(|ui| {
                                        style_header(ui, "Коментар");
                                    });
                                })
                                .body(|body| {
                                    body.rows(25.0, self.leases.len(), |mut row| {
                                        let row_index = row.index();
                                        let lease = self.leases[row_index].clone();

                                        row.col(|ui| {
                                            ui.label(row_index.to_string());
                                        });
                                        row.col(|ui| {
                                            ui.horizontal(|ui| {
                                                if ui.button("⚙").on_hover_text("Редагувати").clicked() {
                                                    self.editing_lease = Some(lease.clone());
                                                    self.original_lease = Some(lease.clone());
                                                    self.is_adding = false;
                                                }
                                                if ui.button(egui::RichText::new("❌").color(egui::Color32::from_rgb(200, 50, 50))).on_hover_text("Видалити").clicked() {
                                                    self.deleting_lease = Some(lease.clone());
                                                }
                                            });
                                        });
                                        row.col(|ui| {
                                            if lease.block_access {
                                                ui.label(egui::RichText::new(" 🔒").size(16.0));
                                            } else {
                                                ui.label(egui::RichText::new("  ").size(16.0));
                                            }
                                        });
                                        row.col(|ui| {
                                            ui.label(lease.address.as_deref().unwrap_or(" "));
                                        });
                                        row.col(|ui| {
                                            ui.label(&lease.mac_address);
                                        });
                                        row.col(|ui| {
                                            ui.label(&lease.server);
                                        });
                                        row.col(|ui| {
                                            ui.label(lease.comment.as_deref().unwrap_or("-"));
                                        });
                                    });
                                });
                        });
                    } else if self.client.is_some() {
                        ui.label("Не завантажено адреси або не знайдено адрес.");
                    }
                }
                Tab::Instructions => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.heading("Як знайти та налаштувати MAC-адресу");
                        });
                        ui.add_space(15.0);

                        ui.group(|ui| {
                            ui.vertical(|ui| {
                                let section_bg = egui::Color32::from_gray(220);
                                let section_text = egui::Color32::BLACK;

                                egui::Frame::NONE
                                    .fill(section_bg)
                                    .corner_radius(4.0)
                                    .inner_margin(egui::Margin::symmetric(8, 4))
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.label(egui::RichText::new("🔍 Як дізнатися свою MAC-адресу:").strong().size(16.0).color(section_text));
                                    });
                                
                                ui.add_space(5.0);

                                ui.label(egui::RichText::new("• Android:").heading());
                                ui.label("  Налаштування -> Про телефон -> Статус (або Відомості про обладнання).");
                                ui.add_space(5.0);

                                ui.label(egui::RichText::new("• iPhone/iPad (iOS):").heading());
                                ui.label("  Параметри -> Загальні -> Про пристрій -> Адреса Wi-Fi.");
                                ui.add_space(5.0);

                                ui.label(egui::RichText::new("• Windows:").heading());
                                ui.label("  Відкрийте Командний рядок (cmd) і введіть `getmac` або `ipconfig /all`.");
                                ui.label("  Або: Налаштування -> Мережа та Інтернет -> Wi-Fi -> Властивості обладнання.");
                                ui.add_space(5.0);

                                ui.label(egui::RichText::new("• macOS:").heading());
                                ui.label("  Системні параметри -> Мережа -> Wi-Fi -> Додатково -> Обладнання.");
                            });
                        });

                        ui.add_space(20.0);

                        ui.group(|ui| {
                            ui.vertical(|ui| {
                                let section_bg = egui::Color32::from_gray(220);
                                let section_text = egui::Color32::BLACK;

                                egui::Frame::NONE
                                    .fill(section_bg)
                                    .corner_radius(4.0)
                                    .inner_margin(egui::Margin::symmetric(8, 4))
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.label(egui::RichText::new("🔒 Як зробити MAC-адресу постійною (вимкнути випадкову адресу):").strong().size(16.0).color(section_text));
                                    });

                                ui.add_space(5.0);
                                ui.label("Більшість сучасних пристроїв використовують випадкову MAC-адресу для безпеки. Щоб редактор працював правильно, потрібно встановити постійну адресу для вашої мережі.");
                                ui.add_space(10.0);

                                ui.label(egui::RichText::new("• Android:").heading());
                                ui.label("  1. Налаштування -> Wi-Fi.");
                                ui.label("  2. Натисніть на іконку налаштувань (шестерня) біля назви вашої мережі.");
                                ui.label("  3. Знайдіть пункт 'Тип MAC-адреси'.");
                                ui.label("  4. Виберіть 'MAC-адреса пристрою' замість 'Рандомізована'.");
                                ui.add_space(5.0);

                                ui.label(egui::RichText::new("• iPhone/iPad (iOS):").heading());
                                ui.label("  1. Параметри -> Wi-Fi.");
                                ui.label("  2. Натисніть кнопку 'i' біля вашої мережі.");
                                ui.label("  3. Вимкніть перемикач 'Приватна адреса Wi-Fi'.");
                                ui.add_space(5.0);

                                ui.label(egui::RichText::new("• Windows:").heading());
                                ui.label("  1. Налаштування -> Мережа та Інтернет -> Wi-Fi.");
                                ui.label("  2. Виберіть вашу мережу.");
                                ui.label("  3. Вимкніть 'Випадкові апаратні адреси' (Random Hardware Addresses).");
                            });
                        });
                    });
                }
            }
        });

        // Edit/Add Window
        let ctx = ui.ctx();
        let mut should_close = false;
        if let Some(mut lease) = self.editing_lease.take() {
            let mut open = true;
            let title = if self.is_adding {
                "Додавання адреси"
            } else {
                "Редагування адреси"
            };

            egui::Window::new(title).open(&mut open).show(ctx, |ui| {
                egui::Grid::new("edit_grid")
                    .num_columns(2)
                    .spacing([40.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("IP-адреса:");
                        let mut address = lease.address.clone().unwrap_or_default();
                        ui.text_edit_singleline(&mut address);
                        lease.address = if address.is_empty() {
                            None
                        } else {
                            Some(address)
                        };
                        ui.end_row();

                        ui.label("MAC-адреса:");
                        ui.text_edit_singleline(&mut lease.mac_address);
                        ui.end_row();

                        ui.label("Сервер:");
                        ui.text_edit_singleline(&mut lease.server);
                        ui.end_row();

                        ui.label("Коментар:");
                        let mut comment = lease.comment.clone().unwrap_or_default();
                        ui.text_edit_singleline(&mut comment);
                        lease.comment = Some(comment);
                        ui.end_row();

                        ui.label("Заблокувати доступ:");
                        ui.checkbox(&mut lease.block_access, "");
                        ui.end_row();
                    });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Зберегти").clicked() {
                        self.save_lease(lease.clone(), self.is_adding);
                        should_close = true;
                    }
                    if ui.button("Скасувати").clicked() {
                        should_close = true;
                    }
                });
            });

            if open && !should_close {
                self.editing_lease = Some(lease);
            }
        }

        // Deletion Confirmation Window
        if let Some(lease) = self.deleting_lease.take() {
            let mut open = true;
            let mut should_delete = false;
            let mut should_close = false;

            egui::Window::new("Підтвердження видалення")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("⚠️ Ви впевнені, що хочете видалити цей запис?").heading().color(egui::Color32::RED));
                        ui.label("Цю дію неможливо буде скасувати.");
                        ui.add_space(10.0);

                        ui.group(|ui| {
                            egui::Grid::new("delete_info_grid")
                                .num_columns(2)
                                .spacing([20.0, 4.0])
                                .show(ui, |ui| {
                                    ui.label("IP-адреса:");
                                    ui.label(lease.address.as_deref().unwrap_or("-"));
                                    ui.end_row();

                                    ui.label("MAC-адреса:");
                                    ui.label(&lease.mac_address);
                                    ui.end_row();

                                    ui.label("Сервер:");
                                    ui.label(&lease.server);
                                    ui.end_row();

                                    ui.label("Коментар:");
                                    ui.label(lease.comment.as_deref().unwrap_or("-"));
                                    ui.end_row();

                                    ui.label("Заблоковано:");
                                    ui.label(if lease.block_access {"так"} else {"ні"});
                                    ui.end_row();
                                });
                        });

                        ui.add_space(15.0);
                        ui.horizontal(|ui| {
                            if ui.button("Видалити").clicked() {
                                should_delete = true;
                                should_close = true;
                            }
                            if ui.button("Скасувати").clicked() {
                                should_close = true;
                            }
                        });
                    });
                });

            if should_delete {
                self.delete_lease(lease);
            } else if open && !should_close {
                self.deleting_lease = Some(lease);
            }
        }
    }
}
