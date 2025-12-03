FROM rust:1.91.1-bullseye

ENV DATABASE_URL postgres://gus:gus@postgres:5432/monitoramento_rede

RUN cargo install sqlx-cli

COPY ./codagem/Cargo.toml ./codagem/config.toml /app/
COPY ./codagem/src/ /app/src/

WORKDIR /app

RUN cargo sqlx prepare

RUN cargo build --release

CMD [ "cargo", "run", "--release" ]