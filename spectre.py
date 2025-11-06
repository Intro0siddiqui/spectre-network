#!/usr/bin/env python3
"""
Spectre Network - Main Integration Script
Coordinates the entire polyglot proxy pipeline
"""

import json
import subprocess
import time
import argparse
import sys
import os
from pathlib import Path
import logging

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s',
    handlers=[
        logging.FileHandler('logs/spectre.log'),
        logging.StreamHandler()
    ]
)
logger = logging.getLogger(__name__)

class SpectreOrchestrator:
    """Orchestrates the complete Spectre Network pipeline"""
    
    def __init__(self, workspace_dir: str = "/workspace/spectre-network"):
        self.workspace_dir = Path(workspace_dir)
        self.go_scraper = self.workspace_dir / "go_scraper"
        self.python_polish = self.workspace_dir / "python_polish.py"
        self.mojo_rotator = self.workspace_dir / "rotator.mojo"
        
    def run_go_scraper(self, limit: int = 500, protocol: str = "all") -> bool:
        """Run the Go proxy scraper"""
        logger.info("Starting Go scraper...")
        
        try:
            cmd = [
                str(self.go_scraper),
                "--limit", str(limit),
                "--protocol", protocol
            ]
            
            # Run scraper and capture output
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=300  # 5 minute timeout
            )
            
            if result.returncode == 0:
                logger.info("Go scraper completed successfully")
                logger.debug(f"Scraper output: {result.stdout}")
                
                # Save raw proxies
                raw_file = self.workspace_dir / "raw_proxies.json"
                with open(raw_file, 'w') as f:
                    f.write(result.stdout)
                
                return True
            else:
                logger.error(f"Go scraper failed: {result.stderr}")
                return False
                
        except subprocess.TimeoutExpired:
            logger.error("Go scraper timed out")
            return False
        except Exception as e:
            logger.error(f"Go scraper error: {e}")
            return False
    
    def run_python_polish(self, input_file: str = None) -> bool:
        """Run the Python proxy polisher"""
        logger.info("Starting Python polisher...")
        
        try:
            cmd = [
                "python3", str(self.python_polish)
            ]
            
            if input_file:
                cmd.extend(["--input", input_file])
            
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=120  # 2 minute timeout
            )
            
            if result.returncode == 0:
                logger.info("Python polisher completed successfully")
                logger.debug(f"Polish output: {result.stdout}")
                
                # Parse and display summary
                lines = result.stdout.strip().split('\n')
                for line in lines:
                    if line.startswith('=== Spectre Polish Summary ==='):
                        print(line)
                    elif any(keyword in line for keyword in ['Total proxies', 'DNS-capable', 'Non-DNS', 'Average latency']):
                        print(line)
                
                return True
            else:
                logger.error(f"Python polisher failed: {result.stderr}")
                return False
                
        except Exception as e:
            logger.error(f"Python polisher error: {e}")
            return False
    
    def run_mojo_rotator(self, mode: str = "phantom", test: bool = True) -> bool:
        """Run the Mojo rotator"""
        logger.info("Starting Mojo rotator...")
        
        try:
            cmd = [
                "mojo", "run", str(self.mojo_rotator)
            ]
            
            if mode:
                cmd.extend(["--mode", mode])
            if test:
                cmd.append("--test")
            
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=60  # 1 minute timeout
            )
            
            if result.returncode == 0:
                logger.info("Mojo rotator completed successfully")
                print(result.stdout)
                return True
            else:
                logger.error(f"Mojo rotator failed: {result.stderr}")
                print("Mojo error:", result.stderr)
                return False
                
        except FileNotFoundError:
            logger.error("Mojo not found. Please install Mojo SDK from https://www.modular.com/mojo")
            print("⚠️  Mojo SDK not installed. Please install it first.")
            return False
        except Exception as e:
            logger.error(f"Mojo rotator error: {e}")
            return False
    
    def get_proxy_stats(self) -> dict:
        """Get statistics from processed proxy pools"""
        stats = {
            'raw_count': 0,
            'dns_count': 0,
            'non_dns_count': 0,
            'combined_count': 0,
            'avg_latency': 0.0,
            'avg_score': 0.0
        }
        
        # Check raw proxies
        raw_file = self.workspace_dir / "raw_proxies.json"
        if raw_file.exists():
            try:
                with open(raw_file) as f:
                    raw_data = json.load(f)
                    stats['raw_count'] = len(raw_data)
            except:
                pass
        
        # Check DNS proxies
        dns_file = self.workspace_dir / "proxies_dns.json"
        if dns_file.exists():
            try:
                with open(dns_file) as f:
                    dns_data = json.load(f)
                    stats['dns_count'] = len(dns_data)
                    if dns_data:
                        stats['avg_latency'] = sum(p.get('latency', 0) for p in dns_data) / len(dns_data)
                        stats['avg_score'] = sum(p.get('score', 0) for p in dns_data) / len(dns_data)
            except:
                pass
        
        # Check non-DNS proxies
        non_dns_file = self.workspace_dir / "proxies_non_dns.json"
        if non_dns_file.exists():
            try:
                with open(non_dns_file) as f:
                    non_dns_data = json.load(f)
                    stats['non_dns_count'] = len(non_dns_data)
            except:
                pass
        
        # Check combined
        combined_file = self.workspace_dir / "proxies_combined.json"
        if combined_file.exists():
            try:
                with open(combined_file) as f:
                    combined_data = json.load(f)
                    stats['combined_count'] = len(combined_data)
            except:
                pass
        
        return stats
    
    def run_full_pipeline(self, limit: int = 500, protocol: str = "all", mode: str = "phantom"):
        """Run the complete Spectre pipeline"""
        start_time = time.time()
        
        logger.info("🕵️  Starting Spectre Network full pipeline...")
        logger.info(f"Parameters: limit={limit}, protocol={protocol}, mode={mode}")
        
        # Step 1: Go scraper
        if not self.run_go_scraper(limit, protocol):
            logger.error("Pipeline failed at Go scraper step")
            return False
        
        # Step 2: Python polisher
        if not self.run_python_polish("raw_proxies.json"):
            logger.error("Pipeline failed at Python polisher step")
            return False
        
        # Step 3: Mojo rotator
        if not self.run_mojo_rotator(mode, test=True):
            logger.error("Pipeline failed at Mojo rotator step")
            return False
        
        # Step 4: Show statistics
        stats = self.get_proxy_stats()
        self.print_pipeline_summary(stats, time.time() - start_time)
        
        logger.info("✅ Spectre Network pipeline completed successfully!")
        return True
    
    def print_pipeline_summary(self, stats: dict, duration: float):
        """Print pipeline completion summary"""
        print("\n" + "="*60)
        print("🕵️  SPECTRE NETWORK PIPELINE SUMMARY")
        print("="*60)
        print(f"📊 Raw Proxies Scraped: {stats['raw_count']}")
        print(f"🔒 DNS-Capable Proxies: {stats['dns_count']}")
        print(f"🌐 Non-DNS Proxies: {stats['non_dns_count']}")
        print(f"📈 Combined Pool: {stats['combined_count']}")
        print(f"⚡ Average Latency: {stats['avg_latency']:.3f}s")
        print(f"🎯 Average Score: {stats['avg_score']:.3f}")
        print(f"⏱️  Total Duration: {duration:.2f}s")
        print("="*60)
        
        # Calculate success rates
        if stats['raw_count'] > 0:
            dns_rate = (stats['dns_count'] / stats['raw_count']) * 100
            total_rate = (stats['combined_count'] / stats['raw_count']) * 100
            print(f"📊 DNS Pool Rate: {dns_rate:.1f}%")
            print(f"📊 Total Pool Rate: {total_rate:.1f}%")

def main():
    parser = argparse.ArgumentParser(description="Spectre Network Orchestrator")
    parser.add_argument("--mode", choices=["lite", "stealth", "high", "phantom"], 
                       default="phantom", help="Anonymity mode")
    parser.add_argument("--limit", type=int, default=500, help="Proxy limit")
    parser.add_argument("--protocol", choices=["all", "http", "https", "socks4", "socks5"],
                       default="all", help="Proxy protocol")
    parser.add_argument("--step", choices=["scrape", "polish", "rotate", "full"],
                       default="full", help="Pipeline step to run")
    parser.add_argument("--stats", action="store_true", help="Show proxy statistics")
    
    args = parser.parse_args()
    
    # Initialize orchestrator
    orchestrator = SpectreOrchestrator()
    
    # Create logs directory
    os.makedirs("logs", exist_ok=True)
    
    if args.stats:
        stats = orchestrator.get_proxy_stats()
        orchestrator.print_pipeline_summary(stats, 0)
        return
    
    # Run requested pipeline step
    if args.step == "scrape":
        success = orchestrator.run_go_scraper(args.limit, args.protocol)
    elif args.step == "polish":
        success = orchestrator.run_python_polish("raw_proxies.json")
    elif args.step == "rotate":
        success = orchestrator.run_mojo_rotator(args.mode, test=True)
    elif args.step == "full":
        success = orchestrator.run_full_pipeline(args.limit, args.protocol, args.mode)
    
    sys.exit(0 if success else 1)

if __name__ == "__main__":
    main()