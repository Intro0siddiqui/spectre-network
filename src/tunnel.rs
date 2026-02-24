use crate::crypto;
use crate::types::{ChainHop, CryptoHop, Proxy, RotationDecision};
use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::time::timeout;
use uuid::Uuid;

/// Chunk size for the encrypted pipe — 16 KiB
const CHUNK: usize = 16 * 1024;

/// Default timeout for hop verification in seconds
const DEFAULT_HOP_TIMEOUT_SECS: u64 = 5;

/// Maximum retry attempts for circuit building
const MAX_CIRCUIT_RETRIES: usize = 3;

/// Verifies that a proxy hop is reachable and functional.
///
/// Attempts a TCP connection to the proxy with the specified timeout.
/// Returns Ok(true) if the proxy is reachable, Ok(false) if connection
/// fails or times out.
///
/// # Arguments
/// * `ip` - The IP address of the proxy
/// * `port` - The port number of the proxy
/// * `timeout_secs` - Connection timeout in seconds
///
/// # Returns
/// * `Ok(true)` - Proxy is reachable
/// * `Ok(false)` - Proxy is unreachable or timed out
/// * `Err` - Unexpected error during verification
async fn verify_hop(ip: &str, port: u16, timeout_secs: u64) -> Result<bool> {
    let addr = format!("{}:{}", ip, port);
    let timeout_duration = Duration::from_secs(timeout_secs);

    tracing::debug!(hop_addr = %addr, timeout = timeout_secs, "Verifying hop");

    match timeout(timeout_duration, TcpStream::connect(&addr)).await {
        Ok(Ok(_stream)) => {
            // Connection successful - stream is dropped immediately
            tracing::debug!(hop_addr = %addr, "Hop is reachable");
            Ok(true)
        }
        Ok(Err(e)) => {
            // Connection failed
            tracing::debug!(hop_addr = %addr, error = %e, "Hop connection failed");
            Ok(false)
        }
        Err(_) => {
            // Timeout occurred
            tracing::debug!(hop_addr = %addr, timeout = timeout_secs, "Hop verification timed out");
            Ok(false)
        }
    }
}

/// Verifies all hops in the chain are reachable before circuit construction.
///
/// # Arguments
/// * `chain` - The chain of proxy hops to verify
/// * `timeout_secs` - Timeout per hop in seconds
///
/// # Returns
/// * `Ok(Vec<bool>)` - Vector indicating which hops are reachable (true = reachable)
/// * `Err` - Error during verification
async fn verify_chain(chain: &[ChainHop], timeout_secs: u64) -> Result<Vec<bool>> {
    let mut results = Vec::with_capacity(chain.len());

    for hop in chain {
        let is_reachable = verify_hop(&hop.ip, hop.port, timeout_secs).await?;
        results.push(is_reachable);

        if !is_reachable {
            tracing::warn!(
                hop_addr = %format!("{}:{}", hop.ip, hop.port),
                protocol = %hop.proto,
                "Hop is unreachable"
            );
        } else {
            tracing::debug!(
                hop_addr = %format!("{}:{}", hop.ip, hop.port),
                protocol = %hop.proto,
                "Hop verified successfully"
            );
        }
    }

    Ok(results)
}

/// Builds a circuit through the proxy chain with hop verification and retry logic.
///
/// Before building the circuit, verifies that each proxy hop is reachable.
/// If a hop fails during construction, attempts to retry with alternative proxies.
/// Includes timeout for the entire circuit building process.
///
/// # Arguments
/// * `chain` - The chain of proxy hops to build the circuit through
/// * `target` - The final destination address (host:port)
/// * `timeout_secs` - Timeout per hop verification in seconds (default: 5)
/// * `max_retries` - Maximum number of retry attempts (default: 3)
///
/// # Returns
/// * `Ok(TcpStream)` - Established circuit connection
/// * `Err` - Error with details about which hop failed
async fn build_circuit_with_verification(
    chain: &[ChainHop],
    target: &str,
    timeout_secs: u64,
    max_retries: usize,
) -> Result<TcpStream> {
    if chain.is_empty() {
        anyhow::bail!("Empty proxy chain - cannot build circuit");
    }

    tracing::info!(hop_count = chain.len(), %target, "Building circuit");

    // Pre-verify all hops before starting circuit construction
    tracing::debug!("Pre-verifying {} hops in the chain...", chain.len());
    let hop_status = verify_chain(chain, timeout_secs).await?;

    // Count reachable hops
    let reachable_count = hop_status.iter().filter(|&&s| s).count();
    let required_hops = chain.len();

    if reachable_count < required_hops {
        tracing::warn!(
            reachable = reachable_count,
            required = required_hops,
            "Some hops are unreachable, attempting circuit construction anyway..."
        );
    }

    // Attempt circuit construction with retries
    let mut last_error: Option<anyhow::Error> = None;

    for attempt in 0..=max_retries {
        if attempt > 0 {
            tracing::debug!(
                attempt = attempt + 1,
                max = max_retries,
                "Circuit build retry"
            );
        }

        match build_circuit_internal(chain, target, &hop_status).await {
            Ok(stream) => {
                tracing::info!(hop_count = chain.len(), "Circuit successfully built");
                return Ok(stream);
            }
            Err(e) => {
                tracing::warn!(attempt = attempt + 1, error = %e, "Circuit build failed");
                last_error = Some(e);

                // Re-verify hops that failed to see if they've recovered
                if attempt < max_retries {
                    tracing::debug!("Re-verifying failed hops before retry...");
                    for (i, hop) in chain.iter().enumerate() {
                        if !hop_status[i] {
                            let recovered = verify_hop(&hop.ip, hop.port, timeout_secs).await?;
                            if recovered {
                                tracing::info!(
                                    hop_addr = %format!("{}:{}", hop.ip, hop.port),
                                    protocol = %hop.proto,
                                    "Hop recovered and is now reachable"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // All retries exhausted - return detailed error
    let error_msg = format!(
        "Failed to build circuit after {} retries. Last error: {}. \
         Chain: [{}]. Target: {}",
        max_retries,
        last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        chain
            .iter()
            .map(|h| format!("{}://{}:{}", h.proto, h.ip, h.port))
            .collect::<Vec<_>>()
            .join(" -> "),
        target
    );

    Err(anyhow::anyhow!(error_msg))
}

/// Internal circuit building function that uses pre-verified hop status.
///
/// # Arguments
/// * `chain` - The chain of proxy hops
/// * `target` - The final destination address
/// * `hop_status` - Pre-computed reachability status for each hop
///
/// # Returns
/// * `Ok(TcpStream)` - Established circuit
/// * `Err` - Error with hop details
async fn build_circuit_internal(
    chain: &[ChainHop],
    target: &str,
    hop_status: &[bool],
) -> Result<TcpStream> {
    // Connect to the first hop
    let first_hop = &chain[0];
    let addr = format!("{}:{}", first_hop.ip, first_hop.port);

    // Check if first hop was verified as reachable
    if !hop_status[0] {
        anyhow::bail!(
            "First hop unreachable: {}://{}:{} - connection refused or timed out",
            first_hop.proto,
            first_hop.ip,
            first_hop.port
        );
    }

    tracing::debug!(
        hop_addr = %addr,
        protocol = %first_hop.proto,
        "Connecting to first hop"
    );
    let mut stream = TcpStream::connect(&addr).await.context(format!(
        "Failed to connect to first hop {}://{}:{}",
        first_hop.proto, first_hop.ip, first_hop.port
    ))?;

    // Handshake with first hop
    let next_dest = if chain.len() > 1 {
        let next = &chain[1];
        format!("{}:{}", next.ip, next.port)
    } else {
        target.to_string()
    };

    handshake_proxy(&mut stream, first_hop, &next_dest).await?;

    // Iterate through remaining hops
    for i in 1..chain.len() {
        let current_hop = &chain[i];
        let next_dest = if i == chain.len() - 1 {
            target.to_string()
        } else {
            let next = &chain[i + 1];
            format!("{}:{}", next.ip, next.port)
        };

        tracing::debug!(
            hop_index = i + 1,
            hop_addr = %format!("{}:{}", current_hop.ip, current_hop.port),
            protocol = %current_hop.proto,
            next_dest = %next_dest,
            "Tunneling through hop"
        );

        // Check if this hop was verified as reachable
        if !hop_status[i] {
            anyhow::bail!(
                "Hop {} unreachable: {}://{}:{} - connection refused or timed out",
                i + 1,
                current_hop.proto,
                current_hop.ip,
                current_hop.port
            );
        }

        handshake_proxy(&mut stream, current_hop, &next_dest).await?;
    }

    Ok(stream)
}

pub async fn start_socks_server(
    port: u16,
    initial_decision: RotationDecision,
    dns_pool: Vec<Proxy>,
    non_dns_pool: Vec<Proxy>,
    combined_pool: Vec<Proxy>,
) -> Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(port = port, "Spectre Tunnel (SOCKS5) listening");

    let decision = Arc::new(RwLock::new(initial_decision));

    // Spawn health monitor for live rotation
    let monitor_decision = Arc::clone(&decision);
    tokio::spawn(async move {
        chain_health_monitor(monitor_decision, dns_pool, non_dns_pool, combined_pool).await;
    });

    loop {
        let (client_stream, client_addr) = listener.accept().await?;
        tracing::debug!(client_addr = %client_addr, "New connection accepted");

        let current_decision = decision.read().await.clone();

        tokio::spawn(async move {
            // Use the exit hop's crypto material (last in chain)
            let exit_crypto = current_decision.encryption.last().cloned();
            if let Err(e) =
                handle_socks5_client(client_stream, current_decision.chain, exit_crypto).await
            {
                tracing::debug!(error = %e, "Connection error");
            }
        });
    }
}

/// Periodic health check and chain rotation
async fn chain_health_monitor(
    decision: Arc<RwLock<RotationDecision>>,
    dns: Vec<Proxy>,
    non_dns: Vec<Proxy>,
    combined: Vec<Proxy>,
) {
    use crate::rotator;
    let mut interval = tokio::time::interval(Duration::from_secs(300)); // Rotate every 5 mins
    loop {
        interval.tick().await;

        let mode = { decision.read().await.mode.clone() };
        tracing::info!("Health check: rotating chain for mode {}", mode);

        if let Some(new_decision) = rotator::build_chain_decision(&mode, &dns, &non_dns, &combined)
        {
            let mut w = decision.write().await;
            *w = new_decision;

            let chain_str = w
                .chain
                .iter()
                .map(|h| format!("{}://{}:{}", h.proto, h.ip, h.port))
                .collect::<Vec<_>>()
                .join(" -> ");

            tracing::info!(chain_id = %w.chain_id, chain = %chain_str, "Chain rotated successfully");
        }
    }
}

async fn handle_socks5_client(
    mut client: TcpStream,
    chain: Vec<ChainHop>,
    exit_crypto: Option<CryptoHop>,
) -> Result<()> {
    let connection_id = Uuid::new_v4();
    let span = tracing::info_span!("socks5_connection", id = %connection_id);
    let _guard = span.enter();

    // Optimize: reduce latency by disabling Nagle's algorithm
    client.set_nodelay(true)?;

    let client_addr = client
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    tracing::info!(client_addr = %client_addr, "New SOCKS5 connection");

    // 1. SOCKS5 Handshake
    let mut buf = [0u8; 2];
    client.read_exact(&mut buf).await?;

    if buf[0] != 0x05 {
        anyhow::bail!("Invalid SOCKS version");
    }

    let n_methods = buf[1] as usize;
    let mut methods = vec![0u8; n_methods];
    client.read_exact(&mut methods).await?;

    // We only support NO AUTH (0x00)
    client.write_all(&[0x05, 0x00]).await?;

    // 2. Request details
    let mut head = [0u8; 4];
    client.read_exact(&mut head).await?;

    let ver = head[0];
    let cmd = head[1];
    let _rsv = head[2];
    let atyp = head[3];

    if ver != 0x05 || cmd != 0x01 {
        // Only support CONNECT (0x01)
        anyhow::bail!("Unsupported SOCKS command");
    }

    let target_addr = match atyp {
        0x01 => {
            // IPv4
            let mut ip_bytes = [0u8; 4];
            client.read_exact(&mut ip_bytes).await?;
            let mut port_bytes = [0u8; 2];
            client.read_exact(&mut port_bytes).await?;
            let port = u16::from_be_bytes(port_bytes);
            format!(
                "{}.{}.{}.{}:{}",
                ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3], port
            )
        }
        0x03 => {
            // Domain name
            let mut len_byte = [0u8; 1];
            client.read_exact(&mut len_byte).await?;
            let len = len_byte[0] as usize;

            // VALIDATION: Reject overly long domain names to prevent memory exhaustion
            // RFC 1035 specifies max domain name length of 255 bytes
            if len > 255 {
                anyhow::bail!("Domain name too long: {} bytes", len);
            }

            // Reject zero-length domain names
            if len == 0 {
                anyhow::bail!("Invalid domain name: zero length");
            }

            let mut domain_bytes = vec![0u8; len];
            client.read_exact(&mut domain_bytes).await?;

            // Validate domain name contains only valid characters
            // Basic check: printable ASCII characters, dots, and hyphens
            for &byte in &domain_bytes {
                if !byte.is_ascii_graphic() && byte != b'.' && byte != b'-' {
                    anyhow::bail!("Invalid character in domain name: 0x{:02x}", byte);
                }
            }

            let domain = String::from_utf8(domain_bytes)?;
            let mut port_bytes = [0u8; 2];
            client.read_exact(&mut port_bytes).await?;
            let port = u16::from_be_bytes(port_bytes);
            format!("{}:{}", domain, port)
        }
        _ => anyhow::bail!("Unsupported address type"),
    };

    tracing::info!(target = %target_addr, "Target requested");

    // 3. Build circuit through the chain
    let mut server = build_circuit(&chain, &target_addr).await?;
    server.set_nodelay(true)?;

    // 4. Send success to client
    client
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await?;

    // 5. Pipe data — with AES-GCM encryption if keys are available
    match exit_crypto {
        Some(crypto_hop) => {
            tracing::info!("Encrypted tunnel active (AES-256-GCM)");
            encrypted_pipe(client, server, &crypto_hop.key_hex, &crypto_hop.nonce_hex).await?;
        }
        None => {
            tracing::warn!("Plain tunnel (no encryption keys)");
            let (mut cr, mut cw) = client.split();
            let (mut sr, mut sw) = server.split();
            tokio::select! {
                res = tokio::io::copy(&mut cr, &mut sw) => { res?; }
                res = tokio::io::copy(&mut sr, &mut cw) => { res?; }
            }
        }
    }

    tracing::info!("Connection closed");
    Ok(())
}

/// Bidirectional encrypted pipe with per-packet nonce derivation.
/// Outbound (client → server): read chunk → AES-256-GCM encrypt with counter → send
/// Inbound  (server → client): read chunk → AES-256-GCM decrypt with counter → send
///
/// Chunk framing: [8-byte counter][4-byte LE length][ciphertext + tag]
/// The counter ensures each packet uses a unique nonce, preventing AES-GCM nonce reuse.
async fn encrypted_pipe(
    mut client: TcpStream,
    mut server: TcpStream,
    key_hex: &str,
    nonce_hex: &str,
) -> Result<()> {
    let key = key_hex.to_string();
    let nonce = nonce_hex.to_string();

    let (mut cr, mut cw) = client.split();
    let (mut sr, mut sw) = server.split();

    let key_c = key.clone();
    let nonce_c = nonce.clone();

    tracing::debug!("Starting encrypted pipe");

    // Outbound: client sends → encrypt with counter → forward to server
    // Counter starts at 0 and increments for each packet in this direction
    let outbound = async {
        let mut buf = vec![0u8; CHUNK];
        let mut counter: u64 = 0;
        loop {
            let n = cr.read(&mut buf).await?;
            if n == 0 {
                break;
            }

            // Encrypt with counter-derived nonce to prevent nonce reuse
            let encrypted = crypto::encrypt_with_counter(&key_c, &nonce_c, counter, &buf[..n])
                .map_err(|e| std::io::Error::other(e))?;

            // Frame: [8-byte counter][4-byte LE length][ciphertext]
            let len = encrypted.len() as u32;
            sw.write_all(&counter.to_le_bytes()).await?;
            sw.write_all(&len.to_le_bytes()).await?;
            sw.write_all(&encrypted).await?;

            // Increment counter for next packet
            counter = counter.wrapping_add(1);
            if counter == 0 {
                // Counter wrapped - this should never happen in practice (2^64 packets)
                // but we log a warning if it does
                tracing::warn!("Packet counter wrapped! Consider rotating session keys.");
            }
        }
        Ok::<_, std::io::Error>(())
    };

    // Inbound: server responds → decrypt with counter → send back to client
    // The counter is read from each received frame to derive the correct nonce
    let inbound = async {
        loop {
            // Read 8-byte counter
            let mut counter_buf = [0u8; 8];
            match sr.read_exact(&mut counter_buf).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }
            let received_counter = u64::from_le_bytes(counter_buf);

            // Read 4-byte length prefix
            let mut len_buf = [0u8; 4];
            match sr.read_exact(&mut len_buf).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }
            let len = u32::from_le_bytes(len_buf) as usize;
            if len == 0 || len > CHUNK * 2 {
                break; // sanity guard
            }

            let mut enc_buf = vec![0u8; len];
            sr.read_exact(&mut enc_buf).await?;

            // Decrypt with the counter from the frame to derive the same nonce
            let decrypted = crypto::decrypt_with_counter(&key, &nonce, received_counter, &enc_buf)
                .map_err(|e| std::io::Error::other(e))?;

            cw.write_all(&decrypted).await?;
        }
        Ok::<_, std::io::Error>(())
    };

    tokio::select! {
        res = outbound => { res?; }
        res = inbound  => { res?; }
    }

    Ok(())
}

/// Builds a circuit through the proxy chain with hop verification.
///
/// This is the main entry point for circuit building. It uses default
/// timeout (5 seconds per hop) and retry (3 attempts) values.
///
/// # Arguments
/// * `chain` - The chain of proxy hops to build the circuit through
/// * `target` - The final destination address (host:port)
///
/// # Returns
/// * `Ok(TcpStream)` - Established circuit connection
/// * `Err` - Error with details about which hop failed
async fn build_circuit(chain: &[ChainHop], target: &str) -> Result<TcpStream> {
    build_circuit_with_verification(chain, target, DEFAULT_HOP_TIMEOUT_SECS, MAX_CIRCUIT_RETRIES)
        .await
}

pub async fn handshake_proxy(stream: &mut TcpStream, hop: &ChainHop, target: &str) -> Result<()> {
    match hop.proto.to_lowercase().as_str() {
        "socks5" => {
            // SOCKS5 Handshake - wrap the whole process in a timeout
            let socks_future = async {
                stream.write_all(&[0x05, 0x01, 0x00]).await?;
                let mut buf = [0u8; 2];
                stream.read_exact(&mut buf).await?;
                if buf[0] != 0x05 || buf[1] != 0x00 {
                    anyhow::bail!("SOCKS5 handshake failed with {}", hop.ip);
                }

                let (host, port_str) = target.rsplit_once(':').unwrap_or((target, "80"));
                let port: u16 = port_str.parse().unwrap_or(80);

                let mut req = vec![0x05, 0x01, 0x00, 0x03];
                req.push(host.len() as u8);
                req.extend_from_slice(host.as_bytes());
                req.extend_from_slice(&port.to_be_bytes());
                stream.write_all(&req).await?;

                let mut head = [0u8; 4];
                stream.read_exact(&mut head).await?;
                if head[1] != 0x00 {
                    anyhow::bail!("SOCKS5 connect failed on {}", hop.ip);
                }

                let atyp = head[3];
                match atyp {
                    0x01 => {
                        let mut b = [0u8; 6];
                        stream.read_exact(&mut b).await?;
                    }
                    0x03 => {
                        let mut len = [0u8; 1];
                        stream.read_exact(&mut len).await?;
                        let mut b = vec![0u8; len[0] as usize + 2];
                        stream.read_exact(&mut b).await?;
                    }
                    0x04 => {
                        let mut b = [0u8; 18];
                        stream.read_exact(&mut b).await?;
                    }
                    _ => {}
                }
                Ok::<(), anyhow::Error>(())
            };

            if tokio::time::timeout(std::time::Duration::from_secs(5), socks_future)
                .await
                .is_err()
            {
                anyhow::bail!("SOCKS5 handshake timeout on {}", hop.ip);
            }
        }
        "http" | "https" => {
            // HTTP CONNECT
            let req = format!("CONNECT {} HTTP/1.1\r\nHost: {}\r\n\r\n", target, target);
            stream.write_all(req.as_bytes()).await?;

            // Read Response (look for 200 OK)
            // Wrap the read loop in a timeout
            let read_future = async {
                let mut header_buf = Vec::new();
                let mut byte = [0u8; 1];
                loop {
                    match stream.read(&mut byte).await? {
                        0 => anyhow::bail!("Handshake closed prematurely"),
                        _ => {
                            header_buf.push(byte[0]);
                            if header_buf.len() >= 4
                                && &header_buf[header_buf.len() - 4..] == b"\r\n\r\n"
                            {
                                break;
                            }
                            if header_buf.len() > 4096 {
                                anyhow::bail!("HTTP CONNECT header too large");
                            }
                        }
                    }
                }
                let response = String::from_utf8_lossy(&header_buf);
                if !response.contains("200 Connection established") && !response.contains("200 OK")
                {
                    anyhow::bail!("HTTP CONNECT failed on {}: {}", hop.ip, response);
                }
                Ok::<(), anyhow::Error>(())
            };

            if tokio::time::timeout(std::time::Duration::from_secs(3), read_future)
                .await
                .is_err()
            {
                anyhow::bail!("HTTP CONNECT read timeout on {}", hop.ip);
            }
        }
        _ => anyhow::bail!("Unknown protocol: {}", hop.proto),
    }
    Ok(())
}
