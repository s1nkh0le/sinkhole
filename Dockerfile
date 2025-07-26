# -------- builder --------
FROM rust:1.88 AS builder
WORKDIR /build

# Copy & build
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN cargo build --release --bin tfilter


# -------- runtime (reuse builder base) --------
FROM builder AS runtime
RUN rm -rf /build/target/debug /usr/local/cargo/registry

WORKDIR /app
COPY --from=builder build/target/release/tfilter /app/tfilter

ENV CTRL_PORT=7000 DEBUG=0 BODY_LIMIT_BYTES=1048576
EXPOSE 7000
ENTRYPOINT ["/app/tfilter"]