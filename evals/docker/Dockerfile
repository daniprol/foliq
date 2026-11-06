# The computer the agent works on. Two images come from this file:
#
#   baseline  a Linux shell with the tools an agent usually reaches for:
#             grep, ripgrep, pdftotext, and Python with pypdf, pdfplumber,
#             openpyxl and pandas
#   foliq     the same, plus the `foliq` command built from this checkout
#
# Build both with `uv run python -m foliq_eval prepare` (the build context is
# the repository root). The documents are not in the images: they are mounted
# at /docs when a sample starts.

FROM python:3.12-slim-trixie AS baseline
RUN apt-get update \
    && apt-get install -y --no-install-recommends poppler-utils ripgrep file less \
    && rm -rf /var/lib/apt/lists/*
RUN pip install --no-cache-dir pypdf pdfplumber openpyxl pandas
WORKDIR /docs

# `foliq` is a single Rust binary, so the build stage only needs Rust.
FROM rust:1.97-slim-trixie AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p foliq-cli

FROM baseline AS foliq
COPY --from=build /src/target/release/foliq /usr/local/bin/foliq
