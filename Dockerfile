ARG ZIG_VERSION=0.12.0

FROM --platform=$BUILDPLATFORM rust:1.88 AS chef
WORKDIR /app
ENV PKGCONFIG_SYSROOTDIR=/

RUN apt-get update && \
  apt-get install -y --no-install-recommends \
  build-essential clang curl git \
  pkg-config libssl-dev ca-certificates && \
  rm -rf /var/lib/apt/lists/*

# manually install Zig as there is no pre-built package for debian
ARG ZIG_VERSION
RUN set -eux; \
  arch="$(dpkg --print-architecture)"; \
  case "$arch" in \
  amd64)  ZARCH=x86_64 ;; \
  arm64)  ZARCH=aarch64 ;; \
  *) echo "unsupported arch $arch" && exit 1 ;; \
  esac; \
  curl -L "https://ziglang.org/download/${ZIG_VERSION}/zig-linux-${ZARCH}-${ZIG_VERSION}.tar.xz" \
  -o /tmp/zig.tar.xz; \
  tar -C /opt -xf /tmp/zig.tar.xz; \
  ln -s /opt/zig-linux-${ZARCH}-${ZIG_VERSION}/zig /usr/local/bin/zig; \
  rm /tmp/zig.tar.xz
# ----------------------------------------------------------------------

RUN cargo install --locked cargo-chef cargo-zigbuild
RUN rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

RUN cargo chef cook --recipe-path recipe.json --release --zigbuild \
  --target x86_64-unknown-linux-gnu \
  --target aarch64-unknown-linux-gnu

COPY . .
RUN cargo zigbuild --release \
  --target x86_64-unknown-linux-gnu \
  --target aarch64-unknown-linux-gnu && \
  mkdir -p /app/linux/amd64 /app/linux/arm64 && \
  cp target/x86_64-unknown-linux-gnu/release/cli /app/linux/amd64/ && \
  cp target/aarch64-unknown-linux-gnu/release/cli /app/linux/arm64/


FROM debian:bookworm-slim AS runtime
WORKDIR /app

ARG TARGETPLATFORM

RUN apt-get update && \
  apt-get install -y --no-install-recommends ca-certificates && \
  rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/${TARGETPLATFORM}/cli /usr/bin/cli
COPY --from=builder /app/resources /app/resources

ENV APP_APPLICATION__HOST="0.0.0.0"

CMD  ["/usr/bin/cli"]
