CREATE TABLE users (
  id INT AUTO_INCREMENT PRIMARY KEY,
  email VARCHAR(255) NOT NULL UNIQUE,
  name VARCHAR(100) NOT NULL,
  role ENUM('admin','editor','viewer') NOT NULL DEFAULT 'viewer',
  is_active TINYINT(1) NOT NULL DEFAULT 1,
  balance DECIMAL(12,2) DEFAULT 0,
  settings JSON,
  avatar BLOB,
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE posts (
  id BIGINT AUTO_INCREMENT PRIMARY KEY,
  author_id INT NOT NULL,
  title VARCHAR(200) NOT NULL,
  body TEXT,
  views INT UNSIGNED DEFAULT 0,
  CONSTRAINT fk_posts_author FOREIGN KEY (author_id) REFERENCES users(id) ON DELETE CASCADE
);
CREATE VIEW active_users AS SELECT id, email FROM users WHERE is_active = 1;

INSERT INTO users (email, name, role, is_active, balance, settings) VALUES
 ('ann@example.com', 'Ann', 'admin', 1, 120.50, '{"theme":"dark"}'),
 ('bob@example.com', 'Bob', 'editor', 0, 0, NULL),
 ('cid@example.com', 'Cid', 'viewer', 1, 9.99, '{"beta":true}');
INSERT INTO posts (author_id, title, body, views) VALUES (1, 'Hello', 'First', 10), (1, 'Second', NULL, 3), (3, 'Cid post', 'x', 0);
