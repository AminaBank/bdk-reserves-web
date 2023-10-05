FROM rust:1.72-alpine3.18 as builder
RUN apk add --no-cache build-base
USER bin
WORKDIR /app
COPY . .
RUN cargo test
RUN cargo build --release
RUN install -D target/release/bdk-reserves-web dist/bin/bdk-reserves-web
RUN ldd dist/bin/bdk-reserves-web | tr -s [:blank:] '\n' | grep ^/ | xargs -I % install -D % dist/%
RUN ln -s ld-musl-x86_64.so.1 dist/lib/libc.musl-x86_64.so.1

RUN rustup component add clippy-preview \
 && rustup component add rustfmt
RUN cargo install cargo-audit
RUN cargo fmt -- --check
RUN cargo clippy
RUN cargo audit

FROM scratch
COPY --from=builder /app/dist /
USER 65534
ENTRYPOINT ["/bin/bdk-reserves-web"]