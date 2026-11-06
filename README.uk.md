<p align="center"><img src="static/brand/logo-horizontal.svg" width="520" alt="Rowveil — Explore / Query / Understand"></p>

# Rowveil

[English](README.md) · **Українська**

Database explorer & query tool. Окремий десктопний застосунок для перегляду, редагування й керування структурою баз даних.
Бекенд на Rust (axum + sqlx), UI — vanilla JS, вбудований у бінарник. Жодних ORM чи конфігів проєкту не потрібно.

Підтримка: **PostgreSQL**, **MySQL / MariaDB**, **SQLite**, **Redis / Valkey**, **Google BigQuery**.

## Можливості

- Кілька підключень: з env та додані через UI (зберігаються в `connections.json`). Стартова сторінка показує їх із пошуком і фільтром за тегами.
- Нове підключення додається двома способами: окремими полями (тип БД, host, port, user, password, database, SSL) або рядком URL. Спецсимволи в паролі кодуються автоматично.
- Кожне підключення можна відкрити в окремому вікні (у браузерній версії — в окремій вкладці).
- Теги підключень: готові `LOCAL`, `DEV`, `TEST`, `STAGE`, `PROD` або власні (до 8). Кольорові мітки в меню підключень, фільтр за тегами, зміна назви / тегів / read-only пізніше. Підключення з `PROD` має червону лінію над робочою областю і `[PROD]` у заголовку вікна.
- Список таблиць і view, вибір схеми / бази, пошук, оцінка кількості рядків.
- Грід: пагінація, сортування (Shift+клік — за кількома колонками), зміна ширини колонок, приховування колонок.
- Фільтри: `=, <>, >, >=, <, <=, like, ilike, not like, not ilike, in, not in, is null, is not null`.
- Редагування клітинок, додавання і видалення рядків. Зміни накопичуються і зберігаються однією транзакцією (`Ctrl+S`).
- Редактори за типом: enum і boolean — список, `NULL` і `default`.
- JSON (і в колонках JSON, і в звичайному TEXT): редактор CodeMirror з підсвіткою, номерами рядків і згортанням; перемикач Formatted / Raw, Collapse / Expand, перевірка валідності, `Shift+Alt+F` — форматувати. Форматування не змінює числа й порядок ключів, а компактний JSON зберігається компактним.
- Панель рядка: усі поля, перехід за foreign key, зворотні зв'язки ("Referenced by").
- Вкладка Structure: колонки, індекси, foreign keys, зв'язки.
- Режим Schema: підвкладки Schemas, Tables, Views, Enums, Roles, Policies; Copy schema — DDL усієї бази (у буфер або в SQL-консоль).
  - Таблиці: перейменування, перенесення в іншу схему, RLS; додати / змінити / видалити колонку, обмеження, індекс, політику; створити / видалити таблицю.
  - View: редагування визначення, перейменування, створення, видалення, refresh для materialized view.
  - Enum-и, ролі (прапорці, ліміт підключень, пароль, привілеї) і RLS-політики — для PostgreSQL.
  - Кожна зміна показує SQL і виконується лише після Apply. PostgreSQL і SQLite виконують зміни в одній транзакції; MySQL комітить кожну DDL-команду окремо.
  - Діалекти: PostgreSQL — усі підвкладки; MySQL — Schemas (бази), Tables, Views; SQLite — Tables, Views (колонку можна лише перейменувати або видалити).
- SQL-консоль на CodeMirror 6:
  - кнопка ▶ біля кожного statement запускає тільки його; `Ctrl+Enter` — statement під курсором або виділення; `Shift+Ctrl+Enter` — усе;
  - меню Run: «Run all» і «Explain»; кнопка Prettify (`Shift+Alt+F`);
  - гарячі клавіші як у VS Code: `Alt+↑/↓`, `Shift+Alt+↑/↓`, `Ctrl+D`, `Ctrl+Shift+L`, `Ctrl+/`, `Ctrl+Shift+K`, `Ctrl+L`, `Alt+Click`, `Ctrl+F` тощо (повний список — кнопка з клавіатурою в панелі);
  - підсвітка, автокомпліт (таблиці, колонки, alias-и, ключові слова, функції діалекту; `Tab` або `Enter` вставляє підказку), номери рядків, згортання блоків, історія;
  - ПКМ на таблиці: шаблон `SELECT … FROM`, копіювання назви; ПКМ на заголовку колонки: копіювання назви колонки;
  - збережені запити (`Ctrl+S`): для одного підключення або для всіх підключень діалекту, список у сайдбарі;
  - результати таблицею або JSON, кілька результатів, копіювання, експорт CSV / JSON.
- Експорт у CSV / JSON (поточна сторінка або всі відфільтровані рядки, до 10 000).
- Теми: Rowveil Light і Rowveil Dark (за замовчуванням — як у системі), Graphite, Paper, One Dark Pro, Dracula, Nord, GitHub Light (кнопка внизу сайдбару, прев'ю при наведенні).
- Режим read-only. Опційний Basic Auth.

### Redis

- URL: `redis://[user:password@]host:6379/0`, TLS — `rediss://`; `valkey://` теж працює. У формі підключення: host, port, user (ACL, необов'язково), пароль, номер бази, TLS.
- Сайдбар: вибір бази (`db0…db15` з кількістю ключів), фільтр за glob-шаблоном (`user:*`; простий текст шукає входження), фільтр за типом, дерево за `:` або плоский список. Ключі вантажаться через `SCAN` порціями, тож великий Redis не блокується.
- Перегляд і редагування за типом: string (текст або JSON-редактор), hash, list, set, sorted set, stream (записи, consumer groups), JSON-документи модуля RedisJSON. Великі колекції гортаються порціями (`HSCAN`/`SSCAN`/`LRANGE`/`ZRANGE`).
- TTL (зміна або `PERSIST`), перейменування (`RENAMENX`), видалення (`UNLINK`), нові ключі будь-якого типу.
- Консоль команд як redis-cli: відповіді у форматі redis-cli, історія `↑/↓`, кілька команд — по рядку, збережені команди (`Ctrl+S`). У read-only режимі виконуються лише команди читання.

### BigQuery

- URL: `bigquery://PROJECT[/DATASET]?credentials=PATH&location=EU&maximumBytesBilled=N&endpoint=URL`.
- Облікові дані (за порядком): `credentials=` (ключ сервісного акаунта або файл `gcloud auth application-default login`), `GOOGLE_APPLICATION_CREDENTIALS`, файл ADC від gcloud, сервер метаданих на Google Cloud. `endpoint=` — для емулятора (без облікових даних).
- Датасети замість схем; таблиці, view, materialized view; типи `STRUCT` і `ARRAY` показуються як JSON, `TIMESTAMP` — у UTC.
- Перегляд таблиці без фільтрів і сортування читає дані через `tabledata.list`: без query job і без оплати. Фільтри, сортування і консоль запускають query job; заголовок результату показує обсяг processed / billed. `Explain` у консолі робить dry run і показує, скільки байтів обробить запит.
- `maximumBytesBilled` обмежує вартість кожного запиту.
- Редагування рядків — для таблиць з primary key (`NOT ENFORCED`), інакше через DML у консолі. Вкладка Schema показує DDL таблиць і view, Copy schema — DDL усього проєкту; зміни схеми — через консоль.

## Встановлення

Готові збірки — на сторінці [Releases](https://github.com/byht1/rowveil/releases/latest) і на [сайті](https://byht1.github.io/rowveil/#downloads).

| Система | Файл | Як встановити |
|---|---|---|
| Windows 10/11 | `Rowveil-Setup-x64.msi` | Інсталятор: папка, ярлики, «Відкрити за допомогою» для `.db`, сторінка «де що зберігається» |
| Windows | `rowveil.exe` | Portable: один файл без встановлення |
| macOS 11+ | `Rowveil-macOS-universal.dmg` | Перетягніть Rowveil у Applications. Перший запуск: правий клік → Open |
| Debian / Ubuntu | `rowveil-linux-amd64.deb` | `sudo apt install ./rowveil-linux-amd64.deb` |
| Інший Linux | `rowveil-linux-x86_64.tar.gz` | Потрібні WebKitGTK 4.1 і GTK 3 (див. `README.txt` в архіві) |

Де зберігаються дані (`connections.json` — підключення, паролі в URL відкритим текстом; `ui-prefs.json` — тема, вкладки, історія; `window.json`; `rowveil.log`; `webview/`):

| Система | Папка |
|---|---|
| Windows | `%APPDATA%\rowveil\` |
| macOS | `~/Library/Application Support/rowveil/` |
| Linux | `~/.config/rowveil/` |

Видалення програми папку з даними не чіпає.

## Оновлення

Десктопний застосунок сам перевіряє нові версії (під час запуску і кожні 6 годин). Коли вийшла нова версія, у футері сайдбару з'являється кнопка **Update**:

1. Натисніть **Update**: відкриються нотатки релізу.
2. Натисніть **Download and install**. Файл завантажується у фоні, Rowveil перевіряє його SHA-256 за `SHA256SUMS.txt` з релізу.
3. Натисніть **Restart and update**. Rowveil закривається, встановлює оновлення і запускається знову.

| Встановлення | Як оновлюється |
|---|---|
| Windows, MSI | `msiexec /passive`: Windows один раз просить дозволу (UAC). Вибрані компоненти зберігаються |
| Windows, portable `rowveil.exe` | Файл замінюється на місці |
| macOS | `Rowveil.app` замінюється з dmg |
| Linux, tar.gz | Файли `rowveil` і `rowveil-server` замінюються на місці |
| Linux, `.deb`; `rowveil-server` / Docker | Лише сповіщення і посилання на реліз |

Автоматичну перевірку можна вимкнути у вікні оновлень (клік по номеру версії у футері). Дані й підключення під час оновлення не змінюються.

## Десктопний застосунок

- Власне вікно з іконкою Rowveil, без консолі й браузера. Рушій: WebView2 (Windows), WKWebView (macOS), WebKitGTK (Linux).
- Кожне підключення можна відкрити в окремому вікні. Розмір і позиція головного вікна запам'ятовуються.
- Експорт CSV / JSON відкриває системний діалог «Зберегти як»; після збереження — «Show in folder».
- Базу можна передати аргументом: `rowveil C:\data\app.db` або `rowveil postgres://user:pass@host/db`.
- Сервер усередині застосунку слухає лише `127.0.0.1` на випадковому порту й приймає запити тільки від свого вікна (секретний токен на кожен запуск).

### Збірка з вихідного коду

```bash
cargo build --release                       # Windows / macOS
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev && cargo build --release   # Linux
```

Пакети: `packaging/windows/build-msi.ps1` (WiX 5), `packaging/macos/build-dmg.sh`, `packaging/linux/package.sh`. Тег `v*` запускає `.github/workflows/release.yml`: збірка для всіх трьох систем, перевірка інсталяторів і публікація релізу.

## Браузерна версія і Docker

`rowveil-server` — той самий інтерфейс у браузері. Для Docker і для доступу з інших машин.

```bash
./target/release/rowveil-server --url postgres://user:pass@localhost:5432/db
# відкриється http://127.0.0.1:4983
```

```bash
docker build -t rowveil .
docker run -p 4983:4983 -v rowveil-data:/data   -e DATABASE_URL=postgres://user:pass@host.docker.internal:5432/db   --add-host host.docker.internal:host-gateway   rowveil
```

Демо з тестовою базою PostgreSQL:

```bash
docker compose up -d --build
# відкрийте http://localhost:4983
```

## Тестовий стенд

PostgreSQL, MySQL, SQLite, Redis (Redis Stack з модулем JSON) і емулятор BigQuery з даними для перевірки всіх функцій (JSON у тексті, великі числа, enum-и, ролі, RLS, «незручні» назви, 5000 рядків тощо):

```bash
docker compose -f examples/testbed/docker-compose.yml up -d --build
# відкрийте http://localhost:4984
```

Деталі й URL підключень — у [examples/testbed/README.md](examples/testbed/README.md).

## Налаштування

| Змінна / прапорець | Опис | За замовчуванням |
|---|---|---|
| `DATABASE_URL` / `--url` | Основне підключення (у десктопі — аргумент) | — |
| `DATABASE_URL_<NAME>` | Додаткові підключення (назва = `<name>`) | — |
| `DATABASE_TAGS_<NAME>` | Теги цього підключення: `DEV,LOCAL` (`DATABASE_TAGS` — для `DATABASE_URL`). Теги, змінені в UI, зберігаються в `env-connection-tags.json` | — |
| `STUDIO_HOST` / `--host` | Адреса (лише `rowveil-server`) | `127.0.0.1` (Docker: `0.0.0.0`) |
| `STUDIO_PORT` / `--port` | Порт (лише `rowveil-server`) | `4983` |
| `STUDIO_DATA_DIR` / `--data-dir` | Папка для `connections.json` | `%APPDATA%\rowveil` (Docker: `/data`) |
| `STUDIO_NO_OPEN` / `--no-open` | Не відкривати браузер (лише `rowveil-server`) | `false` |
| `STUDIO_READ_ONLY` / `--read-only` | Заборонити зміни даних | `false` |
| `STUDIO_PASSWORD` / `--password` | Увімкнути Basic Auth | — |
| `STUDIO_USER` / `--user` | Логін Basic Auth | `admin` |

Формати URL:

```
postgres://user:pass@host:5432/db
mysql://user:pass@host:3306/db      (або mariadb://)
sqlite://./data.db                  (або шлях до файлу .db / .sqlite)
redis://:pass@host:6379/0           (TLS: rediss://, також valkey://)
bigquery://project/dataset?credentials=C:\key.json&location=EU
```

## Обмеження

- SQLite-файл без права запису для поточного користувача відкривається в режимі read-only. На Windows у панелі таблиці з'являється кнопка «Grant write access»: Windows один раз запитує дозвіл (UAC), і `icacls` дає вашому користувачу право Modify лише на цей файл. Після цього файл можна редагувати без прав адміністратора, також в інших програмах. На Linux / macOS змініть власника або права файлу (`chown` / `chmod`).
- Редагувати можна тільки таблиці з primary key. Таблиці без PK і view відкриваються лише для читання.
- Read-only режим у SQL-консолі пропускає лише запити, що читають дані (`SELECT`, `WITH`, `SHOW`, `EXPLAIN`, `PRAGMA`). Перевірка ґрунтується на ключових словах. Для повного захисту використовуйте користувача БД з правами лише на читання.
- Консоль повертає до 5000 рядків на кожен результат.

## Розробка

```bash
cargo test
cargo run -- examples/testbed/testbed.db                                   # десктоп
cargo run --bin rowveil-server -- --url sqlite://examples/testbed/testbed.db  # браузер
```

Структура: `src/lib.rs` — сервер і API, `src/main.rs` — `rowveil-server`, `src/bin/desktop.rs` — десктопне вікно (tao + wry), `build.rs` — іконка й метадані `.exe`. Без десктопу (Linux / Docker): `cargo build --no-default-features --bin rowveil-server`.

Бренд: `assets/brand/` (кольори, гайд), `assets/rowveil.ico`, `static/brand/`.

У debug-збірці файли зі `static/` читаються з диска, тож зміни UI видно після перезавантаження сторінки.

`static/vendor/codemirror.js` — зібраний бандл CodeMirror 6 і sql-formatter. Для `cargo build` Node.js не потрібен. Щоб оновити бандл:

```bash
cd ui-vendor
npm install
npm run build
```
Тестові дані: `examples/testbed/` (див. вище) і короткі сиди `examples/*-seed.sql`.

## Ліцензія

[MIT](LICENSE)
