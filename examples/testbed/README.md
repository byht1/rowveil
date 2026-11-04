# Тестовий стенд Rowveil

Три SQL-бази з однаковою предметною областю (організації, користувачі, замовлення), Redis і емулятор BigQuery. Дані покривають усі функції Rowveil.

## Запуск

Усе разом — PostgreSQL, MySQL, Redis, BigQuery і Rowveil (веб) у Docker:

```bash
docker compose -f examples/testbed/docker-compose.yml up -d --build
# відкрийте http://localhost:4984
```

Тільки бази, для Rowveil, запущеного локально:

```bash
docker compose -f examples/testbed/docker-compose.yml up -d postgres mysql redis redis-seed bigquery
./target/release/rowveil.exe      # або rowveil-server
```

| База | URL |
|---|---|
| PostgreSQL | `postgres://postgres:postgres@localhost:54329/testbed` |
| PostgreSQL, роль з RLS | `postgres://app_reader:reader@localhost:54329/testbed` |
| MySQL | `mysql://root:root@localhost:13306/testbed` |
| SQLite | `sqlite://examples/testbed/testbed.db` |
| Redis | `redis://localhost:16379/0` |
| BigQuery (емулятор) | `bigquery://testbed?endpoint=http://localhost:19050` |

Скинути дані до початкового стану:

```bash
docker compose -f examples/testbed/docker-compose.yml down -v   # PostgreSQL і MySQL
python examples/testbed/build-sqlite.py                         # SQLite
docker compose -f examples/testbed/docker-compose.yml run --rm redis-seed   # Redis
docker compose -f examples/testbed/docker-compose.yml restart bigquery      # BigQuery (дані в пам'яті)
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

### Redis (`redis.txt`)

| Ключі | Що перевіряє |
|---|---|
| `app:*` | Рядки: звичайний, JSON (`app:config`), багаторядковий |
| `binary:blob` | Бінарне значення: показ `\xHH`, лише читання |
| `cache:homepage`, `session:short`, `session:*` | TTL; 300 ключів `session:*` — дерево і порції `SCAN` |
| `user:1..3`, `order:*` (120) | Hash; великі папки в дереві |
| `queue:emails`, `log:recent` | List: push у голову / хвіст, редагування й видалення за індексом |
| `tags:post:*`, `online:users` | Set |
| `leaderboard`, `trending:hourly` | Sorted set, порядок за score |
| `events:orders` | Stream і consumer group `billing` |
| `product:1`, `product:2` | Документи RedisJSON |
| `visitors:today` | HyperLogLog (рядок) |
| база `1` | Перемикач баз: `other:db`, `other:settings` |

### BigQuery (`bigquery.yaml`, проєкт `testbed`)

| Таблиця | Що перевіряє |
|---|---|
| `shop.customers` | `BOOL`, `TIMESTAMP`, `ARRAY<STRING>`, `STRUCT` |
| `shop.orders` | `NUMERIC`, `FLOAT64`, `DATE`, `JSON` |
| `analytics.page_views` | Другий датасет, агрегування в консолі |

Емулятор не повертає `mode` колонок і `numDmlAffectedRows`, тож `REQUIRED` і кількість змінених рядків видно лише в справжньому BigQuery.

Скрипти: `postgres.sql`, `mysql.sql`, `sqlite.sql`. Файл `testbed.db` збирає `build-sqlite.py`.
