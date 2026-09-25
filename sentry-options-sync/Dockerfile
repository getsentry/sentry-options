FROM rust:1-bookworm AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY sentry-options-validation ./sentry-options-validation
COPY sentry-options-cli ./sentry-options-cli
COPY sentry-options-sync ./sentry-options-sync
COPY clients ./clients
COPY examples/rust ./examples/rust
RUN cargo build --release -p sentry-options-sync

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /build/target/release/sentry-options-sync /sentry-options-sync
ENTRYPOINT ["/sentry-options-sync"]
