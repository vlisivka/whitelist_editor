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
    is_adding: bool,
    search_query: String,
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
            status: "Not connected".to_owned(),
            editing_lease: None,
            is_adding: false,
            search_query: String::new(),
            selected_tab: Tab::Editor,
        }
    }

    fn connect_and_refresh(&mut self) {
        self.status = "Connecting...".to_owned();
        match SSHClient::connect(&self.host, &self.user, &self.pass) {
            Ok(client) => {
                self.status = "Connected".to_owned();
                self.client = Some(client);
                self.refresh_leases();
            }
            Err(e) => {
                self.status = format!("Connection error: {}", e);
            }
        }
    }

    fn refresh_leases(&mut self) {
        if let Some(client) = &mut self.client {
            match client.execute("/ip/dhcp-server/lease/export") {
                Ok(output) => {
                    self.leases = parse_leases(&output);
                    self.status = format!("Loaded {} leases", self.leases.len());
                }
                Err(e) => {
                    self.status = format!("Error fetching leases: {}", e);
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
                // To edit, find by MAC.
                // MikroTik ROS 7.x set [find mac-address=X] ...
                format!(
                    "/ip/dhcp-server/lease/set [find mac-address=\"{}\"] address={} server={} comment=\"{}\" block-access={}",
                    lease.mac_address,
                    lease.address.unwrap_or("0.0.0.0".to_owned()),
                    lease.server,
                    lease.comment.unwrap_or_default(),
                    if lease.block_access { "yes" } else { "no" }
                )
            };

            match client.execute(&cmd) {
                Ok(_) => {
                    self.status = "Lease saved successfully".to_owned();
                    self.refresh_leases();
                }
                Err(e) => {
                    self.status = format!("Error saving lease: {}", e);
                }
            }
        }
    }
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
                    ui.heading("MikroTik Whitelist Editor");

                    // Connection Panel
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Host:");
                            ui.text_edit_singleline(&mut self.host);
                            ui.label("User:");
                            ui.text_edit_singleline(&mut self.user);
                            ui.label("Pass:");
                            ui.add(egui::TextEdit::singleline(&mut self.pass).password(true));

                            if ui.button("Connect").clicked() {
                                self.connect_and_refresh();
                            }
                        });
                    });

                    ui.add_space(10.0);

                    // Status Bar
                    ui.horizontal(|ui| {
                        ui.label(format!("Status: {}", self.status));
                        if self.client.is_some() {
                            if ui.button("Refresh").clicked() {
                                self.refresh_leases();
                            }
                            if ui.button("Add New Lease").clicked() {
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
                                        style_header(ui, "#");
                                    });
                                    header.col(|ui| {
                                        style_header(ui, "Actions");
                                    });
                                    header.col(|ui| {
                                        style_header(ui, "Blocked");
                                    });
                                    header.col(|ui| {
                                        style_header(ui, "Address");
                                    });
                                    header.col(|ui| {
                                        style_header(ui, "MAC Address");
                                    });
                                    header.col(|ui| {
                                        style_header(ui, "Server");
                                    });
                                    header.col(|ui| {
                                        style_header(ui, "Comment");
                                    });
                                })
                                .body(|body| {
                                    body.rows(25.0, self.leases.len(), |mut row| {
                                        let row_index = row.index();
                                        let lease = &self.leases[row_index];

                                        row.col(|ui| {
                                            ui.label(row_index.to_string());
                                        });
                                        row.col(|ui| {
                                            if ui.button("Edit").clicked() {
                                                self.editing_lease = Some(lease.clone());
                                                self.is_adding = false;
                                            }
                                        });
                                        row.col(|ui| {
                                            let mut blocked = lease.block_access;
                                            ui.add_enabled(
                                                false,
                                                egui::Checkbox::without_text(&mut blocked),
                                            );
                                        });
                                        row.col(|ui| {
                                            ui.label(lease.address.as_deref().unwrap_or("-"));
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
                        ui.label("No leases found or not loaded yet.");
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
                "Add Lease"
            } else {
                "Edit Lease"
            };

            egui::Window::new(title).open(&mut open).show(ctx, |ui| {
                egui::Grid::new("edit_grid")
                    .num_columns(2)
                    .spacing([40.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Address:");
                        let mut address = lease.address.clone().unwrap_or_default();
                        ui.text_edit_singleline(&mut address);
                        lease.address = if address.is_empty() {
                            None
                        } else {
                            Some(address)
                        };
                        ui.end_row();

                        ui.label("MAC Address:");
                        ui.text_edit_singleline(&mut lease.mac_address);
                        ui.end_row();

                        ui.label("Server:");
                        ui.text_edit_singleline(&mut lease.server);
                        ui.end_row();

                        ui.label("Comment:");
                        let mut comment = lease.comment.clone().unwrap_or_default();
                        ui.text_edit_singleline(&mut comment);
                        lease.comment = Some(comment);
                        ui.end_row();

                        ui.label("Block Access:");
                        ui.checkbox(&mut lease.block_access, "");
                        ui.end_row();
                    });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        self.save_lease(lease.clone(), self.is_adding);
                        should_close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        should_close = true;
                    }
                });
            });

            if open && !should_close {
                self.editing_lease = Some(lease);
            }
        }
    }
}
