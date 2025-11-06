#!/usr/bin/env python3
"""
Spectre Network - Example Usage Scripts
Demonstrates various ways to use the polyglot proxy mesh
"""

import json
import time
import requests
from pathlib import Path
import argparse

def example_basic_usage():
    """Basic usage example"""
    print("🕵️  Spectre Network - Basic Usage Example")
    print("=" * 50)
    
    # Check if we have proxy files
    workspace = Path("/workspace/spectre-network")
    dns_file = workspace / "proxies_dns.json"
    non_dns_file = workspace / "proxies_non_dns.json"
    
    if dns_file.exists():
        print("✅ Found DNS-capable proxies")
        with open(dns_file) as f:
            dns_proxies = json.load(f)
            print(f"📊 DNS Proxies available: {len(dns_proxies)}")
            
            if dns_proxies:
                # Show top 3 proxies
                print("\n🔒 Top 3 DNS-capable proxies:")
                for i, proxy in enumerate(dns_proxies[:3]):
                    print(f"  {i+1}. {proxy['ip']}:{proxy['port']} ({proxy['type']}) - "
                          f"Latency: {proxy.get('latency', 'N/A'):.3f}s, "
                          f"Score: {proxy.get('score', 'N/A'):.2f}")
    
    if non_dns_file.exists():
        print("\n✅ Found non-DNS proxies")
        with open(non_dns_file) as f:
            non_dns_proxies = json.load(f)
            print(f"📊 Non-DNS Proxies available: {len(non_dns_proxies)}")
    
    print("\n💡 To use these proxies:")
    print("1. Run: python3 spectre.py --step full")
    print("2. Or: mojo run rotator.mojo --mode phantom")

def example_phantom_mode():
    """Phantom mode example"""
    print("\n👻 Spectre Network - Phantom Mode Example")
    print("=" * 50)
    
    print("Phantom mode provides the highest level of anonymity:")
    print("• Multi-hop proxy chains (3-5 hops)")
    print("• Per-hop ECDH/AES-GCM encryption")
    print("• Forward secrecy with key rotation")
    print("• Correlation attack detection")
    print("• Dynamic chain rebuilding")
    
    print("\n🚀 To test phantom mode:")
    print("python3 spectre.py --mode phantom --step rotate")
    print("mojo run rotator.mojo --mode phantom --test")
    
    print("\n🛡️  Security features:")
    print("• DNS leak protection via socks5h://")
    print("• Traffic padding against timing attacks")
    print("• Quantum-resistant crypto ready")
    print("• Sentinel monitoring for correlation")

def example_bulk_scraping():
    """Bulk scraping example"""
    print("\n📈 Spectre Network - Bulk Scraping Example")
    print("=" * 50)
    
    print("For large-scale scraping operations:")
    
    # Show usage for different scales
    scenarios = [
        {"name": "Quick Test", "limit": 100, "description": "Fast testing"},
        {"name": "Standard", "limit": 500, "description": "Normal operations"},
        {"name": "Heavy Load", "limit": 1000, "description": "Intensive scraping"},
        {"name": "Enterprise", "limit": 2000, "description": "Large-scale operations"}
    ]
    
    for scenario in scenarios:
        print(f"\n{scenario['name']} ({scenario['limit']} proxies):")
        print(f"  Description: {scenario['description']}")
        print(f"  Command: python3 spectre.py --limit {scenario['limit']} --step full")
        print(f"  Expected time: {scenario['limit']//100 * 2}-{scenario['limit']//100 * 5} minutes")
    
    print("\n⚠️  Rate limiting recommendations:")
    print("• Standard mode: 50 requests/minute")
    print("• Stealth mode: 30 requests/minute") 
    print("• Phantom mode: 10 requests/minute")
    print("• Use burst: 2-3x normal rate")

def example_modes_comparison():
    """Compare different modes"""
    print("\n🔍 Spectre Network - Mode Comparison")
    print("=" * 50)
    
    modes = [
        {
            "name": "Lite",
            "speed": "Fast (0.5s)",
            "anonymity": "Basic",
            "use_case": "Bulk scraping, low-risk",
            "dns": "Local",
            "encryption": "None"
        },
        {
            "name": "Stealth", 
            "speed": "Medium (0.8s)",
            "anonymity": "Medium",
            "use_case": "Evasion, moderate threat",
            "dns": "Local",
            "encryption": "TLS"
        },
        {
            "name": "High",
            "speed": "Slow (1.2s)", 
            "anonymity": "High",
            "use_case": "Leak-proof operations",
            "dns": "Remote",
            "encryption": "TLS"
        },
        {
            "name": "Phantom",
            "speed": "Very Slow (2-4s)",
            "anonymity": "Maximum", 
            "use_case": "High-threat, OSINT",
            "dns": "Multi-hop",
            "encryption": "Per-hop AES-GCM"
        }
    ]
    
    print(f"{'Mode':<10} {'Speed':<12} {'Anonymity':<12} {'DNS':<12} {'Encryption':<15} {'Use Case'}")
    print("-" * 90)
    
    for mode in modes:
        print(f"{mode['name']:<10} {mode['speed']:<12} {mode['anonymity']:<12} "
              f"{mode['dns']:<12} {mode['encryption']:<15} {mode['use_case']}")
    
    print("\n💡 Mode selection guide:")
    print("• Lite: Development, testing, low-value targets")
    print("• Stealth: General web scraping, moderate protection")
    print("• High: Sensitive data, API rate limiting evasion")  
    print("• Phantom: Investigative journalism, OSINT, high-threat")

def example_integration():
    """Integration examples"""
    print("\n🔌 Spectre Network - Integration Examples")
    print("=" * 50)
    
    print("1. Python requests integration:")
    print("""
import json
import requests

# Load proxies from Spectre
with open('proxies_dns.json') as f:
    proxies = json.load(f)

# Use top proxy
proxy = proxies[0]
proxy_url = f"{proxy['type']}://{proxy['ip']}:{proxy['port']}"
proxies_dict = {'http': proxy_url, 'https': proxy_url}

response = requests.get('https://httpbin.org/ip', proxies=proxies_dict)
print(response.json())
    """)
    
    print("\n2. Selenium WebDriver integration:")
    print("""
from selenium import webdriver
from selenium.webdriver.common.proxy import Proxy, ProxyType

# Configure proxy
proxy = Proxy()
proxy.proxy_type = ProxyType.MANUAL
proxy.http_proxy = "198.50.250.77:3128"
proxy.ssl_proxy = "198.50.250.77:3128"

options = webdriver.ChromeOptions()
options.proxy = proxy

driver = webdriver.Chrome(options=options)
driver.get("https://httpbin.org/ip")
print(driver.page_source)
driver.quit()
    """)
    
    print("\n3. aiohttp async integration:")
    print("""
import aiohttp
import asyncio

async def fetch_with_proxy():
    proxy_url = "https://198.50.250.77:3128"
    
    async with aiohttp.ClientSession() as session:
        async with session.get('https://httpbin.org/ip', proxy=proxy_url) as resp:
            return await resp.json()

result = asyncio.run(fetch_with_proxy())
print(result)
    """)

def example_security_audit():
    """Security audit example"""
    print("\n🔒 Spectre Network - Security Audit Example")
    print("=" * 50)
    
    print("Testing for potential security issues:")
    
    checks = [
        "DNS leak test: https://www.dnsleaktest.com/",
        "IP verification: https://httpbin.org/ip",
        "WebRTC leak test: https://browserleaks.com/webrtc",
        "Fingerprint test: https://amiunique.org/",
        "Correlation attack simulation",
        "Traffic analysis resistance"
    ]
    
    print("Security validation checklist:")
    for i, check in enumerate(checks, 1):
        print(f"  {i}. {check}")
    
    print("\n🛡️  Spectre's security guarantees:")
    print("• No DNS leaks in High/Phantom modes")
    print("• Per-hop forward secrecy in Phantom mode")
    print("• Random chain rotation prevents correlation")
    print("• Padding defeats traffic analysis")
    print("• No centralized points of failure")

def example_troubleshooting():
    """Troubleshooting guide"""
    print("\n🔧 Spectre Network - Troubleshooting Guide")
    print("=" * 50)
    
    issues = [
        {
            "problem": "Go scraper fails",
            "causes": ["Network timeout", "CAPTCHA protection", "Rate limiting"],
            "solutions": ["Increase timeout", "Use different sources", "Add delays"]
        },
        {
            "problem": "Low proxy success rate",
            "causes": ["Proxy sources down", "Validation too strict", "Geographic issues"],
            "solutions": ["Refresh sources", "Lower validation threshold", "Check target sites"]
        },
        {
            "problem": "Phantom mode slow",
            "causes": ["Chain building", "Encryption overhead", "Proxy quality"],
            "solutions": ["Use fewer hops", "Cache chains", "Pre-filter proxies"]
        },
        {
            "problem": "Mojo not found",
            "causes": ["SDK not installed", "Path not set", "Version mismatch"],
            "solutions": ["Install from modular.com", "Check PATH", "Use Python fallback"]
        }
    ]
    
    for issue in issues:
        print(f"\n❌ Problem: {issue['problem']}")
        print(f"   Possible causes: {', '.join(issue['causes'])}")
        print(f"   Solutions: {', '.join(issue['solutions'])}")
    
    print("\n📞 Getting help:")
    print("• Check logs: tail -f logs/spectre.log")
    print("• Run diagnostics: python3 test_spectre.py")
    print("• GitHub issues: Create detailed bug reports")
    print("• Performance profiling: Use --verbose flags")

def main():
    """Main example runner"""
    parser = argparse.ArgumentParser(description="Spectre Network Usage Examples")
    parser.add_argument("--example", choices=[
        "basic", "phantom", "bulk", "modes", "integration", "security", "troubleshooting"
    ], default="basic", help="Which example to run")
    
    args = parser.parse_args()
    
    examples = {
        "basic": example_basic_usage,
        "phantom": example_phantom_mode,
        "bulk": example_bulk_scraping,
        "modes": example_modes_comparison,
        "integration": example_integration,
        "security": example_security_audit,
        "troubleshooting": example_troubleshooting
    }
    
    examples[args.example]()

if __name__ == "__main__":
    main()