# -------- builder --------
FROM rust:1.88 AS builder
WORKDIR /build

# Copy the whole workspace and build both binaries
COPY Cargo.toml Cargo.lock ./
COPY shared shared
COPY filter filter
COPY controller controller
RUN cargo build --release --bin filter --bin controller


# -------- filter runtime (default) --------
FROM debian:bookworm-slim AS filter
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /build/target/release/filter /app/filter

ENV SINKHOLE_FILTER_CONTROL_BIND=0.0.0.0:7001 \
    SINKHOLE_FILTER_DEBUG=false \
    SINKHOLE_FILTER_BODY_LIMIT_BYTES=1048576
EXPOSE 7001
ENTRYPOINT ["/app/filter"]


# -------- controller runtime --------
FROM debian:bookworm-slim AS controller
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /build/target/release/controller /app/controller

ENV SINKHOLE_CONTROLLER_BIND=0.0.0.0:7000 \
    SINKHOLE_CONTROLLER_DEBUG=false
EXPOSE 7000
ENTRYPOINT ["/app/controller"]
