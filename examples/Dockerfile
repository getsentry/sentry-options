FROM python:3.13-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y curl build-essential pkg-config libssl-dev && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build

# Copy source for building Python client
COPY Cargo.toml Cargo.lock ./
COPY sentry-options-validation ./sentry-options-validation
COPY sentry-options-cli ./sentry-options-cli
COPY clients ./clients
COPY examples/rust ./examples/rust

# Build Python wheel
RUN pip install maturin[patchelf] && \
    cd clients/python && \
    maturin build --release

FROM python:3.13-slim

WORKDIR /app

# Install built wheel
COPY --from=builder /build/target/wheels/*.whl /tmp/
RUN pip install /tmp/*.whl && rm /tmp/*.whl

# Copy example script
COPY examples/python/main.py /app/main.py

# Copy schemas (baked into image)
COPY examples/schemas/sentry-options-testing /etc/sentry-options/schemas/sentry-options-testing

# Values directory created by k8s when mounting ConfigMap

ENV SENTRY_OPTIONS_DIR=/etc/sentry-options

CMD ["python", "-u", "main.py"]
