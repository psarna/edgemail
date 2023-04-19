FROM rust:slim-buster

WORKDIR /usr/src/app

COPY . .
RUN cargo build --release

EXPOSE 8080
ENTRYPOINT while true; do ./target/release/edgemail "0.0.0.0:8080"; done
