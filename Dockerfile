# --- build stage -------------------------------------------------------------
FROM rust:1-bookworm AS builder
WORKDIR /app
COPY . .
# Build only the server (pulls in pathsmith-core). The web UI is embedded into the
# binary at compile time via rust-embed, so the runtime image needs nothing else.
RUN cargo build --release -p pathsmith-server

# --- runtime stage -----------------------------------------------------------
# distroless/cc provides glibc + libgcc (needed by the dynamically-linked binary)
# with no shell or package manager — tiny and low attack surface.
FROM gcr.io/distroless/cc-debian12
COPY --from=builder /app/target/release/pathsmith-server /usr/local/bin/pathsmith-server
EXPOSE 8080
ENV PORT=8080 PATHSMITH_HOST=0.0.0.0
# UI on by default; set PATHSMITH_UI=0 to run as a pure API.
ENTRYPOINT ["/usr/local/bin/pathsmith-server"]
