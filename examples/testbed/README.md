# Тестовий стенд Roostbase

Три бази з однаковою предметною областю (організації, користувачі, замовлення). Дані покривають усі функції Roostbase.

## Запуск

Усе разом — PostgreSQL, MySQL і Roostbase (веб) у Docker:

```bash
docker compose -f examples/testbed/docker-compose.yml up -d --build
# відкрийте http://localhost:4984
```

Тільки бази, для Roostbase, запущеного локально:

```bash
docker compose -f examples/testbed/docker-compose.yml up -d postgres mysql
./target/release/roostbase.exe      # або roostbase-server
```

| База | URL |
|---|---|
| PostgreSQL | `postgres://postgres:postgres@localhost:54329/testbed` |
| PostgreSQL, роль з RLS | `postgres://app_reader:reader@localhost:54329/testbed` |
| MySQL | `mysql://root:root@localhost:13306/testbed` |
| SQLite | `sqlite://examples/testbed/testbed.db` |

Скинути дані до початкового стану:

```bash
docker compose -f examples/testbed/docker-compose.yml down -v   # PostgreSQL і MySQL
python examples/testbed/build-sqlite.py                         # SQLite
```

Порт MySQL `13306`, а не `33069`, бо Windows (Hyper-V) резервує діапазон `32992–33091`.

## Що на чому перевіряти

| Об'єкт | Що перевіряє |
|---|---|
| `users` | enum, масиви, `jsonb`, `inet`, `uuid`, NULL-и, багаторядковий і довгий текст, emoji, кирилиця. Тригер `updated_at`. Індекси: унікальний за виразом, GIN, частковий |
| `profiles.preferences` | **JSON у колонці `TEXT`** (як в `app.db`). Числа більші за 2^53, `1.50`, екранування |
| `profiles.raw_payload` | Відформатований JSON у тексті й текст, схожий на JSON, але невалідний |
| `app_config` | Реальний конфіг: вкладені об'єкти, масиви, `18446744073709551615`, невалідний JSON, звичайний текст |
| `events` | `bigint` біля 2^53 і `9223372036854775807` (MySQL: `18446744073709551615`), `bytea`/`BLOB`, generated-колонка `day` |
| `products` | generated `price_with_vat`, CHECK, `bytea`, `integer[]` |
| `memberships`, `order_items` | Складений первинний ключ (SQLite: `WITHOUT ROWID`) |
| `orders` | Identity-колонка, enum зі статусами, `tstzrange`, `interval`, зовнішній ключ з `ON DELETE RESTRICT` |
| `audit_log` | **Без первинного ключа**: таблиця відкривається лише для читання |
| `"Order Details"` | Пробіли й великі літери в назві, колонки `user`, `order`, `select`, `з пробілом` |
| `wide_metrics` | 29 колонок: горизонтальна прокрутка, приховування колонок |
| `page_views` | 5000 рядків: пагінація, сортування, фільтри |
| `active_users`, `order_totals` | View |
| `analytics.daily_sales` | Materialized view (PostgreSQL) |
| Схеми `billing`, `analytics` | Перемикач схем, Schema → Schemas (MySQL: окрема база `billing`) |
| Enum-и `user_role`, `order_status`, `billing.plan_tier` | Schema → Enums |
| Ролі `app_reader`, `app_writer`, `analyst` | Schema → Roles: прапорці, ліміт підключень, привілеї, членство |
| `billing.invoices` | RLS і три політики (permissive і restrictive). Підключення `app_reader` бачить лише оплачені рахунки не в UAH |
| `events` у SQLite | `STRICT`-таблиця |

Скрипти: `postgres.sql`, `mysql.sql`, `sqlite.sql`. Файл `testbed.db` збирає `build-sqlite.py`.
