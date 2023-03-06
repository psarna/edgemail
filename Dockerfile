FROM rust:slim-buster

WORKDIR /usr/src/app

COPY . .
RUN cargo build --release

ENTRYPOINT while true; do cargo run --release; done
