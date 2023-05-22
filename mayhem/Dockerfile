# Build step
FROM fuzzers/cargo-fuzz:0.11.0 as builder

# Put sources in /project
ADD . /project
WORKDIR /project

RUN cd crates/ruff_python_formatter && cargo fuzz build

# Packaging step
FROM ubuntu:22.04
COPY --from=builder /project/crates/ruff_python_formatter/fuzz/target/x86_64-unknown-linux-gnu/release/fmt /
