PRAGMA foreign_keys = ON;
CREATE TABLE authors (id INTEGER PRIMARY KEY, name TEXT NOT NULL, verified BOOLEAN DEFAULT 0, meta JSON, created_at DATETIME DEFAULT CURRENT_TIMESTAMP);
CREATE TABLE books (id INTEGER PRIMARY KEY AUTOINCREMENT, author_id INTEGER NOT NULL REFERENCES authors(id), title TEXT NOT NULL, price REAL, cover BLOB);
CREATE INDEX books_author ON books(author_id);
CREATE VIEW cheap_books AS SELECT * FROM books WHERE price < 10;
INSERT INTO authors (name, verified, meta) VALUES ('Tolkien', 1, '{"country":"UK"}'), ('Herbert', 0, NULL), ('Le Guin', 1, '{"awards":5}');
INSERT INTO books (author_id, title, price, cover) VALUES (1, 'The Hobbit', 9.5, x'89504e47'), (1, 'LOTR', 25, NULL), (2, 'Dune', 12.99, NULL), (3, 'Earthsea', 7.25, NULL);
