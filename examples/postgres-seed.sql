CREATE TYPE user_role AS ENUM ('admin', 'editor', 'viewer');

CREATE TABLE users (
  id serial PRIMARY KEY,
  email varchar(255) NOT NULL UNIQUE,
  name text NOT NULL,
  role user_role NOT NULL DEFAULT 'viewer',
  is_active boolean NOT NULL DEFAULT true,
  balance numeric(12, 2) DEFAULT 0,
  tags text[] DEFAULT '{}',
  settings jsonb DEFAULT '{}'::jsonb,
  external_id uuid DEFAULT gen_random_uuid(),
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE posts (
  id integer GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  author_id integer NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  title text NOT NULL,
  body text,
  published boolean DEFAULT false,
  views bigint DEFAULT 0,
  created_at timestamp DEFAULT now()
);
CREATE INDEX posts_author_idx ON posts(author_id);

CREATE TABLE tags (id serial PRIMARY KEY, label text NOT NULL UNIQUE);
CREATE TABLE post_tags (
  post_id integer REFERENCES posts(id) ON DELETE CASCADE,
  tag_id integer REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (post_id, tag_id)
);
CREATE TABLE audit_log (at timestamptz DEFAULT now(), message text);

CREATE VIEW active_users AS SELECT id, email, name FROM users WHERE is_active;

CREATE SCHEMA billing;
CREATE TABLE billing.invoices (
  id bigserial PRIMARY KEY,
  user_id integer REFERENCES public.users(id),
  amount numeric(10,2) NOT NULL,
  issued_on date DEFAULT current_date
);

INSERT INTO users (email, name, role, is_active, balance, tags, settings)
SELECT 'user' || g || '@example.com', 'User ' || g,
       (ARRAY['admin','editor','viewer'])[1 + g % 3]::user_role,
       g % 5 <> 0, round((random() * 1000)::numeric, 2),
       ARRAY['tag' || (g % 4), 'group' || (g % 2)],
       jsonb_build_object('theme', CASE WHEN g % 2 = 0 THEN 'dark' ELSE 'light' END, 'notifications', g % 3 = 0)
FROM generate_series(1, 120) g;

INSERT INTO posts (author_id, title, body, published, views)
SELECT 1 + (g % 120), 'Post #' || g, 'Body of post ' || g || E'\nwith a second line', g % 2 = 0, g * 17
FROM generate_series(1, 300) g;

INSERT INTO tags (label) VALUES ('rust'), ('sql'), ('web'), ('db');
INSERT INTO post_tags SELECT g, 1 + g % 4 FROM generate_series(1, 100) g;
INSERT INTO audit_log (message) VALUES ('seeded'), ('no primary key here');
INSERT INTO billing.invoices (user_id, amount) SELECT 1 + g % 50, g * 9.99 FROM generate_series(1, 40) g;
