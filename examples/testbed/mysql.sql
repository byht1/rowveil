-- Roostbase testbed: MySQL 8 / MariaDB 10.5+.
-- Enums, JSON, JSON stored as TEXT, generated columns, CHECK constraints, unsigned big integers,
-- composite keys, a table without a primary key, awkward identifiers, wide and long tables,
-- views, a trigger and a second database.

SET NAMES utf8mb4;
SET SESSION cte_max_recursion_depth = 10000;

CREATE DATABASE IF NOT EXISTS billing CHARACTER SET utf8mb4;

CREATE TABLE organizations (
  id         INT AUTO_INCREMENT PRIMARY KEY,
  name       VARCHAR(120) NOT NULL,
  slug       VARCHAR(60) NOT NULL UNIQUE,
  country    CHAR(2) NOT NULL DEFAULT 'UA',
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE users (
  id         INT AUTO_INCREMENT PRIMARY KEY,
  org_id     INT NULL,
  email      VARCHAR(255) NOT NULL,
  full_name  VARCHAR(200) NOT NULL,
  role       ENUM('owner', 'admin', 'editor', 'viewer') NOT NULL DEFAULT 'viewer',
  is_active  TINYINT(1) NOT NULL DEFAULT 1,
  balance    DECIMAL(12, 2) NOT NULL DEFAULT 0,
  settings   JSON NULL,
  birthday   DATE NULL,
  bio        TEXT NULL,
  created_at DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  UNIQUE KEY users_email_key (email),
  KEY users_org_idx (org_id),
  CONSTRAINT users_org_fk FOREIGN KEY (org_id) REFERENCES organizations (id) ON DELETE SET NULL,
  CONSTRAINT users_balance_check CHECK (balance >= -1000)
);

CREATE TABLE profiles (
  user_id     INT PRIMARY KEY,
  avatar_url  VARCHAR(255) NULL,
  preferences TEXT NULL,          -- compact JSON in a text column
  raw_payload LONGTEXT NULL,
  CONSTRAINT profiles_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE TABLE memberships (
  org_id    INT NOT NULL,
  user_id   INT NOT NULL,
  role      ENUM('owner', 'admin', 'editor', 'viewer') NOT NULL DEFAULT 'viewer',
  joined_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (org_id, user_id),
  CONSTRAINT memberships_org_fk FOREIGN KEY (org_id) REFERENCES organizations (id) ON DELETE CASCADE,
  CONSTRAINT memberships_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE TABLE products (
  id             INT AUTO_INCREMENT PRIMARY KEY,
  sku            VARCHAR(32) NOT NULL UNIQUE,
  title          VARCHAR(200) NOT NULL,
  description    TEXT NULL,
  price          DECIMAL(10, 2) NOT NULL,
  vat_rate       DECIMAL(4, 2) NOT NULL DEFAULT 20.00,
  price_with_vat DECIMAL(12, 2) AS (ROUND(price * (1 + vat_rate / 100), 2)) STORED,
  stock          INT UNSIGNED NOT NULL DEFAULT 0,
  attributes     JSON NULL,
  image          BLOB NULL,
  discontinued   BOOLEAN NOT NULL DEFAULT FALSE,
  CONSTRAINT products_price_check CHECK (price >= 0)
);

CREATE TABLE orders (
  id        BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
  user_id   INT NOT NULL,
  status    ENUM('draft', 'pending', 'paid', 'shipped', 'cancelled', 'refunded') NOT NULL DEFAULT 'draft',
  total     DECIMAL(12, 2) NOT NULL DEFAULT 0,
  currency  CHAR(3) NOT NULL DEFAULT 'UAH',
  note      VARCHAR(255) NULL,
  placed_at DATETIME NOT NULL,
  KEY orders_user_status_idx (user_id, status),
  CONSTRAINT orders_user_fk FOREIGN KEY (user_id) REFERENCES users (id)
);

CREATE TABLE order_items (
  order_id   BIGINT UNSIGNED NOT NULL,
  line_no    SMALLINT NOT NULL,
  product_id INT NOT NULL,
  qty        INT NOT NULL,
  unit_price DECIMAL(10, 2) NOT NULL,
  PRIMARY KEY (order_id, line_no),
  CONSTRAINT order_items_order_fk FOREIGN KEY (order_id) REFERENCES orders (id) ON DELETE CASCADE,
  CONSTRAINT order_items_product_fk FOREIGN KEY (product_id) REFERENCES products (id),
  CONSTRAINT order_items_qty_check CHECK (qty > 0)
);

CREATE TABLE app_config (
  id     INT AUTO_INCREMENT PRIMARY KEY,
  name   VARCHAR(60) NOT NULL UNIQUE,
  params TEXT NOT NULL
);

CREATE TABLE events (
  id          BIGINT UNSIGNED PRIMARY KEY,
  kind        VARCHAR(30) NOT NULL,
  payload     JSON NULL,
  happened_at DATETIME(6) NOT NULL,
  checksum    BINARY(16) NULL,
  trace_id    CHAR(36) NULL
);

CREATE TABLE audit_log (
  at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  actor   VARCHAR(60) NULL,
  action  VARCHAR(20) NOT NULL,
  details JSON NULL
);

CREATE TABLE `Order Details` (
  `ID`                INT AUTO_INCREMENT PRIMARY KEY,
  `user`              VARCHAR(60) NULL,
  `order`             INT NULL,
  `select`            BOOLEAN DEFAULT FALSE,
  `Mixed Case Column` VARCHAR(60) NULL,
  `з пробілом`         VARCHAR(60) NULL
);

CREATE TABLE wide_metrics (
  id INT AUTO_INCREMENT PRIMARY KEY,
  c01 INT, c02 INT, c03 INT, c04 INT, c05 INT, c06 INT, c07 INT, c08 INT,
  c09 DECIMAL(8,2), c10 DECIMAL(8,2), c11 DECIMAL(8,2), c12 DECIMAL(8,2), c13 DECIMAL(8,2), c14 DECIMAL(8,2),
  c15 VARCHAR(60), c16 VARCHAR(60), c17 VARCHAR(60), c18 VARCHAR(60), c19 VARCHAR(60), c20 VARCHAR(60),
  c21 BOOLEAN, c22 BOOLEAN, c23 DATE, c24 DATE, c25 DATETIME, c26 CHAR(36), c27 JSON, c28 TEXT
);

CREATE TABLE page_views (
  id        BIGINT AUTO_INCREMENT PRIMARY KEY,
  user_id   INT NULL,
  path      VARCHAR(200) NOT NULL,
  referrer  VARCHAR(300) NULL,
  country   CHAR(2) NULL,
  ms        INT NOT NULL,
  is_bot    TINYINT(1) NOT NULL DEFAULT 0,
  viewed_at DATETIME NOT NULL,
  KEY page_views_viewed_at_idx (viewed_at)
);

CREATE TABLE billing.invoices (
  id        BIGINT AUTO_INCREMENT PRIMARY KEY,
  org_id    INT NOT NULL,
  amount    DECIMAL(10, 2) NOT NULL,
  currency  CHAR(3) NOT NULL DEFAULT 'USD',
  paid      TINYINT(1) NOT NULL DEFAULT 0,
  issued_on DATE NOT NULL
);

CREATE TRIGGER users_lower_email BEFORE INSERT ON users FOR EACH ROW SET NEW.email = LOWER(NEW.email);

-- ---------------------------------------------------------------- data
INSERT INTO organizations (name, slug, country) VALUES
  ('Acme Corp', 'acme', 'US'), ('Київстар Лаб', 'kyivstar-lab', 'UA'), ('Globex', 'globex', 'DE'),
  ('Initech', 'initech', 'US'), ('Порожня організація', 'empty-org', 'UA');

INSERT INTO users (org_id, email, full_name, role, is_active, balance, settings, birthday, bio, created_at)
WITH RECURSIVE seq (g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 240)
SELECT
  IF(g % 17 = 0, NULL, 1 + g % 4),
  CONCAT('user', g, '@example.com'),
  CONCAT(ELT(1 + g % 6, 'Олена Коваль', 'Taras Shevchenko', 'Ivan Franko', 'Леся Українка', 'John Smith', 'Анна-Марія О''Коннор'), ' #', g),
  ELT(1 + g % 5, 'owner', 'admin', 'editor', 'viewer', 'viewer'),
  g % 7 <> 0,
  ROUND(((g * 37) % 2000 - 300) / 3, 2),
  JSON_OBJECT('theme', ELT(1 + g % 3, 'dark', 'light', 'one-dark-pro'), 'notifications', JSON_OBJECT('email', g % 2 = 0, 'push', g % 3 = 0), 'limits', JSON_ARRAY(g, g * 10)),
  IF(g % 11 = 0, NULL, DATE_ADD('1970-01-01', INTERVAL (g * 97 % 15000) DAY)),
  CASE g % 5 WHEN 0 THEN NULL WHEN 1 THEN 'Short bio' WHEN 2 THEN 'Багаторядкове біо\nдругий рядок'
             WHEN 3 THEN REPEAT('Long text that goes on and on. ', 40) ELSE 'Emoji 🚀 and symbols <>&"'' ✓' END,
  NOW(3) - INTERVAL g DAY
FROM seq;

INSERT INTO profiles (user_id, avatar_url, preferences, raw_payload)
SELECT id,
  IF(id % 3 = 0, NULL, CONCAT('https://i.pravatar.cc/150?u=', id)),
  CONCAT('{"lang":"', ELT(1 + id % 3, 'uk', 'en', 'de'), '","hwid":"', UUID(), '","tier":', id % 4,
         ',"big":1234567890123456789', id % 10, ',"ratio":1.50,"nested":{"diagnostics":{"source":"Tpm","compute_ms":', id * 3, '}}}'),
  IF(id % 2 = 0, '{\n  "pretty": true,\n  "items": [1, 2]\n}', 'not json at all {')
FROM users;

INSERT INTO memberships (org_id, user_id, role) SELECT org_id, id, role FROM users WHERE org_id IS NOT NULL;

INSERT INTO products (sku, title, description, price, vat_rate, stock, attributes, image, discontinued)
WITH RECURSIVE seq (g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 150)
SELECT CONCAT('SKU-', LPAD(g, 5, '0')),
  CONCAT(ELT(1 + g % 6, 'Кава в зернах', 'Ноутбук', 'Keyboard', 'Monitor 27"', 'Чайник', 'Headphones'), ' ', g),
  IF(g % 4 = 0, NULL, CONCAT('Опис товару номер ', g)),
  ROUND(g * 13.37 % 5000, 2), IF(g % 10 = 0, 7.00, 20.00), (g * 7) % 120,
  JSON_OBJECT('color', ELT(1 + g % 3, 'black', 'white', 'silver'), 'weight_g', g * 11),
  IF(g % 5 = 0, UNHEX(MD5(g)), NULL), g % 13 = 0
FROM seq;

INSERT INTO orders (user_id, status, currency, note, placed_at)
WITH RECURSIVE seq (g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 600)
SELECT 1 + g % 240, ELT(1 + g % 7, 'draft', 'pending', 'paid', 'paid', 'shipped', 'cancelled', 'refunded'),
  ELT(1 + g % 3, 'UAH', 'USD', 'EUR'), IF(g % 6 = 0, 'Залишити біля дверей', NULL),
  NOW() - INTERVAL ((g * 37) % 8000) HOUR
FROM seq;

INSERT INTO order_items (order_id, line_no, product_id, qty, unit_price)
WITH RECURSIVE l (n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM l WHERE n < 3)
SELECT o.id, l.n, p.id, 1 + (o.id + l.n) % 4, p.price
FROM orders o JOIN l ON l.n <= 1 + o.id % 3
JOIN products p ON p.id = 1 + (o.id * 7 + l.n * 13) % 150;

UPDATE orders o JOIN (SELECT order_id, SUM(qty * unit_price) AS s FROM order_items GROUP BY order_id) t ON t.order_id = o.id
SET o.total = t.s;

INSERT INTO app_config (name, params) VALUES
  ('install', '{"hwid":"8c1e7a52-3f0d-4b9e-a6d2-5b71c0e94f3a","tier":1,"tier_tag":"t1.demo","created_at":"2026-10-16T11:12:18.6935212Z","diagnostics":{"key_source":"Tpm","compute_ms":64}}'),
  ('limits', '{"maxAttempts":30,"bigId":18446744073709551615,"price":1.50}'),
  ('features', '[{"id":"export","isEnabled":true},{"id":"sync","isEnabled":false}]'),
  ('broken', '{"this is": "not valid json",}'),
  ('plain', 'just a text value');

INSERT INTO events (id, kind, payload, happened_at, checksum, trace_id)
WITH RECURSIVE seq (g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 400)
SELECT IF(g <= 5, 9007199254740990 + g, g), ELT(1 + g % 5, 'login', 'logout', 'purchase', 'error', 'sync'),
  IF(g % 4 = 0, NULL, JSON_OBJECT('n', g, 'ok', g % 2 = 0)), NOW(6) - INTERVAL ((g * 13) % 10000) MINUTE,
  UNHEX(MD5(CONCAT('event', g))), UUID()
FROM seq;
INSERT INTO events (id, kind, happened_at) VALUES (18446744073709551615, 'max-unsigned-bigint', NOW(6));

INSERT INTO audit_log (at, actor, action, details)
WITH RECURSIVE seq (g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 50)
SELECT NOW() - INTERVAL g MINUTE, IF(g % 3 = 0, NULL, CONCAT('user', g)), ELT(1 + g % 3, 'create', 'update', 'delete'), JSON_OBJECT('row', g)
FROM seq;

INSERT INTO `Order Details` (`user`, `order`, `select`, `Mixed Case Column`, `з пробілом`)
WITH RECURSIVE seq (g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 25)
SELECT CONCAT('user', g), g * 10, g % 2 = 0, CONCAT('Value ', g), CONCAT('Значення ', g) FROM seq;

INSERT INTO wide_metrics (c01, c02, c03, c04, c05, c06, c07, c08, c09, c10, c11, c12, c13, c14,
                          c15, c16, c17, c18, c19, c20, c21, c22, c23, c24, c25, c26, c27, c28)
WITH RECURSIVE seq (g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 60)
SELECT g, g * 2, g * 3, g * 4, g * 5, g * 6, g * 7, g * 8, g / 3, g / 7, g * 1.5, g * 2.5, g * 0.1, g * 0.01,
  CONCAT('text ', g), MD5(g), REPEAT('x', g % 40), CONCAT('Рядок ', g), NULL, 'last',
  g % 2 = 0, g % 3 = 0, CURDATE() - INTERVAL g DAY, CURDATE() + INTERVAL g DAY, NOW() - INTERVAL g HOUR,
  UUID(), JSON_OBJECT('g', g), CONCAT('a', g, ',b', g)
FROM seq;

INSERT INTO page_views (user_id, path, referrer, country, ms, is_bot, viewed_at)
WITH RECURSIVE seq (g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 5000)
SELECT IF(g % 4 = 0, NULL, 1 + g % 240), ELT(1 + g % 6, '/', '/pricing', '/docs', '/blog/roostbase', '/login', '/товари'),
  IF(g % 3 = 0, CONCAT('https://google.com/search?q=', g), NULL), ELT(1 + g % 5, 'UA', 'US', 'DE', 'PL', NULL),
  20 + (g * 31) % 4000, g % 19 = 0, NOW() - INTERVAL ((g * 7) % 43200) MINUTE
FROM seq;

INSERT INTO billing.invoices (org_id, amount, currency, paid, issued_on)
WITH RECURSIVE seq (g) AS (SELECT 1 UNION ALL SELECT g + 1 FROM seq WHERE g < 80)
SELECT 1 + g % 4, ROUND(g * 41.5 % 9000 + 10, 2), ELT(1 + g % 3, 'USD', 'EUR', 'UAH'), g % 3 <> 0, CURDATE() - INTERVAL (g * 3) DAY
FROM seq;

-- ---------------------------------------------------------------- views
CREATE VIEW active_users AS SELECT id, email, full_name, role, org_id FROM users WHERE is_active;
CREATE VIEW order_totals AS
  SELECT u.id AS user_id, u.full_name, COUNT(o.id) AS orders, COALESCE(SUM(o.total), 0) AS spent
  FROM users u LEFT JOIN orders o ON o.user_id = u.id AND o.status IN ('paid', 'shipped')
  GROUP BY u.id, u.full_name;
