FROM ubuntu:22.04

RUN apt-get update && apt-get upgrade -y
RUN apt-get install -y build-essential cmake curl python3 clang
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain none -y
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup toolchain install nightly --allow-downgrade --profile minimal --component clippy

RUN update-alternatives --install /usr/bin/python python /usr/bin/python3 1
ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get install -y python3-numpy python3-kubernetes

RUN mkdir -p /app
WORKDIR /app

COPY src/ /app/src/
COPY lib/ /app/lib/
COPY sample/ /app/sample/
COPY tests/ /app/tests/
COPY sim-scripts/ /app/sim-scripts/

COPY Cargo.toml /app/Cargo.toml
COPY Cargo.lock /app/Cargo.lock

RUN cargo build --release
COPY *.sh /app/
RUN chmod +x /app/*.sh
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://github.com/kubernetes-sigs/kwok/releases/download/v0.5.1/kwokctl-linux-amd64 -o /usr/local/bin/kwokctl && chmod +x /usr/local/bin/kwokctl
RUN curl -L --proto "=https" --tlsv1.2 -sSf https://dl.k8s.io/release/v1.29.2/bin/linux/amd64/kubectl -o /usr/local/bin/kubectl && chmod +x /usr/local/bin/kubectl
ENV PATH="/usr/local/bin:${PATH}"

