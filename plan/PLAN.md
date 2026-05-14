# План: Поле пошуку для таблиці лізів

## Опис задачі

Додати рядок пошуку над таблицею лізів, який фільтрує відображені рядки за збігом
хоча б в одному полі: IP-адреса, MAC-адреса, сервер, коментар. Поруч із полем пошуку
розмістити кнопку «✕» для швидкого скидання пошукового запиту.
Методологія — TDD: спочатку тести, потім реалізація.

---

## Запитання перед початком

1. **Чутливість до регістру.** Пошук планується **нечутливим** до регістру
   (`"syn"` знаходить `"029SYN"`). Підтвердьте або вкажіть інше.
   Відповідь: так, пошук нечутливий до регістру.

2. **Розміщення рядка пошуку.** Варіанти:
   - **А** — між рядком кнопок («Оновити», «Додати адресу») і таблицею *(рекомендовано)*
   - **Б** — в одному горизонтальному рядку з кнопками
   Відповідь: варіант А.

3. **Лічильник знайдених записів.** Чи потрібно показувати, наприклад,
   `"Знайдено: 3 / 25"` при активному пошуку? Якщо так — де саме
   (у рядку статусу чи поруч із полем пошуку)?
   Відповідь: ні лічильник не потрібен.

---

## Запропоновані зміни

### Крок 1 — Тести (TDD: написати тести, що падають)

**Файл:** `src/mikrotik_data.rs`

Додати тести до блоку `#[cfg(test)]` для нової функції `filter_leases`:

| Назва тесту | Сценарій |
|---|---|
| `test_filter_leases_empty_query` | Порожній запит → повертає всі записи |
| `test_filter_leases_by_ip` | Пошук за частиною IP (`"172.22"`) |
| `test_filter_leases_by_mac` | Пошук за MAC-адресою |
| `test_filter_leases_by_server` | Пошук за назвою сервера |
| `test_filter_leases_by_comment` | Пошук за коментарем |
| `test_filter_leases_no_match` | Немає збігів → порожній вектор |
| `test_filter_leases_case_insensitive` | `"SYN"` знаходить `"029SYN"` |

---

### Крок 2 — Реалізація `filter_leases` в `mikrotik_data.rs`

Додати публічну чисту функцію перед блоком `#[cfg(test)]`:

```rust
/// Повертає підмножину лізів, у яких хоча б одне поле
/// містить рядок `query` (нечутливо до регістру).
/// Порожній `query` → повертає всі ліза.
pub fn filter_leases<'a>(leases: &'a [Lease], query: &str) -> Vec<&'a Lease> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return leases.iter().collect();
    }
    leases
        .iter()
        .filter(|l| {
            l.address.as_deref().unwrap_or("").to_lowercase().contains(&q)
                || l.mac_address.to_lowercase().contains(&q)
                || l.server.to_lowercase().contains(&q)
                || l.comment.as_deref().unwrap_or("").to_lowercase().contains(&q)
        })
        .collect()
}
```

Перевіряються поля: `address`, `mac_address`, `server`, `comment`.

---

### Крок 3 — Стан UI в `app.rs`

**3а. Імпорт функції** (у рядку `use crate::mikrotik_data::{...}`):
```rust
filter_leases,
```

**3б. Нове поле** у структурі `WhitelistApp`:
```rust
#[serde(skip)]
search_query: String,
```

**3в. Ініціалізація** у `Default::default()`:
```rust
search_query: String::new(),
```

---

### Крок 4 — UI: рядок пошуку в `app.rs`

Між рядком кнопок і таблицею додати горизонтальний блок:

```rust
// Рядок пошуку
ui.horizontal(|ui| {
    ui.label("🔍 Пошук:");
    ui.add(
        egui::TextEdit::singleline(&mut self.search_query)
            .hint_text("IP, MAC, сервер, коментар...")
            .desired_width(300.0),
    );
    if ui.button("✕").on_hover_text("Скинути пошук").clicked() {
        self.search_query.clear();
    }
});
ui.add_space(5.0);
```

---

### Крок 5 — Фільтрація у тілі таблиці в `app.rs`

Перед рендерингом таблиці обчислити відфільтрований список:

```rust
let filtered: Vec<&Lease> = filter_leases(&self.data.leases, &self.search_query);
```

Замінити у `body.rows(...)`:
- `self.data.leases.len()` → `filtered.len()`
- `self.data.leases[row_index].clone()` → `filtered[row_index].clone()`

> Нумерація рядків `№` буде за порядком у відфільтрованому списку.
> Якщо потрібна глобальна нумерація (з оригінального списку) — уточніть.

---

### Крок 6 — Тести для UI в `app.rs`

Додати до блоку `#[cfg(test)]`:

```rust
#[test]
fn test_search_query_initial_state() {
    let app = WhitelistApp::default();
    assert_eq!(app.search_query, "");
}

#[test]
fn test_filter_applied_to_leases() {
    let mut app = WhitelistApp::default();
    let mut responses = HashMap::new();
    responses.insert(
        "/ip/dhcp-server/export".to_string(),
        MIKROTIK_EXPORT.to_string(),
    );
    app.client = Some(Box::new(MockSSHClient { responses }));
    app.refresh_data();

    app.search_query = "corp-dhcp".to_string();
    let filtered = filter_leases(&app.data.leases, &app.search_query);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].server, "corp-dhcp");
}
```

---

## Порядок виконання

```
1. Написати тести в mikrotik_data.rs  →  cargo test (має ВПАСТИ)
2. Реалізувати filter_leases           →  cargo test (має пройти)
3. Додати поле search_query в app.rs
4. Додати UI рядка пошуку
5. Підключити filter_leases до таблиці
6. Написати тести в app.rs             →  cargo test (має пройти)
7. Фінальний cargo test (всі тести зелені)
```

---

## Обсяг змін

| Файл | Що змінюється |
|---|---|
| `src/mikrotik_data.rs` | +1 публічна функція `filter_leases` + 7 тестів |
| `src/app.rs` | +1 поле `search_query`, UI рядка пошуку, підключення фільтра до таблиці, +2 тести |

Файли `ssh_client.rs`, `main.rs`, `Cargo.toml` — **без змін**.
