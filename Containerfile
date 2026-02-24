# Spectre Network — Runtime Image (Podman)
#
# Build workflow:
#   1. cargo build --release
#   2. CGO_ENABLED=1 go build -ldflags="-s -w" -o spectre orchestrator.go
#   3. go build -o go_scraper go_scraper.go
#   4. ./spectre run --mode phantom --limit 500   # populates proxies_*.json
#   5. podman build -t spectre-preloaded -f Containerfile .
#   6. podman run -d --name spectre-node -p 1080:1080 spectre-preloaded

FROM ubuntu:24.04
WORKDIR /app

LABEL maintainer="spectre-network"
LABEL version="1.0.0"
LABEL description="Adversarial proxy mesh — pre-loaded runtime image"

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*



# Install binaries to PATH
COPY spectre /usr/local/bin/spectre
COPY go_scraper /usr/local/bin/go_scraper
RUN chmod +x /usr/local/bin/spectre /usr/local/bin/go_scraper

# Copy pre-filled proxy pool — avoids scraping inside the container
COPY proxies_dns.json proxies_non_dns.json proxies_combined.json /app/

# Non-root user
RUN groupadd --gid 2000 spectre && \
    useradd --uid 2000 --gid spectre --create-home --shell /bin/bash spectre && \
    chown -R spectre:spectre /app

USER spectre:spectre
EXPOSE 1080

HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD curl -s --socks5-hostname 127.0.0.1:1080 https://api.ipify.org || exit 1

# Rotate chain from the pre-loaded pool, then start the SOCKS5 server
CMD ["spectre", "serve", "--mode", "phantom", "--port", "1080"]
