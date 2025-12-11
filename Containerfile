# Stage 1: Build Go Scraper
FROM docker.io/library/golang:latest AS go-builder
WORKDIR /app
COPY go_scraper.go .
COPY go.mod .
COPY go.sum .
# Initialize mod if needed or just build. The files exist.
# Downgrade Go requirement to match available stable images (1.23 is usually safe)
RUN sed -i 's/go 1.25.4/go 1.23/g' go.mod
RUN go mod tidy
RUN go build -o go_scraper go_scraper.go

# Stage 2: Build Rust Orchestrator
FROM docker.io/library/rust:latest AS rust-builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
# Build release binary
RUN cargo build --release --bin spectre

# Stage 3: Runtime
FROM docker.io/library/ubuntu:24.04
WORKDIR /app

# Install dependencies for runtime (SSL, curl for testing)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    openssl \
    curl \
    time \
    && rm -rf /var/lib/apt/lists/*

# Copy binaries
COPY --from=go-builder /app/go_scraper /app/go_scraper
COPY --from=rust-builder /app/target/release/spectre /app/spectre

# Copy config/data if needed (none strictly required by code analysis, 
# but it writes json files to CWD)

# Expose SOCKS port
EXPOSE 1080

# Entrypoint script to keep container running or allow exec
CMD ["/bin/bash"]
