# --- chef base ---------------------------------------------------------------
# cargo-chef lets us cache the (expensive) dependency compile in its own layer,
# keyed only on Cargo.toml/Cargo.lock. Source-only changes then skip straight to
# recompiling our three workspace crates instead of the whole dependency tree.
FROM rust:1-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

# --- planner -----------------------------------------------------------------
# Produce a recipe.json describing the dependency graph. This stage's cache busts
# on any source change, but it's cheap (no compilation happens here).
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# --- builder -----------------------------------------------------------------
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Compile dependencies only. This layer is reused as long as recipe.json is
# unchanged — i.e. for every build that doesn't touch Cargo.toml/Cargo.lock.
RUN cargo chef cook --release -p pathsmith-server --recipe-path recipe.json
# Now bring in the real sources and build just our crates against the cached deps.
COPY . .
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
