use crate::mikrotik_data::{
    DhcpData, Lease, escape_mikrotik, find_first_free_ip, find_network_for_server, is_ip_in_range,
    is_ip_unique, is_valid_ipv4, is_valid_mac, parse_all,
};
use crate::ssh_client::{SSHClient, SSHConnector};
use eframe::egui;
use egui_extras::{Column, TableBuilder};

#[derive(PartialEq, Debug, serde::Deserialize, serde::Serialize)]
enum Tab {
    Editor,
    Instructions,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct WhitelistApp {
    // Connection info
    host: String,
    user: String,

    #[serde(skip)]
    pass: String,

    // State
    #[serde(skip)]
    client: Option<Box<dyn SSHConnector>>,
    #[serde(skip)]
    data: DhcpData,
    #[serde(skip)]
    status: String,

    // UI state
    #[serde(skip)]
    editing_lease: Option<Lease>,
    #[serde(skip)]
    original_lease: Option<Lease>,
    #[serde(skip)]
    deleting_lease: Option<Lease>,
    #[serde(skip)]
    is_adding: bool,
    #[serde(skip)]
    selected_tab: Tab,
}

impl Default for WhitelistApp {
    fn default() -> Self {
        Self {
            host: "192.168.88.1".to_owned(),
            user: "admin".to_owned(),
            pass: "".to_owned(),
            client: None,
            data: DhcpData::default(),
            status: "Не під'єднано".to_owned(),
            editing_lease: None,
            original_lease: None,
            deleting_lease: None,
            is_adding: false,
            selected_tab: Tab::Editor,
        }
    }
}

impl WhitelistApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }
        Self::default()
    }

    fn connect_and_refresh(&mut self) {
        self.status = "З'єднуюсь...".to_owned();
        match SSHClient::connect(&self.host, &self.user, &self.pass) {
            Ok(client) => {
                self.status = "З'єднано".to_owned();
                self.client = Some(Box::new(client));
                self.refresh_data();
            }
            Err(e) => {
                self.status = format!("Помилка під'єднання: {}", e);
            }
        }
    }

    fn refresh_data(&mut self) {
        if let Some(client) = &mut self.client {
            match client.execute("/ip/dhcp-server/export") {
                Ok(output) => {
                    self.data = parse_all(&output);
                    self.status = format!(
                        "Завантажено {} адрес, {} серверів",
                        self.data.leases.len(),
                        self.data.servers.len()
                    );
                }
                Err(e) => {
                    self.status = format!("Помилка при отриманні даних: {}", e);
                }
            }
        }
    }

    fn save_lease(&mut self, lease: Lease, is_new: bool) {
        if let Some(client) = &mut self.client {
            let cmd = if is_new {
                format!(
                    "/ip/dhcp-server/lease/add address={} mac-address={} server={} comment={} block-access={}",
                    escape_mikrotik(lease.address.as_deref().unwrap_or("0.0.0.0")),
                    escape_mikrotik(&lease.mac_address),
                    escape_mikrotik(&lease.server),
                    escape_mikrotik(lease.comment.as_deref().unwrap_or("")),
                    if lease.block_access { "yes" } else { "no" }
                )
            } else {
                let find_query = if let Some(original) = &self.original_lease {
                    generate_find_query(original)
                } else {
                    format!("[find mac-address={}]", escape_mikrotik(&lease.mac_address))
                };

                format!(
                    "/ip/dhcp-server/lease/set {} address={} server={} comment={} block-access={}",
                    find_query,
                    escape_mikrotik(lease.address.as_deref().unwrap_or("0.0.0.0")),
                    escape_mikrotik(&lease.server),
                    escape_mikrotik(lease.comment.as_deref().unwrap_or("")),
                    if lease.block_access { "yes" } else { "no" }
                )
            };

            match client.execute(&cmd) {
                Ok(_) => {
                    self.status = "Адреса успішно збережена".to_owned();
                    self.refresh_data();
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
                    self.refresh_data();
                }
                Err(e) => {
                    self.status = format!("Помилка видалення: {}", e);
                }
            }
        }
    }
}

fn generate_find_query(lease: &Lease) -> String {
    let mut parts = vec![format!(
        "mac-address={}",
        escape_mikrotik(&lease.mac_address)
    )];

    if let Some(addr) = lease.address.as_ref().filter(|a| !a.is_empty()) {
        parts.push(format!("address={}", escape_mikrotik(addr)));
    }

    parts.push(format!("server={}", escape_mikrotik(&lease.server)));

    if let Some(comment) = lease.comment.as_ref().filter(|c| !c.is_empty()) {
        parts.push(format!("comment={}", escape_mikrotik(comment)));
    }

    if lease.block_access {
        parts.push("block-access=yes".to_owned());
    }

    if let Some(client_id) = lease.client_id.as_ref().filter(|c| !c.is_empty()) {
        parts.push(format!("client-id={}", escape_mikrotik(client_id)));
    }

    format!("[find {}]", parts.join(" "))
}

impl eframe::App for WhitelistApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
        ui.vertical(|ui| {
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.selected_tab, Tab::Editor, "📋 Список адрес");
                ui.selectable_value(&mut self.selected_tab, Tab::Instructions, "ℹ️ Як знайти MAC");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut theme = ui.ctx().options(|o| o.theme_preference);
                    let old_theme = theme;
                    theme.radio_buttons(ui);
                    if theme != old_theme {
                        ui.ctx().options_mut(|o| o.theme_preference = theme);
                    }
                });
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
                                self.refresh_data();
                            }
                            if ui.button("Додати адресу").clicked() {
                                let mut lease = Lease::default();
                                // If we have servers, pick the first one and suggest IP
                                if let Some(first_server) = self.data.servers.first() {
                                    lease.server = first_server.name.clone();
                                    if let Some(net) = find_network_for_server(first_server, &self.data.networks) {
                                        lease.address = find_first_free_ip(net, &self.data.leases);
                                    }
                                }
                                self.editing_lease = Some(lease);
                                self.is_adding = true;
                            }
                        }
                    });

                    ui.add_space(10.0);

                    // Table View
                    if !self.data.leases.is_empty() {
                        egui::ScrollArea::horizontal().show(ui, |ui| {
                            let table = TableBuilder::new(ui)
                                .id_salt("leases_table")
                                .striped(true)
                                .resizable(true)
                                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                                .column(Column::initial(30.0).at_least(30.0)) // №
                                .column(Column::initial(60.0).at_least(60.0)) // Дії
                                .column(Column::initial(40.0).at_least(40.0)) // Блок
                                .column(Column::initial(100.0).at_least(100.0)) // IP
                                .column(Column::initial(120.0).at_least(120.0)) // MAC
                                .column(Column::initial(80.0).at_least(80.0)) // Сервер
                                .column(Column::remainder()); // Коментар

                            let style_header = |ui: &mut egui::Ui, text: &str| {
                                egui::Frame::NONE
                                    .show(ui, |ui| {
                                        ui.label(egui::RichText::new(text).heading()
                                    );
                                    });
                            };

                            table
                                .header(28.0, |mut header| {
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
                                    body.rows(28.0, self.data.leases.len(), |mut row| {
                                        let row_index = row.index();
                                        let lease = self.data.leases[row_index].clone();

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
                let mut is_valid = true;

                egui::Grid::new("edit_grid")
                    .num_columns(2)
                    .spacing([40.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("IP-адреса:");
                        ui.vertical(|ui| {
                            let mut address = lease.address.clone().unwrap_or_default();
                            let _ip_edit_resp = ui.text_edit_singleline(&mut address);
                            lease.address = if address.is_empty() {
                                None
                            } else {
                                Some(address.clone())
                            };

                            // Validation logic
                            if let Some(net) = self
                                .data
                                .servers
                                .iter()
                                .find(|s| s.name == lease.server)
                                .and_then(|si| find_network_for_server(si, &self.data.networks))
                                && !is_valid_ipv4(
                                    &address,
                                    net,
                                    &self.data.leases,
                                    &lease.mac_address,
                                )
                            {
                                is_valid = false;
                                // Show specific error messages
                                if address.parse::<std::net::Ipv4Addr>().is_err() {
                                    ui.label(
                                        egui::RichText::new("❌ Невірний формат IP")
                                            .color(egui::Color32::LIGHT_RED)
                                            .size(10.0),
                                    );
                                } else if !is_ip_in_range(&address, net) {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "❌ Поза діапазоном {}",
                                            net.address
                                        ))
                                        .color(egui::Color32::LIGHT_RED)
                                        .size(10.0),
                                    );
                                } else if !is_ip_unique(
                                    &address,
                                    &self.data.leases,
                                    &lease.mac_address,
                                ) {
                                    ui.label(
                                        egui::RichText::new("⚠ Ця адреса вже використовується")
                                            .color(egui::Color32::LIGHT_RED)
                                            .size(10.0),
                                    );
                                }
                            }
                        });
                        ui.end_row();

                        ui.label("MAC-адреса:");
                        ui.vertical(|ui| {
                            ui.text_edit_singleline(&mut lease.mac_address);

                            if !is_valid_mac(&lease.mac_address) {
                                is_valid = false;
                                ui.label(
                                    egui::RichText::new(
                                        "❌ Невірний формат MAC (XX:XX:XX:XX:XX:XX)",
                                    )
                                    .color(egui::Color32::LIGHT_RED)
                                    .size(10.0),
                                );
                            }
                        });
                        ui.end_row();

                        ui.label("Сервер:");
                        let old_server = lease.server.clone();
                        egui::ComboBox::from_id_salt("server_combo")
                            .selected_text(&lease.server)
                            .show_ui(ui, |ui| {
                                for s in &self.data.servers {
                                    ui.selectable_value(&mut lease.server, s.name.clone(), &s.name);
                                }
                            });

                        // If server changed, suggest first free IP
                        if lease.server != old_server
                            && let Some(free_ip) = self
                                .data
                                .servers
                                .iter()
                                .find(|s| s.name == lease.server)
                                .and_then(|si| find_network_for_server(si, &self.data.networks))
                                .and_then(|net| find_first_free_ip(net, &self.data.leases))
                        {
                            lease.address = Some(free_ip);
                        }
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
                    let save_btn = egui::Button::new("Зберегти");
                    if ui.add_enabled(is_valid, save_btn).clicked() {
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
                        ui.label(
                            egui::RichText::new("⚠ Ви впевнені, що хочете видалити цей запис?")
                                .heading()
                                .color(egui::Color32::RED),
                        );
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
                                    ui.label(if lease.block_access { "так" } else { "ні" });
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
    });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh_client::MockSSHClient;
    use std::collections::HashMap;

    const MIKROTIK_EXPORT: &str = r#"
/ip dhcp-server
add add-arp=yes comment=guest interface=GSTVLAN name=guest-dhcp
add add-arp=yes comment=corp interface=CRPVLAN lease-time=1h name=corp-dhcp
add add-arp=yes comment=manage interface=MNGVLAN name=mng-server
/ip dhcp-server lease
add address=172.16.20.217 block-access=yes mac-address=A4:C6:9A:08:86:C8 server=corp-dhcp
add address=172.22.2.29 comment=029SYN mac-address=F4:1E:57:7F:D1:57 server=mng-server
/ip dhcp-server network
add address=172.16.20.0/23 comment=corp dns-server=172.16.20.1 gateway=172.16.20.1
add address=172.22.2.0/24 comment=manage dns-server=172.22.2.2 gateway=172.22.2.1
add address=192.168.10.0/24 comment=guest dns-server=192.168.10.1 gateway=192.168.10.1
"#;

    #[test]
    fn test_app_initial_state() {
        let app = WhitelistApp::default();
        assert_eq!(app.selected_tab, Tab::Editor);
        assert_eq!(app.status, "Не під'єднано");
        assert!(app.client.is_none());
    }

    #[test]
    fn test_refresh_data_with_mock() {
        let mut app = WhitelistApp::default();
        let mut responses = HashMap::new();
        responses.insert(
            "/ip/dhcp-server/export".to_string(),
            MIKROTIK_EXPORT.to_string(),
        );

        let mock_client = MockSSHClient { responses };
        app.client = Some(Box::new(mock_client));

        app.refresh_data();

        assert_eq!(app.data.leases.len(), 2);
        assert_eq!(app.data.servers.len(), 3);
        assert_eq!(app.data.networks.len(), 3);
        assert!(app.status.contains("Завантажено 2 адрес"));
    }

    #[test]
    fn test_tab_switching() {
        let mut app = WhitelistApp::default();
        assert_eq!(app.selected_tab, Tab::Editor);

        app.selected_tab = Tab::Instructions;
        assert_eq!(app.selected_tab, Tab::Instructions);
    }
}
