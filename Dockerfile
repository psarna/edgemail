FROM rust:slim-buster

WORKDIR /usr/src/app

COPY . .
RUN apt-get update -y && apt-get install -y  libsqlite3-dev
RUN cargo build --release

EXPOSE 8080
ENTRYPOINT while true; do ./target/release/edgemail "0.0.0.0:8080"; done
