# --- web UI ---
FROM node:22-slim AS web
WORKDIR /src
COPY package.json package-lock.json ./
COPY packages/lonk-client/package.json packages/lonk-client/
COPY web/package.json web/
COPY e2e/package.json e2e/
RUN npm ci
COPY packages/lonk-client packages/lonk-client
COPY web web
RUN npm run build

# --- lonkd ---
# rust-embed embeds web/dist at compile time, so the web stage must run first
FROM rust:1-slim AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY --from=web /src/web/dist web/dist
RUN cargo build --release --bin lonkd

# --- runtime ---
FROM debian:bookworm-slim
# ca-certificates: outbound HTTPS for the /:id/status dead-link probes
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/lonkd /usr/local/bin/lonkd
RUN mkdir -p /data && chown 65534:65534 /data
ENV LONK_DB=/data/lonk.db ROCKET_ADDRESS=0.0.0.0 ROCKET_PORT=8000
EXPOSE 8000
VOLUME /data
USER 65534:65534
ENTRYPOINT ["lonkd"]
