
# План: Сортування по стовпцях таблиці лізів

## Опис задачі

Додати інтерактивне сортування таблиці лізів по стовпцях — IP-адреса, MAC-адреса, Сервер, Коментар. Клік на заголовок стовпця встановлює сортування по цьому стовпцю. Повторний клік — змінює порядок (зростання / спадання). Третій клік — скидає сортування (повертає оригінальний порядок).

Методологія — TDD: спочатку тести, потім реалізація.

## Запитання перед початком

Які стовпці підлягають сортуванню? Запропонований варіант: IP-адреса, MAC-адреса, Сервер, Коментар. Стовпці «№», «Дії», «Блок» — без сортування. Підтвердьте або вкажіть інше.
Підтверджую.

Скидання сортування. При третьому кліку повернути оригінальний порядок (порядок, в якому дані прийшли з роутера).

Взаємодія з пошуком. Сортування має застосовуватись після фільтрації (тобто відсортований список — відфільтровані результати).

Збереження стану між сесіями. Чи потрібно зберігати обраний стовпець та порядок сортування між перезапусками програми (serde + eframe::Storage)? Пропозиція: ні, скидати до None при кожному запуску.

## Аналіз поточного коду

Актуальний pipeline (app.rs, рядки 282-370)

self.data.leases -> filter_leases(...) -> filtered: Vec<&Lease> -> рендер

Після завдання pipeline стане:

self.data.leases -> filter_leases(...) -> sort_leases(...) -> sorted: Vec<&Lease> -> рендер

### Що вже є у mikrotik_data.rs

Структура Lease (поля address, mac_address, server, comment)
Функція filter_leases — повертає Vec<&Lease>

### Що потрібно додати

| Де | Що |
| mikrotik_data.rs | SortColumn, SortOrder, sort_leases |
| app.rs | Поля sort_column, sort_order; метод toggle_sort; клікабельні заголовки |

## Запропоновані зміни

### Крок 1 — Тести для sort_leases (TDD, мають ВПАСТИ)
Файл: src/mikrotik_data.rs

Назва тесту	Сценарій

test_sort_leases_by_ip_asc	IP зростання: 172.16.x < 172.22.x
test_sort_leases_by_ip_desc	IP спадання: зворотній порядок
test_sort_leases_by_mac_asc	MAC зростання (лексикографічно)
test_sort_leases_by_server_asc	Сервер зростання
test_sort_leases_by_comment_asc	Коментар зростання; None — в кінці
test_sort_leases_no_sort	column = None — порядок незмінний

### Крок 2 — SortColumn, SortOrder, sort_leases в mikrotik_data.rs

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SortColumn { Ip, Mac, Server, Comment }
#[derive(Debug, Clone, PartialEq, Default)]
pub enum SortOrder { #[default] Asc, Desc }
pub fn sort_leases<'a>(
    leases: Vec<&'a Lease>,
    column: Option<&SortColumn>,
    order: &SortOrder,
) -> Vec<&'a Lease> {
    let Some(col) = column else { return leases; };
    let mut sorted = leases;
    sorted.sort_by(|a, b| {
        let cmp = match col {
            SortColumn::Ip => {
                let a_ip: Option<std::net::Ipv4Addr> =
                    a.address.as_deref().and_then(|s| s.parse().ok());
                let b_ip: Option<std::net::Ipv4Addr> =
                    b.address.as_deref().and_then(|s| s.parse().ok());
                match (a_ip, b_ip) {
                    (Some(a), Some(b)) => u32::from(a).cmp(&u32::from(b)),
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            }
            SortColumn::Mac => a.mac_address.to_lowercase().cmp(&b.mac_address.to_lowercase()),
            SortColumn::Server => a.server.to_lowercase().cmp(&b.server.to_lowercase()),
            SortColumn::Comment => {
                match (a.comment.is_some(), b.comment.is_some()) {
                    (false, true) => std::cmp::Ordering::Greater,
                    (true, false) => std::cmp::Ordering::Less,
                    _ => a.comment.as_deref().unwrap_or("").to_lowercase()
                             .cmp(&b.comment.as_deref().unwrap_or("").to_lowercase()),
                }
            }
        };
        if *order == SortOrder::Desc { cmp.reverse() } else { cmp }
    });
    sorted
}
```

IP-сортування числове (через Ipv4Addr -> u32): 10.0.0.9 < 10.0.0.10.

### Крок 3 — Стан UI в app.rs

Імпорт:

```rust
sort_leases, SortColumn, SortOrder,
```

Нові поля у WhitelistApp:

```rust
#[serde(skip)]
sort_column: Option<SortColumn>,
#[serde(skip)]
sort_order: SortOrder,
```

Ініціалізація у Default:

```rust
sort_column: None,
sort_order: SortOrder::default(),
```

### Крок 4 — Метод toggle_sort в app.rs

```rust
fn toggle_sort(&mut self, col: SortColumn) {
    if self.sort_column.as_ref() == Some(&col) {
        match self.sort_order {
            SortOrder::Asc  => self.sort_order = SortOrder::Desc,
            SortOrder::Desc => {
                self.sort_column = None;        // 3-й клік — скидання
                self.sort_order  = SortOrder::Asc;
            }
        }
    } else {
        self.sort_column = Some(col);
        self.sort_order  = SortOrder::Asc;
    }
}
```

### Крок 5 — UI: клікабельні заголовки в app.rs

Замінити style_header closure на sort_header, що показує ↑/↓ та повертає натиснуту колонку.

Щоб уникнути конфлікту borrow-checker (closure бере &self, а toggle_sort потребує &mut self), заголовки фіксують клік у локальній змінній, а виклик toggle_sort відбувається після блоку .header(...).

```rust
let mut clicked_col: Option<SortColumn> = None;
table.header(28.0, |mut header| {
    // №, Дії, Блок — без сортування
    header.col(|ui| { /* style незмінний */ });
    header.col(|ui| { /* style незмінний */ });
    header.col(|ui| { /* style незмінний */ });
    // Клікабельні стовпці
    for (label, col_val) in [
        ("IP-адреса",  SortColumn::Ip),
        ("MAC-адреса", SortColumn::Mac),
        ("Сервер",     SortColumn::Server),
        ("Коментар",   SortColumn::Comment),
    ] {
        header.col(|ui| {
            let indicator = if self.sort_column.as_ref() == Some(&col_val) {
                if self.sort_order == SortOrder::Asc { " ↑" } else { " ↓" }
            } else { "" };
            if ui.add(
                egui::Button::new(egui::RichText::new(
                    format!("{}{}", label, indicator)).heading())
                    .frame(false)
            ).clicked() {
                clicked_col = Some(col_val);
            }
        });
    }
})
.body(|body| { /* без змін */ });
if let Some(col) = clicked_col {
    self.toggle_sort(col);
}
```

### Крок 6 — Pipeline в app.rs

```rust
let filtered: Vec<&Lease> = filter_leases(&self.data.leases, &self.search_query);
let sorted:   Vec<&Lease> = sort_leases(filtered, self.sort_column.as_ref(), &self.sort_order);
// Далі body.rows використовує sorted замість filtered
```

### Крок 7 — Тести toggle_sort в app.rs

```rust
#[test]
fn test_toggle_sort_first_click() { /* sort_column=Ip, order=Asc */ }
#[test]
fn test_toggle_sort_second_click_reverses_order() { /* order=Desc */ }
#[test]
fn test_toggle_sort_third_click_resets() { /* sort_column=None, order=Asc */ }
#[test]
fn test_toggle_sort_different_column_resets_order() { /* Mac, Asc */ }
```

## Порядок виконання

1. Тести sort_leases в mikrotik_data.rs  ->  cargo test (ВПАСТИ)
2. Реалізувати SortColumn/SortOrder/sort_leases  ->  cargo test (пройти)
3. Поля + toggle_sort в app.rs
4. Тести toggle_sort  ->  cargo test (пройти)
5. UI: клікабельні заголовки + pipeline
6. Фінальний cargo test (всі зелені)

## Обсяг змін

Файл	Що змінюється
src/mikrotik_data.rs	+SortColumn, SortOrder, sort_leases, 6 тестів
src/app.rs	+2 поля, toggle_sort, заголовки, pipeline, 4 тести
ssh_client.rs, main.rs, Cargo.toml — без змін.
