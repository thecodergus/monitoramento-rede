FROM rust:1.91.1-bullseye

RUN cargo install sqlx-cli

COPY ./codagem /app

WORKDIR /app

CMD [ "cargo", "run", "--release" ]