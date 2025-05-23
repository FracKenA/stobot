FROM docker.io/rust:bookworm-slim AS builder
ENV RUST_BACKTRACE=full
WORKDIR /usr/src/stobot
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry,id=registry \
    --mount=type=cache,target=/usr/src/stobot/target,id=host_target \
    cargo update && cargo install --path .

FROM docker.io/debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/stobot /usr/local/bin/stobot
ENTRYPOINT ["stobot"]
