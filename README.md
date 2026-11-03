<p align="center"><img src="static/brand/symbol.svg" width="120" alt="Roostbase"></p>

# Roostbase

Database explorer & query tool. Окремий десктопний застосунок для перегляду, редагування й керування структурою баз даних.
Бекенд на Rust (axum + sqlx), UI — vanilla JS, вбудований у бінарник. Жодних ORM чи конфігів проєкту не потрібно.

Підтримка: **PostgreSQL**, **MySQL / MariaDB**, **SQLite**.

## Можливості

- Кілька підключень: з env та додані через UI (зберігаються в `connections.json`).
- Нове підключення додається двома способами: окремими полями (тип БД, host, port, user, password, database, SSL) або рядком URL. Спецсимволи в паролі кодуються автоматично.
- Кожне підключення можна відкрити в окремому вікні (у браузерній версії — в окремій вкладці).
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
  - підсвітка, автокомпліт (таблиці, колонки, alias-и, ключові слова, функції діалекту), номери рядків, згортання блоків, історія;
  - результати таблицею або JSON, кілька результатів, копіювання, експорт CSV / JSON.
- Експорт у CSV / JSON (поточна сторінка або всі відфільтровані рядки, до 10 000).
- Теми: Roostbase (за замовчуванням), Roostbase Light, Graphite, Paper, One Dark Pro, Dracula, Nord, GitHub Light (кнопка внизу сайдбару, прев'ю при наведенні).
- Режим read-only. Опційний Basic Auth.

## Встановлення

Готові збірки — на сторінці [Releases](https://github.com/byht1/roostbase/releases/latest) і на [сайті](https://byht1.github.io/roostbase/#downloads).

| Система | Файл | Як встановити |
|---|---|---|
| Windows 10/11 | `Roostbase-Setup-x64.msi` | Інсталятор: папка, ярлики, «Відкрити за допомогою» для `.db`, сторінка «де що зберігається» |
| Windows | `roostbase.exe` | Portable: один файл без встановлення |
| macOS 11+ | `Roostbase-macOS-universal.dmg` | Перетягніть Roostbase у Applications. Перший запуск: правий клік → Open |
| Debian / Ubuntu | `roostbase-linux-amd64.deb` | `sudo apt install ./roostbase-linux-amd64.deb` |
| Інший Linux | `roostbase-linux-x86_64.tar.gz` | Потрібні WebKitGTK 4.1 і GTK 3 (див. `README.txt` в архіві) |

Де зберігаються дані (`connections.json` — підключення, паролі в URL відкритим текстом; `window.json`; `roostbase.log`; `webview/`):

| Система | Папка |
|---|---|
| Windows | `%APPDATA%\roostbase\` |
| macOS | `~/Library/Application Support/roostbase/` |
| Linux | `~/.config/roostbase/` |

Видалення програми папку з даними не чіпає.

## Десктопний застосунок

- Власне вікно з іконкою Roostbase, без консолі й браузера. Рушій: WebView2 (Windows), WKWebView (macOS), WebKitGTK (Linux).
- Кожне підключення можна відкрити в окремому вікні. Розмір і позиція головного вікна запам'ятовуються.
- Експорт CSV / JSON відкриває системний діалог «Зберегти як»; після збереження — «Show in folder».
- Базу можна передати аргументом: `roostbase C:\data\app.db` або `roostbase postgres://user:pass@host/db`.
- Сервер усередині застосунку слухає лише `127.0.0.1` на випадковому порту й приймає запити тільки від свого вікна (секретний токен на кожен запуск).

### Збірка з вихідного коду

```bash
cargo build --release                       # Windows / macOS
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev && cargo build --release   # Linux
```

Пакети: `packaging/windows/build-msi.ps1` (WiX 5), `packaging/macos/build-dmg.sh`, `packaging/linux/package.sh`. Тег `v*` запускає `.github/workflows/release.yml`: збірка для всіх трьох систем, перевірка інсталяторів і чернетка релізу.

## Браузерна версія і Docker

`roostbase-server` — той самий інтерфейс у браузері. Для Docker і для доступу з інших машин.

```bash
./target/release/roostbase-server --url postgres://user:pass@localhost:5432/db
# відкриється http://127.0.0.1:4983
```

```bash
docker build -t roostbase .
docker run -p 4983:4983 -v roostbase-data:/data   -e DATABASE_URL=postgres://user:pass@host.docker.internal:5432/db   --add-host host.docker.internal:host-gateway   roostbase
```

Демо з тестовою базою PostgreSQL:

```bash
docker compose up -d --build
# відкрийте http://localhost:4983
```

## Тестовий стенд

PostgreSQL, MySQL і SQLite з даними для перевірки всіх функцій (JSON у тексті, великі числа, enum-и, ролі, RLS, «незручні» назви, 5000 рядків тощо):

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
| `STUDIO_HOST` / `--host` | Адреса (лише `roostbase-server`) | `127.0.0.1` (Docker: `0.0.0.0`) |
| `STUDIO_PORT` / `--port` | Порт (лише `roostbase-server`) | `4983` |
| `STUDIO_DATA_DIR` / `--data-dir` | Папка для `connections.json` | `%APPDATA%\roostbase` (Docker: `/data`) |
| `STUDIO_NO_OPEN` / `--no-open` | Не відкривати браузер (лише `roostbase-server`) | `false` |
| `STUDIO_READ_ONLY` / `--read-only` | Заборонити зміни даних | `false` |
| `STUDIO_PASSWORD` / `--password` | Увімкнути Basic Auth | — |
| `STUDIO_USER` / `--user` | Логін Basic Auth | `admin` |

Формати URL:

```
postgres://user:pass@host:5432/db
mysql://user:pass@host:3306/db      (або mariadb://)
sqlite://./data.db                  (або шлях до файлу .db / .sqlite)
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
cargo run --bin roostbase-server -- --url sqlite://examples/testbed/testbed.db  # браузер
```

Структура: `src/lib.rs` — сервер і API, `src/main.rs` — `roostbase-server`, `src/bin/desktop.rs` — десктопне вікно (tao + wry), `build.rs` — іконка й метадані `.exe`. Без десктопу (Linux / Docker): `cargo build --no-default-features --bin roostbase-server`.

Бренд: `assets/brand/` (кольори, гайд), `assets/roostbase.ico`, `static/brand/`.

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
