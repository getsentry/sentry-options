FROM python:3.13-slim AS builder

RUN apt-get update && apt-get install -y curl build-essential pkg-config libssl-dev && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY sentry-options-validation ./sentry-options-validation
COPY sentry-options-cli ./sentry-options-cli
COPY clients ./clients
COPY examples/rust ./examples/rust
RUN pip install 'maturin[patchelf]>=1.7,<2' && \
    cd clients/python && maturin build --release

FROM python:3.13-slim

COPY --from=builder /build/target/wheels/*.whl /tmp/
RUN pip install /tmp/*.whl && rm /tmp/*.whl

COPY examples/minikube_propagation/probe.py /app/probe.py
COPY examples/minikube_propagation/schema.json /etc/sentry-options/schemas/getsentry/schema.json

ENV SENTRY_OPTIONS_DIR=/etc/sentry-options
CMD ["python", "-u", "/app/probe.py"]
