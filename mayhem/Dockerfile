# Build step
FROM fuzzers/cargo-fuzz:0.11.0 as builder
RUN rustup toolchain install nightly

# Put sources in /project
ADD . /project
WORKDIR /project

RUN cd crates/ruff_python_formatter && cargo +nightly fuzz build
RUN cd crates/ruff && cargo +nightly fuzz build

# Packaging step
FROM ubuntu:22.04
COPY --from=builder /project/crates/ruff_python_formatter/fuzz/target/x86_64-unknown-linux-gnu/release/fmt /
COPY --from=builder /project/crates/ruff/fuzz/target/x86_64-unknown-linux-gnu/release/lint /
