# Stage 1: Build Rust Shared Library
FROM docker.io/library/rust:latest AS rust-builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
# Build release shared library
# Note: we disable the pyo3 extension module behavior in Cargo.toml
ENV PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1
RUN cargo build --release

# Stage 2: Build Go Scraper & Orchestrator
FROM docker.io/library/golang:latest AS go-builder
WORKDIR /app
# Install CGO requirements
RUN apt-get update && apt-get install -y gcc pkg-config python3-dev
COPY --from=rust-builder /app/target/release/librotator_rs.so ./target/release/
COPY go_scraper.go orchestrator.go .
COPY go.mod go.sum .
RUN sed -i 's/go 1.25.4/go 1.23/g' go.mod
RUN go mod tidy
# Build scraper
RUN go build -o go_scraper go_scraper.go
# Build orchestrator with CGO
RUN CGO_ENABLED=1 go build -o spectre orchestrator.go

# Stage 3: Runtime
FROM docker.io/library/ubuntu:24.04
WORKDIR /app

# Install dependencies for runtime (SSL, curl for testing, python3 lib)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    openssl \
    curl \
    time \
    python3 \
    && rm -rf /var/lib/apt/lists/*

# Copy binaries and shared libraries
COPY --from=rust-builder /app/target/release/librotator_rs.so /usr/lib/librotator_rs.so
RUN ldconfig
COPY --from=go-builder /app/go_scraper /app/go_scraper
COPY --from=go-builder /app/spectre /app/spectre

# Copy config/data if needed (none strictly required by code analysis, 
# but it writes json files to CWD)

# Expose SOCKS port
EXPOSE 1080

# Entrypoint script to keep container running or allow exec
CMD ["/bin/bash"]
