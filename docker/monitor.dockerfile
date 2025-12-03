FROM rust:1.91.1-bullseye

COPY ./codagem /app

WORKDIR /app

CMD [ "cargo", "run", "--release" ]