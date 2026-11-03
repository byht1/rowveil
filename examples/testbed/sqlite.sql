-- QueryNest testbed: SQLite 3.37+.
-- Type affinities, booleans as 0/1, JSON stored as TEXT, BLOBs, big integers, composite keys,
-- WITHOUT ROWID and STRICT tables, a generated column, a table without a declared primary key,
-- awkward identifiers, wide and long tables, views, indexes and a trigger.

PRAGMA foreign_keys = ON;

CREATE TABLE organizations (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL,
  slug       TEXT NOT NULL UNIQUE,
  country    TEXT NOT NULL DEFAULT 'UA',
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE users (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  org_id     INTEGER REFERENCES organizations(id) ON DELETE SET NULL,
  email      TEXT NOT NULL,
  full_name  TEXT NOT NULL,
  role       TEXT NOT NULL DEFAULT 'viewer' CHECK (role IN ('owner', 'admin', 'editor', 'viewer')),
  is_active  BOOLEAN NOT NULL DEFAULT 1,
  balance    NUMERIC NOT NULL DEFAULT 0,
  settings   JSON,
  birthday   DATE,
  bio        TEXT,
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at DATETIME
);
CREATE UNIQUE INDEX users_email_lower_key ON users (lower(email));
CREATE INDEX users_active_idx ON users (org_id) WHERE is_active = 1;

CREATE TABLE profiles (
  user_id     INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  avatar_url  TEXT,
  preferences TEXT,
  raw_payload TEXT
);

CREATE TABLE memberships (
  org_id    INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  user_id   INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role      TEXT NOT NULL DEFAULT 'viewer',
  joined_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (org_id, user_id)
) WITHOUT ROWID;

CREATE TABLE products (
  id             INTEGER PRIMARY KEY,
  sku            TEXT NOT NULL UNIQUE,
  title          TEXT NOT NULL,
  description    TEXT,
  price          REAL NOT NULL CHECK (price >= 0),
  vat_rate       REAL NOT NULL DEFAULT 20.0,
  price_with_vat REAL GENERATED ALWAYS AS (round(price * (1 + vat_rate / 100), 2)) STORED,
  stock          INTEGER NOT NULL DEFAULT 0,
  attributes     JSON,
  image          BLOB,
  discontinued   BOOLEAN NOT NULL DEFAULT 0
);

CREATE TABLE orders (
  id        INTEGER PRIMARY KEY,
  user_id   INTEGER NOT NULL REFERENCES users(id),
  status    TEXT NOT NULL DEFAULT 'draft',
  total     NUMERIC NOT NULL DEFAULT 0,
  currency  TEXT NOT NULL DEFAULT 'UAH',
  note      TEXT,
  placed_at DATETIME NOT NULL
);
CREATE INDEX orders_user_status_idx ON orders (user_id, status);

CREATE TABLE order_items (
  order_id   INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  line_no    INTEGER NOT NULL,
  product_id INTEGER NOT NULL REFERENCES products(id),
  qty        INTEGER NOT NULL CHECK (qty > 0),
  unit_price REAL NOT NULL,
  PRIMARY KEY (order_id, line_no)
);

CREATE TABLE app_config (
  id     INTEGER PRIMARY KEY,
  name   TEXT NOT NULL UNIQUE,
  params TEXT NOT NULL
);

-- STRICT table with big integers
CREATE TABLE events (
  id          INTEGER PRIMARY KEY,
  kind        TEXT NOT NULL,
  payload     TEXT,
  happened_at TEXT NOT NULL,
  checksum    BLOB
) STRICT;

-- no declared primary key: rows are read-only in the grid
CREATE TABLE audit_log (
  at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  actor   TEXT,
  action  TEXT NOT NULL,
  details JSON
);

CREATE TABLE "Order Details" (
  "ID"                INTEGER PRIMARY KEY,
  "user"              TEXT,
  "order"             INTEGER,
  "select"            BOOLEAN DEFAULT 0,
  "Mixed Case Column" TEXT,
  "з пробілом"        TEXT
);

CREATE TABLE wide_metrics (
  id INTEGER PRIMARY KEY,
  c01 INTEGER, c02 INTEGER, c03 INTEGER, c04 INTEGER, c05 INTEGER, c06 INTEGER, c07 INTEGER, c08 INTEGER,
  c09 REAL, c10 REAL, c11 REAL, c12 REAL, c13 REAL, c14 REAL,
  c15 TEXT, c16 TEXT, c17 TEXT, c18 TEXT, c19 TEXT, c20 TEXT,
  c21 BOOLEAN, c22 BOOLEAN, c23 DATE, c24 DATE, c25 DATETIME, c26 TEXT, c27 JSON, c28 TEXT
);

CREATE TABLE page_views (
  id        INTEGER PRIMARY KEY,
  user_id   INTEGER REFERENCES users(id) ON DELETE SET NULL,
  path      TEXT NOT NULL,
  referrer  TEXT,
  country   TEXT,
  ms        INTEGER NOT NULL,
  is_bot    BOOLEAN NOT NULL DEFAULT 0,
  viewed_at DATETIME NOT NULL
);
CREATE INDEX page_views_viewed_at_idx ON page_views (viewed_at DESC);

CREATE TRIGGER users_set_updated_at AFTER UPDATE ON users FOR EACH ROW WHEN NEW.updated_at IS OLD.updated_at
BEGIN
  UPDATE users SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

-- ---------------------------------------------------------------- data
INSERT INTO organizations (name, slug, country) VALUES
  ('Acme Corp', 'acme', 'US'), ('Київстар Лаб', 'kyivstar-lab', 'UA'), ('Globex', 'globex', 'DE'),
  ('Initech', 'initech', 'US'), ('Порожня організація', 'empty-org', 'UA');

WITH RECURSIVE seq(g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 240)
INSERT INTO users (org_id, email, full_name, role, is_active, balance, settings, birthday, bio, created_at)
SELECT
  CASE WHEN g % 17 = 0 THEN NULL ELSE 1 + g % 4 END,
  'user' || g || '@example.com',
  CASE g % 6 WHEN 0 THEN 'Олена Коваль' WHEN 1 THEN 'Taras Shevchenko' WHEN 2 THEN 'Ivan Franko'
             WHEN 3 THEN 'Леся Українка' WHEN 4 THEN 'John Smith' ELSE 'Анна-Марія О''Коннор' END || ' #' || g,
  CASE g % 5 WHEN 0 THEN 'owner' WHEN 1 THEN 'admin' WHEN 2 THEN 'editor' ELSE 'viewer' END,
  g % 7 <> 0,
  round(((g * 37) % 2000 - 300) / 3.0, 2),
  json_object('theme', CASE g % 3 WHEN 0 THEN 'dark' WHEN 1 THEN 'light' ELSE 'one-dark-pro' END,
              'notifications', json_object('email', json(CASE WHEN g % 2 = 0 THEN 'true' ELSE 'false' END)),
              'limits', json_array(g, g * 10)),
  CASE WHEN g % 11 = 0 THEN NULL ELSE date('1970-01-01', '+' || (g * 97 % 15000) || ' days') END,
  CASE g % 5 WHEN 0 THEN NULL WHEN 1 THEN 'Short bio' WHEN 2 THEN 'Багаторядкове біо' || char(10) || 'другий рядок'
             WHEN 3 THEN replace(hex(zeroblob(40)), '00', 'Long text. ') ELSE 'Emoji 🚀 and symbols <>&"'' ✓' END,
  datetime('now', '-' || g || ' days')
FROM seq;

INSERT INTO profiles (user_id, avatar_url, preferences, raw_payload)
SELECT id,
  CASE WHEN id % 3 = 0 THEN NULL ELSE 'https://i.pravatar.cc/150?u=' || id END,
  '{"lang":"' || (CASE id % 3 WHEN 0 THEN 'uk' WHEN 1 THEN 'en' ELSE 'de' END) || '","hwid":"' || lower(hex(randomblob(16))) ||
    '","tier":' || (id % 4) || ',"big":1234567890123456789' || (id % 10) ||
    ',"ratio":1.50,"flags":["a","b"],"empty":{},"nested":{"diagnostics":{"source":"Tpm","compute_ms":' || (id * 3) || '}},"escaped":"say \"hi\", ok"}',
  CASE WHEN id % 2 = 0 THEN '{' || char(10) || '  "pretty": true,' || char(10) || '  "items": [1, 2]' || char(10) || '}' ELSE 'not json at all {' END
FROM users;

INSERT INTO memberships (org_id, user_id, role) SELECT org_id, id, role FROM users WHERE org_id IS NOT NULL;

WITH RECURSIVE seq(g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 150)
INSERT INTO products (sku, title, description, price, vat_rate, stock, attributes, image, discontinued)
SELECT 'SKU-' || substr('00000' || g, -5),
  CASE g % 6 WHEN 0 THEN 'Кава в зернах' WHEN 1 THEN 'Ноутбук' WHEN 2 THEN 'Keyboard' WHEN 3 THEN 'Monitor 27"' WHEN 4 THEN 'Чайник' ELSE 'Headphones' END || ' ' || g,
  CASE WHEN g % 4 = 0 THEN NULL ELSE 'Опис товару номер ' || g END,
  round((g * 1337 % 500000) / 100.0, 2),
  CASE WHEN g % 10 = 0 THEN 7.0 ELSE 20.0 END,
  (g * 7) % 120,
  json_object('color', CASE g % 3 WHEN 0 THEN 'black' WHEN 1 THEN 'white' ELSE 'silver' END, 'weight_g', g * 11),
  CASE WHEN g % 5 = 0 THEN randomblob(16) END,
  g % 13 = 0
FROM seq;

WITH RECURSIVE seq(g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 600)
INSERT INTO orders (user_id, status, currency, note, placed_at)
SELECT 1 + g % 240,
  CASE g % 7 WHEN 0 THEN 'draft' WHEN 1 THEN 'pending' WHEN 2 THEN 'paid' WHEN 3 THEN 'paid' WHEN 4 THEN 'shipped' WHEN 5 THEN 'cancelled' ELSE 'refunded' END,
  CASE g % 3 WHEN 0 THEN 'UAH' WHEN 1 THEN 'USD' ELSE 'EUR' END,
  CASE WHEN g % 6 = 0 THEN 'Залишити біля дверей' END,
  datetime('now', '-' || ((g * 37) % 8000) || ' hours')
FROM seq;

WITH RECURSIVE l(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM l WHERE n < 3)
INSERT INTO order_items (order_id, line_no, product_id, qty, unit_price)
SELECT o.id, l.n, p.id, 1 + (o.id + l.n) % 4, p.price
FROM orders o JOIN l ON l.n <= 1 + o.id % 3
JOIN products p ON p.id = 1 + (o.id * 7 + l.n * 13) % 150;

UPDATE orders SET total = (SELECT round(sum(qty * unit_price), 2) FROM order_items WHERE order_id = orders.id);

INSERT INTO app_config (name, params) VALUES
  ('install', '{"hwid":"8c1e7a52-3f0d-4b9e-a6d2-5b71c0e94f3a","tier":1,"tier_tag":"t1.demo","source_value_digest":"4be1f0c2d97a63e8b5f1c0a9d2e74b6f8a1c3e5d7f9b2a4c6e8d0f1a3b5c7d9e","created_at":"2026-10-16T11:12:18.6935212Z","consecutive_read_failures":0,"last_success_at":"2026-11-02T14:56:20.491383+02:00","diagnostics":{"key_source":"Tpm","compute_ms":64}}'),
  ('limits', '{"maxAttempts":30,"attemptsCount":0,"bigId":18446744073709551615,"price":1.50,"ratio":1e-7}'),
  ('features', '[{"id":"export","isEnabled":true},{"id":"sync","isEnabled":false,"rules":[]}]'),
  ('broken', '{"this is": "not valid json",}'),
  ('plain', 'just a text value');

WITH RECURSIVE seq(g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 400)
INSERT INTO events (id, kind, payload, happened_at, checksum)
SELECT CASE WHEN g <= 5 THEN 9007199254740990 + g ELSE g END,
  CASE g % 5 WHEN 0 THEN 'login' WHEN 1 THEN 'logout' WHEN 2 THEN 'purchase' WHEN 3 THEN 'error' ELSE 'sync' END,
  CASE WHEN g % 4 = 0 THEN NULL ELSE json_object('n', g, 'ok', json(CASE WHEN g % 2 = 0 THEN 'true' ELSE 'false' END)) END,
  strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-' || ((g * 13) % 10000) || ' minutes'),
  randomblob(16)
FROM seq;
INSERT INTO events (id, kind, happened_at) VALUES (9223372036854775807, 'max-int64', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

WITH RECURSIVE seq(g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 50)
INSERT INTO audit_log (at, actor, action, details)
SELECT datetime('now', '-' || g || ' minutes'), CASE WHEN g % 3 = 0 THEN NULL ELSE 'user' || g END,
  CASE g % 3 WHEN 0 THEN 'create' WHEN 1 THEN 'update' ELSE 'delete' END, json_object('row', g)
FROM seq;

WITH RECURSIVE seq(g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 25)
INSERT INTO "Order Details" ("user", "order", "select", "Mixed Case Column", "з пробілом")
SELECT 'user' || g, g * 10, g % 2 = 0, 'Value ' || g, 'Значення ' || g FROM seq;

WITH RECURSIVE seq(g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 60)
INSERT INTO wide_metrics (c01, c02, c03, c04, c05, c06, c07, c08, c09, c10, c11, c12, c13, c14,
                          c15, c16, c17, c18, c19, c20, c21, c22, c23, c24, c25, c26, c27, c28)
SELECT g, g * 2, g * 3, g * 4, g * 5, g * 6, g * 7, g * 8, g / 3.0, g / 7.0, g * 1.5, g * 2.5, g * 0.1, g * 0.01,
  'text ' || g, lower(hex(randomblob(8))), substr('xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx', 1, g % 40), 'Рядок ' || g, NULL, 'last',
  g % 2 = 0, g % 3 = 0, date('now', '-' || g || ' days'), date('now', '+' || g || ' days'), datetime('now', '-' || g || ' hours'),
  lower(hex(randomblob(16))), json_object('g', g), 'a' || g || ',b' || g
FROM seq;

WITH RECURSIVE seq(g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 5000)
INSERT INTO page_views (user_id, path, referrer, country, ms, is_bot, viewed_at)
SELECT CASE WHEN g % 4 = 0 THEN NULL ELSE 1 + g % 240 END,
  CASE g % 6 WHEN 0 THEN '/' WHEN 1 THEN '/pricing' WHEN 2 THEN '/docs' WHEN 3 THEN '/blog/querynest' WHEN 4 THEN '/login' ELSE '/товари' END,
  CASE WHEN g % 3 = 0 THEN 'https://google.com/search?q=' || g END,
  CASE g % 5 WHEN 0 THEN 'UA' WHEN 1 THEN 'US' WHEN 2 THEN 'DE' WHEN 3 THEN 'PL' END,
  20 + (g * 31) % 4000, g % 19 = 0,
  datetime('now', '-' || ((g * 7) % 43200) || ' minutes')
FROM seq;

-- ---------------------------------------------------------------- views
CREATE VIEW active_users AS SELECT id, email, full_name, role, org_id FROM users WHERE is_active = 1;
CREATE VIEW order_totals AS
  SELECT u.id AS user_id, u.full_name, count(o.id) AS orders, coalesce(sum(o.total), 0) AS spent
  FROM users u LEFT JOIN orders o ON o.user_id = u.id AND o.status IN ('paid', 'shipped')
  GROUP BY u.id, u.full_name;
