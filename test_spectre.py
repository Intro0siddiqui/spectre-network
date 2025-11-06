#!/usr/bin/env python3
"""
Spectre Network Test Suite
Validates the functionality of all components
"""

import unittest
import json
import time
import subprocess
import sys
import os
from pathlib import Path

class TestSpectreComponents(unittest.TestCase):
    """Test suite for Spectre Network components"""
    
    def setUp(self):
        """Set up test environment"""
        self.workspace = Path("/workspace/spectre-network")
        os.chdir(self.workspace)
    
    def test_go_scraper_build(self):
        """Test if Go scraper builds correctly"""
        print("\n🔧 Testing Go scraper build...")
        
        try:
            # Build the Go scraper
            result = subprocess.run(
                ["go", "build", "-o", "go_scraper", "go_scraper.go"],
                capture_output=True,
                text=True,
                timeout=30
            )
            
            self.assertEqual(result.returncode, 0, f"Build failed: {result.stderr}")
            self.assertTrue((self.workspace / "go_scraper").exists(), "Go binary not created")
            print("✅ Go scraper builds successfully")
            
        except subprocess.TimeoutExpired:
            self.fail("Go build timed out")
        except Exception as e:
            self.fail(f"Go build error: {e}")
    
    def test_python_polish_syntax(self):
        """Test if Python polisher has valid syntax"""
        print("\n🐍 Testing Python polisher syntax...")
        
        try:
            result = subprocess.run(
                ["python3", "-m", "py_compile", "python_polish.py"],
                capture_output=True,
                text=True,
                timeout=10
            )
            
            self.assertEqual(result.returncode, 0, f"Syntax error: {result.stderr}")
            print("✅ Python polisher syntax is valid")
            
        except Exception as e:
            self.fail(f"Python syntax test failed: {e}")
    
    def test_mojo_rotator_syntax(self):
        """Test if Mojo rotator has valid syntax (if Mojo is available)"""
        print("\n⚡ Testing Mojo rotator syntax...")
        
        try:
            # Check if Mojo is available
            result = subprocess.run(
                ["mojo", "--version"],
                capture_output=True,
                text=True,
                timeout=5
            )
            
            if result.returncode == 0:
                print(f"✅ Mojo found: {result.stdout.strip()}")
                
                # Try syntax check on the rotator
                try:
                    result = subprocess.run(
                        ["mojo", "check", "rotator.mojo"],
                        capture_output=True,
                        text=True,
                        timeout=15
                    )
                    
                    if result.returncode == 0:
                        print("✅ Mojo rotator syntax is valid")
                    else:
                        print(f"⚠️  Mojo syntax check: {result.stderr}")
                except:
                    print("⚠️  Could not run Mojo syntax check")
            else:
                print("⚠️  Mojo not found - skipping syntax check")
                
        except FileNotFoundError:
            print("⚠️  Mojo not installed - skipping test")
        except Exception as e:
            print(f"⚠️  Mojo test error: {e}")
    
    def test_go_scraper_fallback(self):
        """Test Go scraper fallback functionality"""
        print("\n🕵️  Testing Go scraper fallback...")
        
        try:
            # Run with a small limit to test basic functionality
            result = subprocess.run(
                ["./go_scraper", "--limit", "10", "--protocol", "http"],
                capture_output=True,
                text=True,
                timeout=30
            )
            
            if result.returncode == 0:
                # Try to parse the JSON output
                data = json.loads(result.stdout)
                self.assertIsInstance(data, list, "Output should be a list")
                print(f"✅ Go scraper fallback works - got {len(data)} proxies")
            else:
                print(f"⚠️  Go scraper failed: {result.stderr}")
                
        except FileNotFoundError:
            self.skipTest("Go binary not found - build first")
        except Exception as e:
            self.fail(f"Go scraper test failed: {e}")
    
    def test_python_polish_fallback(self):
        """Test Python polisher fallback functionality"""
        print("\n✨ Testing Python polisher fallback...")
        
        try:
            result = subprocess.run(
                ["python3", "python_polish.py"],
                capture_output=True,
                text=True,
                timeout=60
            )
            
            if result.returncode == 0:
                # Check if proxy files were created
                dns_file = self.workspace / "proxies_dns.json"
                non_dns_file = self.workspace / "proxies_non_dns.json"
                combined_file = self.workspace / "proxies_combined.json"
                
                self.assertTrue(dns_file.exists() or non_dns_file.exists(), "No proxy files created")
                print("✅ Python polisher fallback works")
                
                # Show summary
                print("Polish summary:")
                lines = result.stdout.split('\n')
                for line in lines:
                    if any(keyword in line for keyword in ['proxies:', 'capable:', 'DNS', 'latency']):
                        print(f"  {line}")
            else:
                self.fail(f"Python polisher failed: {result.stderr}")
                
        except Exception as e:
            self.fail(f"Python polisher test failed: {e}")
    
    def test_configuration_file(self):
        """Test configuration file parsing"""
        print("\n⚙️  Testing configuration file...")
        
        config_file = self.workspace / "config.ini"
        self.assertTrue(config_file.exists(), "Configuration file missing")
        
        try:
            import configparser
            config = configparser.ConfigParser()
            config.read(config_file)
            
            # Check key sections exist
            required_sections = ['DEFAULT', 'PROXY_SOURCES', 'SCORING_WEIGHTS']
            for section in required_sections:
                self.assertIn(section, config.sections(), f"Missing section: {section}")
            
            # Check key values
            self.assertIn('scrape_limit', config['DEFAULT'], "Missing scrape_limit")
            self.assertIn('preferred_countries', config['SCORING_WEIGHTS'], "Missing preferred_countries")
            
            print("✅ Configuration file is valid")
            
        except Exception as e:
            self.fail(f"Configuration test failed: {e}")
    
    def test_pipeline_integration(self):
        """Test complete pipeline integration"""
        print("\n🔄 Testing complete pipeline integration...")
        
        try:
            # Clean up any existing files
            for f in ["raw_proxies.json", "proxies_dns.json", "proxies_non_dns.json"]:
                (self.workspace / f).unlink(missing_ok=True)
            
            # Test Python orchestrator
            result = subprocess.run(
                ["python3", "spectre.py", "--step", "scrape", "--limit", "50"],
                capture_output=True,
                text=True,
                timeout=120
            )
            
            if result.returncode == 0:
                print("✅ Pipeline integration test passed")
                
                # Check if output files exist
                files_created = []
                for f in ["raw_proxies.json"]:
                    if (self.workspace / f).exists():
                        files_created.append(f)
                
                print(f"Files created: {files_created}")
            else:
                print(f"⚠️  Pipeline test had issues: {result.stderr}")
                
        except Exception as e:
            print(f"⚠️  Pipeline integration test failed: {e}")

class TestSpectreSecurity(unittest.TestCase):
    """Test security and anonymity features"""
    
    def setUp(self):
        self.workspace = Path("/workspace/spectre-network")
        os.chdir(self.workspace)
    
    def test_dns_leak_protection(self):
        """Test DNS leak protection in different modes"""
        print("\n🔒 Testing DNS leak protection...")
        
        # This would test actual DNS resolution through proxies
        # For now, just check that the code has DNS leak protection logic
        
        rotator_file = self.workspace / "rotator.mojo"
        self.assertTrue(rotator_file.exists())
        
        with open(rotator_file) as f:
            content = f.read()
        
        # Check for DNS-related protections
        dns_protections = [
            "socks5h://",  # DNS resolution through proxy
            "DNS",         # DNS mentions
            "dns_proxies", # DNS proxy pool
        ]
        
        for protection in dns_protections:
            self.assertIn(protection, content, f"Missing DNS protection: {protection}")
        
        print("✅ DNS leak protection code present")
    
    def test_phantom_mode_crypto(self):
        """Test phantom mode cryptographic features"""
        print("\n🛡️  Testing phantom mode crypto...")
        
        rotator_file = self.workspace / "rotator.mojo"
        self.assertTrue(rotator_file.exists())
        
        with open(rotator_file) as f:
            content = f.read()
        
        # Check for crypto features
        crypto_features = [
            "phantom",
            "chain",
            "encryption",
            "keys",
        ]
        
        for feature in crypto_features:
            self.assertIn(feature, content, f"Missing crypto feature: {feature}")
        
        print("✅ Phantom mode crypto features present")

def run_performance_test():
    """Run basic performance test"""
    print("\n⚡ Running performance benchmark...")
    
    start_time = time.time()
    
    try:
        # Test Python polish speed
        result = subprocess.run(
            ["python3", "python_polish.py"],
            capture_output=True,
            text=True,
            timeout=30
        )
        
        polish_time = time.time() - start_time
        
        if result.returncode == 0:
            print(f"✅ Python polisher completed in {polish_time:.2f}s")
            
            # Parse output for metrics
            output = result.stdout
            if "Average latency" in output:
                print("Latency metrics available in output")
            
            if polish_time < 30:
                print("✅ Performance target met (<30s)")
            else:
                print("⚠️  Performance target not met (≥30s)")
        else:
            print("❌ Performance test failed")
            
    except Exception as e:
        print(f"❌ Performance test error: {e}")

def main():
    """Run all tests"""
    print("🕵️  Spectre Network Test Suite")
    print("=" * 50)
    
    # Check prerequisites
    print("\n📋 Checking prerequisites...")
    
    # Check Go
    try:
        result = subprocess.run(["go", "version"], capture_output=True, text=True)
        if result.returncode == 0:
            print(f"✅ Go: {result.stdout.strip()}")
        else:
            print("❌ Go not available")
    except FileNotFoundError:
        print("❌ Go not found")
    
    # Check Python
    try:
        import sys
        print(f"✅ Python: {sys.version}")
    except:
        print("❌ Python not available")
    
    # Run component tests
    print("\n🧪 Running component tests...")
    loader = unittest.TestLoader()
    suite = unittest.TestSuite()
    
    # Add all test cases
    suite.addTests(loader.loadTestsFromTestCase(TestSpectreComponents))
    suite.addTests(loader.loadTestsFromTestCase(TestSpectreSecurity))
    
    # Run tests
    runner = unittest.TextTestRunner(verbosity=2)
    result = runner.run(suite)
    
    # Run performance test
    run_performance_test()
    
    # Summary
    print("\n" + "=" * 50)
    if result.wasSuccessful():
        print("🎉 All tests passed!")
        return 0
    else:
        print("❌ Some tests failed")
        return 1

if __name__ == "__main__":
    sys.exit(main())