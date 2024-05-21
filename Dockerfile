FROM rust:1.72-alpine3.18 as builder

ARG ALPINE_REPO
ARG MITM_CA
ARG http_proxy
ENV http_proxy=$http_proxy
ENV https_proxy=$http_proxy
ENV HTTP_PROXY=$http_proxy
ENV HTTPS_PROXY=$http_proxy

# allowing custom package repositories
RUN printf "${ALPINE_REPO}/main\n${ALPINE_REPO}/community\n" > /etc/apk/repositories
RUN cat /etc/apk/repositories

# allowing MITM attacks (requirement for some build systems)
RUN echo "$MITM_CA" > /root/mitm-ca.crt
RUN cat /root/mitm-ca.crt >> /etc/ssl/certs/ca-certificates.crt
RUN apk --no-cache add ca-certificates \
 && rm -rf /var/cache/apk/*
RUN echo "$MITM_CA" > /usr/local/share/ca-certificates/mitm-ca.crt
RUN update-ca-certificates

RUN apk add --no-cache build-base
WORKDIR /app
COPY . .
RUN mkdir target && chown bin target && mkdir dist && chown bin dist
USER bin
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

FROM alpine
COPY --from=builder /app/dist /
USER 65534
ENTRYPOINT ["/bin/bdk-reserves-web"]
