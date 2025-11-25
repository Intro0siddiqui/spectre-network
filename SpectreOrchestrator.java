import com.google.gson.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.logging.*;

/**
 * Spectre Network - Main Integration Class
 * Coordinates the entire polyglot proxy pipeline
 * 
 * Equivalent to spectre.py but implemented in Java
 */
public class SpectreOrchestrator {
    
    private static final Logger LOGGER = Logger.getLogger(SpectreOrchestrator.class.getName());
    
    private final Path workspaceDir;
    private final Path goScraper;
    private final String rustModuleName;
    private final Gson gson;
    
    public SpectreOrchestrator(String workspaceDir) {
        this.workspaceDir = Paths.get(workspaceDir);
        this.goScraper = this.workspaceDir.resolve("go_scraper");
        this.rustModuleName = "rotator_rs";
        this.gson = new GsonBuilder().setPrettyPrinting().create();
        
        // Configure logging
        try {
            Files.createDirectories(Paths.get("logs"));
            FileHandler fileHandler = new FileHandler("logs/spectre.log", true);
            fileHandler.setFormatter(new SimpleFormatter());
            LOGGER.addHandler(fileHandler);
            LOGGER.setLevel(Level.INFO);
        } catch (IOException e) {
            System.err.println("Failed to setup logging: " + e.getMessage());
        }
    }
    
    /**
     * Run the Go proxy scraper
     */
    public boolean runGoScraper(int limit, String protocol) {
        LOGGER.info("Starting Go scraper...");
        
        try {
            ProcessBuilder pb = new ProcessBuilder(
                goScraper.toString(),
                "--limit", String.valueOf(limit),
                "--protocol", protocol
            );
            
            pb.redirectErrorStream(true);
            Process process = pb.start();
            
            // Capture output
            StringBuilder output = new StringBuilder();
            try (BufferedReader reader = new BufferedReader(
                    new InputStreamReader(process.getInputStream()))) {
                String line;
                while ((line = reader.readLine()) != null) {
                    output.append(line).append("\n");
                    LOGGER.fine(line);
                }
            }
            
            // Wait for completion with timeout
            boolean finished = process.waitFor(5, TimeUnit.MINUTES);
            
            if (!finished) {
                process.destroyForcibly();
                LOGGER.severe("Go scraper timed out");
                return false;
            }
            
            if (process.exitValue() == 0) {
                LOGGER.info("Go scraper completed successfully");
                
                // Save raw proxies
                Path rawFile = workspaceDir.resolve("raw_proxies.json");
                Files.writeString(rawFile, output.toString());
                
                return true;
            } else {
                LOGGER.severe("Go scraper failed with exit code: " + process.exitValue());
                return false;
            }
            
        } catch (IOException e) {
            LOGGER.severe("Go scraper IO error: " + e.getMessage());
            return false;
        } catch (InterruptedException e) {
            LOGGER.severe("Go scraper interrupted: " + e.getMessage());
            Thread.currentThread().interrupt();
            return false;
        }
    }
    
    /**
     * Run the Java proxy polisher
     */
    public boolean runJavaPolish(String inputFile) {
        LOGGER.info("Starting Java polisher...");
        
        try {
            ProxyPolish polish = new ProxyPolish(50);
            Map<String, Object> stats = polish.process(inputFile);
            
            if (stats.containsKey("error")) {
                LOGGER.severe("Java polisher failed: " + stats.get("error"));
                return false;
            }
            
            LOGGER.info("Java polisher completed successfully");
            
            // Display summary
            System.out.println("=== Spectre Polish Summary ===");
            System.out.println("Total proxies: " + stats.get("total_proxies"));
            System.out.println("DNS-capable: " + stats.get("dns_proxies"));
            System.out.println("Non-DNS: " + stats.get("non_dns_proxies"));
            System.out.println(String.format("Average latency: %.3fs", stats.get("avg_latency")));
            
            return true;
            
        } catch (Exception e) {
            LOGGER.severe("Java polisher error: " + e.getMessage());
            e.printStackTrace();
            return false;
        }
    }
    
    /**
     * Run the Rust rotator via JNI/JNA
     * Note: This requires Rust library compiled with JNI bindings
     */
    public boolean runRustRotator(String mode) {
        LOGGER.info("Starting Rust rotator (JNI)...");
        
        try {
            // In a real implementation, this would use JNI to call Rust
            // For now, we'll use ProcessBuilder to call a Rust binary
            
            Path rustBinary = workspaceDir.resolve("target/release/rotator_rs");
            
            if (!Files.exists(rustBinary)) {
                LOGGER.warning("Rust binary not found, attempting to build...");
                // Try to build Rust project
                ProcessBuilder buildPb = new ProcessBuilder("cargo", "build", "--release");
                buildPb.directory(workspaceDir.toFile());
                Process buildProcess = buildPb.start();
                buildProcess.waitFor(2, TimeUnit.MINUTES);
            }
            
            // Execute Rust rotator
            ProcessBuilder pb = new ProcessBuilder(
                rustBinary.toString(),
                "--mode", mode,
                "--workspace", workspaceDir.toString()
            );
            
            pb.redirectErrorStream(true);
            Process process = pb.start();
            
            // Capture output
            StringBuilder output = new StringBuilder();
            try (BufferedReader reader = new BufferedReader(
                    new InputStreamReader(process.getInputStream()))) {
                String line;
                while ((line = reader.readLine()) != null) {
                    output.append(line).append("\n");
                }
            }
            
            boolean finished = process.waitFor(30, TimeUnit.SECONDS);
            
            if (!finished) {
                process.destroyForcibly();
                LOGGER.severe("Rust rotator timed out");
                return false;
            }
            
            if (process.exitValue() == 0) {
                LOGGER.info("Rust rotator completed successfully");
                
                // Parse and display decision
                try {
                    JsonObject decision = JsonParser.parseString(output.toString()).getAsJsonObject();
                    displayRustDecision(decision);
                } catch (JsonSyntaxException e) {
                    System.out.println(output.toString());
                }
                
                return true;
            } else {
                LOGGER.severe("Rust rotator failed");
                return false;
            }
            
        } catch (Exception e) {
            LOGGER.severe("Rust rotator error: " + e.getMessage());
            return false;
        }
    }
    
    /**
     * Display Rust rotation decision
     */
    private void displayRustDecision(JsonObject decision) {
        System.out.println("\n=== Spectre Rust Rotator Decision ===");
        System.out.println("Mode: " + decision.get("mode").getAsString());
        System.out.println("Chain ID: " + decision.get("chain_id").getAsString());
        System.out.println("Timestamp: " + decision.get("timestamp").getAsLong());
        
        JsonArray chain = decision.getAsJsonArray("chain");
        JsonArray encryption = decision.getAsJsonArray("encryption");
        
        System.out.println("Chain length: " + chain.size());
        
        for (int i = 0; i < chain.size(); i++) {
            JsonObject hop = chain.get(i).getAsJsonObject();
            JsonObject enc = i < encryption.size() ? encryption.get(i).getAsJsonObject() : null;
            
            System.out.printf(" Hop %d: %s://%s:%d [%s] lat=%.3fs score=%.3f",
                i + 1,
                hop.get("proto").getAsString(),
                hop.get("ip").getAsString(),
                hop.get("port").getAsInt(),
                hop.has("country") ? hop.get("country").getAsString() : "-",
                hop.get("latency").getAsDouble(),
                hop.get("score").getAsDouble()
            );
            
            if (enc != null) {
                String keyHex = enc.get("key_hex").getAsString();
                String nonceHex = enc.get("nonce_hex").getAsString();
                System.out.printf(" key=%s... nonce=%s...",
                    keyHex.substring(0, Math.min(16, keyHex.length())),
                    nonceHex.substring(0, Math.min(12, nonceHex.length()))
                );
            }
            System.out.println();
        }
        
        System.out.printf("Avg latency: %.3fs\n", decision.get("avg_latency").getAsDouble());
        System.out.printf("Score range: %.3f - %.3f\n",
            decision.get("min_score").getAsDouble(),
            decision.get("max_score").getAsDouble()
        );
        System.out.println("=== End Rust Rotator Decision ===\n");
    }
    
    /**
     * Get proxy statistics
     */
    public Map<String, Object> getProxyStats() {
        Map<String, Object> stats = new HashMap<>();
        stats.put("raw_count", 0);
        stats.put("dns_count", 0);
        stats.put("non_dns_count", 0);
        stats.put("combined_count", 0);
        stats.put("avg_latency", 0.0);
        stats.put("avg_score", 0.0);
        
        // Check raw proxies
        Path rawFile = workspaceDir.resolve("raw_proxies.json");
        if (Files.exists(rawFile)) {
            try {
                String content = Files.readString(rawFile);
                JsonArray rawData = JsonParser.parseString(content).getAsJsonArray();
                stats.put("raw_count", rawData.size());
            } catch (Exception e) {
                // Ignore
            }
        }
        
        // Check DNS proxies
        Path dnsFile = workspaceDir.resolve("proxies_dns.json");
        if (Files.exists(dnsFile)) {
            try {
                String content = Files.readString(dnsFile);
                JsonArray dnsData = JsonParser.parseString(content).getAsJsonArray();
                stats.put("dns_count", dnsData.size());
                
                if (dnsData.size() > 0) {
                    double sumLatency = 0.0;
                    double sumScore = 0.0;
                    for (JsonElement elem : dnsData) {
                        JsonObject obj = elem.getAsJsonObject();
                        sumLatency += obj.has("latency") ? obj.get("latency").getAsDouble() : 0.0;
                        sumScore += obj.has("score") ? obj.get("score").getAsDouble() : 0.0;
                    }
                    stats.put("avg_latency", sumLatency / dnsData.size());
                    stats.put("avg_score", sumScore / dnsData.size());
                }
            } catch (Exception e) {
                // Ignore
            }
        }
        
        // Check non-DNS proxies
        Path nonDnsFile = workspaceDir.resolve("proxies_non_dns.json");
        if (Files.exists(nonDnsFile)) {
            try {
                String content = Files.readString(nonDnsFile);
                JsonArray nonDnsData = JsonParser.parseString(content).getAsJsonArray();
                stats.put("non_dns_count", nonDnsData.size());
            } catch (Exception e) {
                // Ignore
            }
        }
        
        // Check combined
        Path combinedFile = workspaceDir.resolve("proxies_combined.json");
        if (Files.exists(combinedFile)) {
            try {
                String content = Files.readString(combinedFile);
                JsonArray combinedData = JsonParser.parseString(content).getAsJsonArray();
                stats.put("combined_count", combinedData.size());
            } catch (Exception e) {
                // Ignore
            }
        }
        
        return stats;
    }
    
    /**
     * Run the complete Spectre pipeline
     */
    public boolean runFullPipeline(int limit, String protocol, String mode) {
        long startTime = System.currentTimeMillis();
        
        LOGGER.info("🕵️  Starting Spectre Network full pipeline...");
        LOGGER.info(String.format("Parameters: limit=%d, protocol=%s, mode=%s", limit, protocol, mode));
        
        // Step 1: Go scraper
        if (!runGoScraper(limit, protocol)) {
            LOGGER.severe("Pipeline failed at Go scraper step");
            return false;
        }
        
        // Step 2: Java polisher
        if (!runJavaPolish("raw_proxies.json")) {
            LOGGER.severe("Pipeline failed at Java polisher step");
            return false;
        }
        
        // Step 3: Rust rotator
        if (!runRustRotator(mode)) {
            LOGGER.severe("Pipeline failed at Rust rotator step");
            return false;
        }
        
        // Step 4: Show statistics
        Map<String, Object> stats = getProxyStats();
        double duration = (System.currentTimeMillis() - startTime) / 1000.0;
        printPipelineSummary(stats, duration);
        
        LOGGER.info("✅ Spectre Network pipeline completed successfully!");
        return true;
    }
    
    /**
     * Print pipeline completion summary
     */
    public void printPipelineSummary(Map<String, Object> stats, double duration) {
        System.out.println("\n" + "=".repeat(60));
        System.out.println("🕵️  SPECTRE NETWORK PIPELINE SUMMARY");
        System.out.println("=".repeat(60));
        System.out.println("📊 Raw Proxies Scraped: " + stats.get("raw_count"));
        System.out.println("🔒 DNS-Capable Proxies: " + stats.get("dns_count"));
        System.out.println("🌐 Non-DNS Proxies: " + stats.get("non_dns_count"));
        System.out.println("📈 Combined Pool: " + stats.get("combined_count"));
        System.out.println(String.format("⚡ Average Latency: %.3fs", stats.get("avg_latency")));
        System.out.println(String.format("🎯 Average Score: %.3f", stats.get("avg_score")));
        System.out.println(String.format("⏱️  Total Duration: %.2fs", duration));
        System.out.println("=".repeat(60));
        
        // Calculate success rates
        int rawCount = (Integer) stats.get("raw_count");
        if (rawCount > 0) {
            int dnsCount = (Integer) stats.get("dns_count");
            int combinedCount = (Integer) stats.get("combined_count");
            double dnsRate = (dnsCount * 100.0) / rawCount;
            double totalRate = (combinedCount * 100.0) / rawCount;
            System.out.println(String.format("📊 DNS Pool Rate: %.1f%%", dnsRate));
            System.out.println(String.format("📊 Total Pool Rate: %.1f%%", totalRate));
        }
    }
    
    /**
     * Main method for command-line execution
     */
    public static void main(String[] args) {
        String mode = "phantom";
        int limit = 500;
        String protocol = "all";
        String step = "full";
        boolean showStats = false;
        
        // Parse command-line arguments
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--mode":
                    if (i + 1 < args.length) mode = args[++i];
                    break;
                case "--limit":
                    if (i + 1 < args.length) limit = Integer.parseInt(args[++i]);
                    break;
                case "--protocol":
                    if (i + 1 < args.length) protocol = args[++i];
                    break;
                case "--step":
                    if (i + 1 < args.length) step = args[++i];
                    break;
                case "--stats":
                    showStats = true;
                    break;
            }
        }
        
        // Initialize orchestrator
        SpectreOrchestrator orchestrator = new SpectreOrchestrator(
            System.getProperty("user.dir")
        );
        
        // Create logs directory
        try {
            Files.createDirectories(Paths.get("logs"));
        } catch (IOException e) {
            System.err.println("Failed to create logs directory: " + e.getMessage());
        }
        
        if (showStats) {
            Map<String, Object> stats = orchestrator.getProxyStats();
            orchestrator.printPipelineSummary(stats, 0);
            return;
        }
        
        // Run requested pipeline step
        boolean success;
        switch (step) {
            case "scrape":
                success = orchestrator.runGoScraper(limit, protocol);
                break;
            case "polish":
                success = orchestrator.runJavaPolish("raw_proxies.json");
                break;
            case "rotate":
                success = orchestrator.runRustRotator(mode);
                break;
            case "full":
                success = orchestrator.runFullPipeline(limit, protocol, mode);
                break;
            default:
                System.err.println("Unknown step: " + step);
                success = false;
        }
        
        System.exit(success ? 0 : 1);
    }
}import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.logging.*;

/**
 * Spectre Network - Main Integration Class
 * Coordinates the entire polyglot proxy pipeline
 * 
 * Equivalent to spectre.py but implemented in Java
 */
public class SpectreOrchestrator {
    
    private static final Logger LOGGER = Logger.getLogger(SpectreOrchestrator.class.getName());
    
    private final Path workspaceDir;
    private final Path goScraper;
    private final String rustModuleName;
    private final Gson gson;
    
    public SpectreOrchestrator(String workspaceDir) {
        this.workspaceDir = Paths.get(workspaceDir);
        this.goScraper = this.workspaceDir.resolve("go_scraper");
        this.rustModuleName = "rotator_rs";
        this.gson = new GsonBuilder().setPrettyPrinting().create();
        
        // Configure logging
        try {
            Files.createDirectories(Paths.get("logs"));
            FileHandler fileHandler = new FileHandler("logs/spectre.log", true);
            fileHandler.setFormatter(new SimpleFormatter());
            LOGGER.addHandler(fileHandler);
            LOGGER.setLevel(Level.INFO);
        } catch (IOException e) {
            System.err.println("Failed to setup logging: " + e.getMessage());
        }
    }
    
    /**
     * Run the Go proxy scraper
     */
    public boolean runGoScraper(int limit, String protocol) {
        LOGGER.info("Starting Go scraper...");
        
        try {
            ProcessBuilder pb = new ProcessBuilder(
                goScraper.toString(),
                "--limit", String.valueOf(limit),
                "--protocol", protocol
            );
            
            pb.redirectErrorStream(true);
            Process process = pb.start();
            
            // Capture output
            StringBuilder output = new StringBuilder();
            try (BufferedReader reader = new BufferedReader(
                    new InputStreamReader(process.getInputStream()))) {
                String line;
                while ((line = reader.readLine()) != null) {
                    output.append(line).append("\n");
                    LOGGER.fine(line);
                }
            }
            
            // Wait for completion with timeout
            boolean finished = process.waitFor(5, TimeUnit.MINUTES);
            
            if (!finished) {
                process.destroyForcibly();
                LOGGER.severe("Go scraper timed out");
                return false;
            }
            
            if (process.exitValue() == 0) {
                LOGGER.info("Go scraper completed successfully");
                
                // Save raw proxies
                Path rawFile = workspaceDir.resolve("raw_proxies.json");
                Files.writeString(rawFile, output.toString());
                
                return true;
            } else {
                LOGGER.severe("Go scraper failed with exit code: " + process.exitValue());
                return false;
            }
            
        } catch (IOException e) {
            LOGGER.severe("Go scraper IO error: " + e.getMessage());
            return false;
        } catch (InterruptedException e) {
            LOGGER.severe("Go scraper interrupted: " + e.getMessage());
            Thread.currentThread().interrupt();
            return false;
        }
    }
    
    /**
     * Run the Java proxy polisher
     */
    public boolean runJavaPolish(String inputFile) {
        LOGGER.info("Starting Java polisher...");
        
        try {
            ProxyPolish polish = new ProxyPolish(50);
            Map<String, Object> stats = polish.process(inputFile);
            
            if (stats.containsKey("error")) {
                LOGGER.severe("Java polisher failed: " + stats.get("error"));
                return false;
            }
            
            LOGGER.info("Java polisher completed successfully");
            
            // Display summary
            System.out.println("=== Spectre Polish Summary ===");
            System.out.println("Total proxies: " + stats.get("total_proxies"));
            System.out.println("DNS-capable: " + stats.get("dns_proxies"));
            System.out.println("Non-DNS: " + stats.get("non_dns_proxies"));
            System.out.println(String.format("Average latency: %.3fs", stats.get("avg_latency")));
            
            return true;
            
        } catch (Exception e) {
            LOGGER.severe("Java polisher error: " + e.getMessage());
            e.printStackTrace();
            return false;
        }
    }
    
    /**
     * Run the Rust rotator via JNI/JNA
     * Note: This requires Rust library compiled with JNI bindings
     */
    public boolean runRustRotator(String mode) {
        LOGGER.info("Starting Rust rotator (JNI)...");
        
        try {
            // In a real implementation, this would use JNI to call Rust
            // For now, we'll use ProcessBuilder to call a Rust binary
            
            Path rustBinary = workspaceDir.resolve("target/release/rotator_rs");
            
            if (!Files.exists(rustBinary)) {
                LOGGER.warning("Rust binary not found, attempting to build...");
                // Try to build Rust project
                ProcessBuilder buildPb = new ProcessBuilder("cargo", "build", "--release");
                buildPb.directory(workspaceDir.toFile());
                Process buildProcess = buildPb.start();
                buildProcess.waitFor(2, TimeUnit.MINUTES);
            }
            
            // Execute Rust rotator
            ProcessBuilder pb = new ProcessBuilder(
                rustBinary.toString(),
                "--mode", mode,
                "--workspace", workspaceDir.toString()
            );
            
            pb.redirectErrorStream(true);
            Process process = pb.start();
            
            // Capture output
            StringBuilder output = new StringBuilder();
            try (BufferedReader reader = new BufferedReader(
                    new InputStreamReader(process.getInputStream()))) {
                String line;
                while ((line = reader.readLine()) != null) {
                    output.append(line).append("\n");
                }
            }
            
            boolean finished = process.waitFor(30, TimeUnit.SECONDS);
            
            if (!finished) {
                process.destroyForcibly();
                LOGGER.severe("Rust rotator timed out");
                return false;
            }
            
            if (process.exitValue() == 0) {
                LOGGER.info("Rust rotator completed successfully");
                
                // Parse and display decision
                try {
                    JsonObject decision = JsonParser.parseString(output.toString()).getAsJsonObject();
                    displayRustDecision(decision);
                } catch (JsonSyntaxException e) {
                    System.out.println(output.toString());
                }
                
                return true;
            } else {
                LOGGER.severe("Rust rotator failed");
                return false;
            }
            
        } catch (Exception e) {
            LOGGER.severe("Rust rotator error: " + e.getMessage());
            return false;
        }
    }
    
    /**
     * Display Rust rotation decision
     */
    private void displayRustDecision(JsonObject decision) {
        System.out.println("\n=== Spectre Rust Rotator Decision ===");
        System.out.println("Mode: " + decision.get("mode").getAsString());
        System.out.println("Chain ID: " + decision.get("chain_id").getAsString());
        System.out.println("Timestamp: " + decision.get("timestamp").getAsLong());
        
        JsonArray chain = decision.getAsJsonArray("chain");
        JsonArray encryption = decision.getAsJsonArray("encryption");
        
        System.out.println("Chain length: " + chain.size());
        
        for (int i = 0; i < chain.size(); i++) {
            JsonObject hop = chain.get(i).getAsJsonObject();
            JsonObject enc = i < encryption.size() ? encryption.get(i).getAsJsonObject() : null;
            
            System.out.printf(" Hop %d: %s://%s:%d [%s] lat=%.3fs score=%.3f",
                i + 1,
                hop.get("proto").getAsString(),
                hop.get("ip").getAsString(),
                hop.get("port").getAsInt(),
                hop.has("country") ? hop.get("country").getAsString() : "-",
                hop.get("latency").getAsDouble(),
                hop.get("score").getAsDouble()
            );
            
            if (enc != null) {
                String keyHex = enc.get("key_hex").getAsString();
                String nonceHex = enc.get("nonce_hex").getAsString();
                System.out.printf(" key=%s... nonce=%s...",
                    keyHex.substring(0, Math.min(16, keyHex.length())),
                    nonceHex.substring(0, Math.min(12, nonceHex.length()))
                );
            }
            System.out.println();
        }
        
        System.out.printf("Avg latency: %.3fs\n", decision.get("avg_latency").getAsDouble());
        System.out.printf("Score range: %.3f - %.3f\n",
            decision.get("min_score").getAsDouble(),
            decision.get("max_score").getAsDouble()
        );
        System.out.println("=== End Rust Rotator Decision ===\n");
    }
    
    /**
     * Get proxy statistics
     */
    public Map<String, Object> getProxyStats() {
        Map<String, Object> stats = new HashMap<>();
        stats.put("raw_count", 0);
        stats.put("dns_count", 0);
        stats.put("non_dns_count", 0);
        stats.put("combined_count", 0);
        stats.put("avg_latency", 0.0);
        stats.put("avg_score", 0.0);
        
        // Check raw proxies
        Path rawFile = workspaceDir.resolve("raw_proxies.json");
        if (Files.exists(rawFile)) {
            try {
                String content = Files.readString(rawFile);
                JsonArray rawData = JsonParser.parseString(content).getAsJsonArray();
                stats.put("raw_count", rawData.size());
            } catch (Exception e) {
                // Ignore
            }
        }
        
        // Check DNS proxies
        Path dnsFile = workspaceDir.resolve("proxies_dns.json");
        if (Files.exists(dnsFile)) {
            try {
                String content = Files.readString(dnsFile);
                JsonArray dnsData = JsonParser.parseString(content).getAsJsonArray();
                stats.put("dns_count", dnsData.size());
                
                if (dnsData.size() > 0) {
                    double sumLatency = 0.0;
                    double sumScore = 0.0;
                    for (JsonElement elem : dnsData) {
                        JsonObject obj = elem.getAsJsonObject();
                        sumLatency += obj.has("latency") ? obj.get("latency").getAsDouble() : 0.0;
                        sumScore += obj.has("score") ? obj.get("score").getAsDouble() : 0.0;
                    }
                    stats.put("avg_latency", sumLatency / dnsData.size());
                    stats.put("avg_score", sumScore / dnsData.size());
                }
            } catch (Exception e) {
                // Ignore
            }
        }
        
        // Check non-DNS proxies
        Path nonDnsFile = workspaceDir.resolve("proxies_non_dns.json");
        if (Files.exists(nonDnsFile)) {
            try {
                String content = Files.readString(nonDnsFile);
                JsonArray nonDnsData = JsonParser.parseString(content).getAsJsonArray();
                stats.put("non_dns_count", nonDnsData.size());
            } catch (Exception e) {
                // Ignore
            }
        }
        
        // Check combined
        Path combinedFile = workspaceDir.resolve("proxies_combined.json");
        if (Files.exists(combinedFile)) {
            try {
                String content = Files.readString(combinedFile);
                JsonArray combinedData = JsonParser.parseString(content).getAsJsonArray();
                stats.put("combined_count", combinedData.size());
            } catch (Exception e) {
                // Ignore
            }
        }
        
        return stats;
    }
    
    /**
     * Run the complete Spectre pipeline
     */
    public boolean runFullPipeline(int limit, String protocol, String mode) {
        long startTime = System.currentTimeMillis();
        
        LOGGER.info("🕵️  Starting Spectre Network full pipeline...");
        LOGGER.info(String.format("Parameters: limit=%d, protocol=%s, mode=%s", limit, protocol, mode));
        
        // Step 1: Go scraper
        if (!runGoScraper(limit, protocol)) {
            LOGGER.severe("Pipeline failed at Go scraper step");
            return false;
        }
        
        // Step 2: Java polisher
        if (!runJavaPolish("raw_proxies.json")) {
            LOGGER.severe("Pipeline failed at Java polisher step");
            return false;
        }
        
        // Step 3: Rust rotator
        if (!runRustRotator(mode)) {
            LOGGER.severe("Pipeline failed at Rust rotator step");
            return false;
        }
        
        // Step 4: Show statistics
        Map<String, Object> stats = getProxyStats();
        double duration = (System.currentTimeMillis() - startTime) / 1000.0;
        printPipelineSummary(stats, duration);
        
        LOGGER.info("✅ Spectre Network pipeline completed successfully!");
        return true;
    }
    
    /**
     * Print pipeline completion summary
     */
    public void printPipelineSummary(Map<String, Object> stats, double duration) {
        System.out.println("\n" + "=".repeat(60));
        System.out.println("🕵️  SPECTRE NETWORK PIPELINE SUMMARY");
        System.out.println("=".repeat(60));
        System.out.println("📊 Raw Proxies Scraped: " + stats.get("raw_count"));
        System.out.println("🔒 DNS-Capable Proxies: " + stats.get("dns_count"));
        System.out.println("🌐 Non-DNS Proxies: " + stats.get("non_dns_count"));
        System.out.println("📈 Combined Pool: " + stats.get("combined_count"));
        System.out.println(String.format("⚡ Average Latency: %.3fs", stats.get("avg_latency")));
        System.out.println(String.format("🎯 Average Score: %.3f", stats.get("avg_score")));
        System.out.println(String.format("⏱️  Total Duration: %.2fs", duration));
        System.out.println("=".repeat(60));
        
        // Calculate success rates
        int rawCount = (Integer) stats.get("raw_count");
        if (rawCount > 0) {
            int dnsCount = (Integer) stats.get("dns_count");
            int combinedCount = (Integer) stats.get("combined_count");
            double dnsRate = (dnsCount * 100.0) / rawCount;
            double totalRate = (combinedCount * 100.0) / rawCount;
            System.out.println(String.format("📊 DNS Pool Rate: %.1f%%", dnsRate));
            System.out.println(String.format("📊 Total Pool Rate: %.1f%%", totalRate));
        }
    }
    
    /**
     * Main method for command-line execution
     */
    public static void main(String[] args) {
        String mode = "phantom";
        int limit = 500;
        String protocol = "all";
        String step = "full";
        boolean showStats = false;
        
        // Parse command-line arguments
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--mode":
                    if (i + 1 < args.length) mode = args[++i];
                    break;
                case "--limit":
                    if (i + 1 < args.length) limit = Integer.parseInt(args[++i]);
                    break;
                case "--protocol":
                    if (i + 1 < args.length) protocol = args[++i];
                    break;
                case "--step":
                    if (i + 1 < args.length) step = args[++i];
                    break;
                case "--stats":
                    showStats = true;
                    break;
            }
        }
        
        // Initialize orchestrator
        SpectreOrchestrator orchestrator = new SpectreOrchestrator(
            System.getProperty("user.dir")
        );
        
        // Create logs directory
        try {
            Files.createDirectories(Paths.get("logs"));
        } catch (IOException e) {
            System.err.println("Failed to create logs directory: " + e.getMessage());
        }
        
        if (showStats) {
            Map<String, Object> stats = orchestrator.getProxyStats();
            orchestrator.printPipelineSummary(stats, 0);
            return;
        }
        
        // Run requested pipeline step
        boolean success;
        switch (step) {
            case "scrape":
                success = orchestrator.runGoScraper(limit, protocol);
                break;
            case "polish":
                success = orchestrator.runJavaPolish("raw_proxies.json");
                break;
            case "rotate":
                success = orchestrator.runRustRotator(mode);
                break;
            case "full":
                success = orchestrator.runFullPipeline(limit, protocol, mode);
                break;
            default:
                System.err.println("Unknown step: " + step);
                success = false;
        }
        
        System.exit(success ? 0 : 1);
    }
}
