#!/bin/bash
# Spectre Network Setup Script
# Sets up the complete polyglot proxy mesh environment

set -e

echo "🕵️  Spectre Network Setup - Building Evolved Anonymity"
echo "======================================================"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if running on Gentoo Linux (as per spec)
if [[ -f /etc/gentoo-release ]]; then
    echo -e "${GREEN}✓ Gentoo Linux detected${NC}"
    PKG_MANAGER="emerge"
elif command -v apt-get > /dev/null; then
    echo -e "${YELLOW}⚠ Ubuntu/Debian detected - using apt${NC}"
    PKG_MANAGER="apt-get"
elif command -v yum > /dev/null; then
    echo -e "${YELLOW}⚠ RedHat/CentOS detected - using yum${NC}"
    PKG_MANAGER="yum"
else
    echo -e "${RED}✗ Unsupported package manager${NC}"
    exit 1
fi

# Function to install packages
install_packages() {
    local packages="$1"
    local description="$2"
    
    echo -e "${YELLOW}Installing $description...${NC}"
    
    if [[ "$PKG_MANAGER" == "emerge" ]]; then
        sudo emerge --oneshot $packages
    elif [[ "$PKG_MANAGER" == "apt-get" ]]; then
        sudo apt-get update && sudo apt-get install -y $packages
    elif [[ "$PKG_MANAGER" == "yum" ]]; then
        sudo yum install -y $packages
    fi
    
    echo -e "${GREEN}✓ $description installed${NC}"
}

# Function to install Python packages
install_python_packages() {
    local packages="$1"
    
    echo -e "${YELLOW}Installing Python packages: $packages${NC}"
    pip3 install --user $packages
    echo -e "${GREEN}✓ Python packages installed${NC}"
}

# Check Go installation
if ! command -v go > /dev/null; then
    echo -e "${RED}✗ Go not found${NC}"
    echo "Installing Go..."
    
    if [[ "$PKG_MANAGER" == "emerge" ]]; then
        install_packages "dev-lang/go" "Go language"
    else
        # Download and install Go manually
        cd /tmp
        wget https://go.dev/dl/go1.21.5.linux-amd64.tar.gz
        sudo tar -C /usr/local -xzf go1.21.5.linux-amd64.tar.gz
        echo 'export PATH=$PATH:/usr/local/go/bin' >> ~/.bashrc
        export PATH=$PATH:/usr/local/go/bin
    fi
else
    echo -e "${GREEN}✓ Go found: $(go version)${NC}"
fi

# Check Python installation
if ! command -v python3 > /dev/null; then
    echo -e "${RED}✗ Python3 not found${NC}"
    install_packages "python3 python3-pip" "Python3 and pip"
else
    echo -e "${GREEN}✓ Python3 found: $(python3 --version)${NC}"
fi

# Install Go dependencies
echo -e "${YELLOW}Installing Go dependencies...${NC}"
cd /workspace/spectre-network
go mod download
go mod tidy

# Install Python dependencies
install_python_packages "aiohttp beautifulsoup4 requests urllib3"

# Check Mojo installation
if ! command -v mojo > /dev/null; then
    echo -e "${RED}✗ Mojo not found${NC}"
    echo "Installing Mojo SDK..."
    
    # This is a simplified installation - in practice, users would install via Modular's installer
    echo -e "${YELLOW}⚠ Please install Mojo SDK manually from https://www.modular.com/mojo${NC}"
    echo "Mojo SDK 1.2+ is required for optimal performance"
else
    echo -e "${GREEN}✓ Mojo found: $(mojo --version)${NC}"
fi

# Build Go scraper
echo -e "${YELLOW}Building Go scraper...${NC}"
go build -o go_scraper go_scraper.go
echo -e "${GREEN}✓ Go scraper built${NC}"

# Make scripts executable
chmod +x python_polish.py
chmod +x rotator.mojo

# Create directories
mkdir -p logs
mkdir -p data

# Create log rotation config
sudo tee /etc/logrotate.d/spectre-network > /dev/null <<EOF
/var/log/spectre-network/*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 0644 $(whoami) $(whoami)
}
EOF

# Set up cron job for automatic proxy refresh
CRON_SCRIPT="#!/bin/bash
# Spectre Network Auto-Refresh Script
# Runs every hour to refresh proxy pools

cd /workspace/spectre-network

# Run Go scraper
./go_scraper --limit 500 > raw_proxies.json

# Polish proxies
python3 python_polish.py --input raw_proxies.json

# Log completion
echo \"[\$(date)] Spectre proxy refresh completed\" >> logs/spectre.log

# Clean up old data
find data/ -name \"*.json\" -mtime +1 -delete
"

echo "$CRON_SCRIPT" > /workspace/spectre-network/auto_refresh.sh
chmod +x /workspace/spectre-network/auto_refresh.sh

# Add cron job (commented out for safety)
echo -e "${YELLOW}⚠ To enable automatic proxy refresh, run:${NC}"
echo "crontab -e"
echo "# Add this line:"
echo "0 * * * * /workspace/spectre-network/auto_refresh.sh"

# Create systemd service (optional)
SERVICE_FILE="[Unit]
Description=Spectre Network Proxy Service
After=network.target

[Service]
Type=simple
User=$(whoami)
WorkingDirectory=/workspace/spectre-network
ExecStart=/workspace/spectre-network/rotator.mojo --daemon
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
"

echo "$SERVICE_FILE" > /workspace/spectre-network/spectre-network.service

echo -e "${GREEN}======================================================${NC}"
echo -e "${GREEN}🕵️  Spectre Network Setup Complete!${NC}"
echo -e "${GREEN}======================================================${NC}"
echo
echo "Installation Summary:"
echo "• Go Scraper: ✅ Built and ready"
echo "• Python Polish: ✅ Installed"
echo "• Mojo Rotator: ⚠️ Manual installation required"
echo
echo "Quick Start:"
echo "1. cd /workspace/spectre-network"
echo "2. ./go_scraper --limit 500 | python3 python_polish.py"
echo "3. mojo run rotator.mojo --mode phantom --test"
echo
echo "Files created:"
echo "• go_scraper (Go binary)"
echo "• python_polish.py (Python script)"
echo "• rotator.mojo (Mojo script)"
echo "• auto_refresh.sh (Cron script)"
echo "• spectre-network.service (Systemd service)"
echo
echo "For Mojo installation: https://www.modular.com/mojo"
echo "Documentation: ./README.md"