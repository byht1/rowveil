# Rowveil web server (the UI opens in a browser).
# The desktop app is built on the host: cargo build --release

# ---- build ----
FROM rust:1-slim-bookworm AS build
WORKDIR /app
# Cache dependencies separately from the app source.
COPY Cargo.toml Cargo.lock build.rs ./
RUN mkdir -p src/bin static \
 && echo '' > src/lib.rs \
 && echo 'fn main() {}' > src/main.rs \
 && echo 'fn main() {}' > src/bin/desktop.rs \
 && cargo build --release --no-default-features --bin rowveil-server \
 && rm -rf src
COPY src src
COPY static static
RUN touch src/lib.rs src/main.rs && cargo build --release --no-default-features --bin rowveil-server

# ---- runtime ----
FROM debian:bookworm-slim
RUN useradd --system --uid 10001 rowveil && mkdir /data && chown rowveil /data
COPY --from=build /app/target/release/rowveil-server /usr/local/bin/rowveil-server
USER rowveil
ENV STUDIO_HOST=0.0.0.0 \
    STUDIO_PORT=4983 \
    STUDIO_NO_OPEN=true \
    STUDIO_DATA_DIR=/data
EXPOSE 4983
VOLUME /data
LABEL org.opencontainers.image.title="Rowveil" \
      org.opencontainers.image.description="Database explorer and query tool for PostgreSQL, MySQL, SQLite, Redis and BigQuery"
ENTRYPOINT ["rowveil-server"]
