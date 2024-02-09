# # Build Stage
# FROM messense/rust-musl-cross:aarch64-musl AS builder

# WORKDIR /opt/previewbot/
# COPY . .
# RUN cargo build --release --target aarch64-unknown-linux-musl

# # Bundle Stage
# FROM scratch
# COPY --from=builder /opt/previewbot/target/aarch64-unknown-linux-musl/release/preview_bot .
# ENTRYPOINT [ "./preview_bot" ]

# == Build Stage ==
# Pinned to build platform, uses cross compilation to reach target platform.
FROM --platform=$BUILDPLATFORM rust AS builder

WORKDIR /opt/previewbot/
COPY . .

ARG TARGETPLATFORM
RUN <<EOF
    if   [ $TARGETPLATFORM = "linux/amd64"  ]; then 
        dpkg --add-architecture amd64
        apt-get update
        apt-get install -y clang mold musl-dev musl-tools musl-dev:amd64
        echo "x86_64-unknown-linux-musl" > /opt/.cargo-target
    elif [ $TARGETPLATFORM = "linux/arm64"  ]; then 
        dpkg --add-architecture arm64
        apt-get update
        apt-get install -y clang mold musl-dev musl-tools musl-dev:arm64
        echo "aarch64-unknown-linux-musl" > /opt/.cargo-target
    elif [ $TARGETPLATFORM = "linux/arm/v7" ]; then 
        dpkg --add-architecture armhf
        apt-get update
        apt-get install -y clang mold musl-dev musl-tools musl-dev:armhf
        echo "armv7-unknown-linux-musleabihf" > /opt/.cargo-target
    else 
        echo "ERROR: Unsupported target platform."
        exit 1
    fi
EOF

RUN rustup target add "$(cat /opt/.cargo-target)"
RUN cargo build --release --target "$(cat /opt/.cargo-target)"
RUN cp target/$(cat /opt/.cargo-target)/release/preview_bot .

# == Bundle Stage ==
FROM scratch
COPY --from=builder /opt/previewbot/preview_bot /
CMD [ "./preview_bot" ]

