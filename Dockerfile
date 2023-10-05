FROM alpine:3.18 as builder
RUN apk add alpine-sdk build-base cargo
RUN adduser -S -G abuild satoshi
USER satoshi
WORKDIR /home/satoshi
COPY . .
RUN cargo test
RUN cargo build --release
RUN ldd target/release/bdk-reserves-web

FROM alpine:3.18 as runner
COPY --from=builder /home/satoshi/target/release/bdk-reserves-web /bin/bdk-reserves-web
RUN apk add --no-cache libstdc++
RUN adduser -S -G abuild satoshi
USER satoshi
CMD ["/bin/bdk-reserves-web"]