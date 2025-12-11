use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::net::SocketAddr;
use log::{info, debug};
use rotator_rs::types::{RotationDecision, ChainHop};
use anyhow::{Result, Context};

pub async fn start_socks_server(port: u16, decision: RotationDecision) -> Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    info!("👻 Spectre Tunnel (SOCKS5) listening on {}", addr);
    
    let chain_str = decision.chain.iter()
        .map(|h| format!("{}://{}:{}", h.proto, h.ip, h.port))
        .collect::<Vec<_>>()
        .join(" -> ");
    info!("⛓️  Chain: {}", chain_str);

    loop {
        let (client_stream, client_addr) = listener.accept().await?;
        debug!("New connection from {}", client_addr);
        
        let chain = decision.chain.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_socks5_client(client_stream, chain).await {
                debug!("Connection error: {}", e);
            }
        });
    }
}

async fn handle_socks5_client(mut client: TcpStream, chain: Vec<ChainHop>) -> Result<()> {
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
    
    if ver != 0x05 || cmd != 0x01 { // Only support CONNECT (0x01)
        anyhow::bail!("Unsupported SOCKS command");
    }

    let target_addr = match atyp {
        0x01 => { // IPv4
            let mut ip_bytes = [0u8; 4];
            client.read_exact(&mut ip_bytes).await?;
            let mut port_bytes = [0u8; 2];
            client.read_exact(&mut port_bytes).await?;
            let port = u16::from_be_bytes(port_bytes);
            format!("{}.{}.{}.{}:{}", ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3], port)
        }
        0x03 => { // Domain
            let mut len_byte = [0u8; 1];
            client.read_exact(&mut len_byte).await?;
            let len = len_byte[0] as usize;
            let mut domain_bytes = vec![0u8; len];
            client.read_exact(&mut domain_bytes).await?;
            let domain = String::from_utf8(domain_bytes)?;
            let mut port_bytes = [0u8; 2];
            client.read_exact(&mut port_bytes).await?;
            let port = u16::from_be_bytes(port_bytes);
            format!("{}:{}", domain, port)
        }
        _ => anyhow::bail!("Unsupported address type"),
    };

    debug!("Target requested: {}", target_addr);

    // 3. Build the Circuit
    let mut server = build_circuit(&chain, &target_addr).await?;
    
    // 4. Send Success to Client
    // BND.ADDR (0x00 * 4) + BND.PORT (0x00 * 2) - we just send zeros
    client.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
    
    // 5. Pipe Data
    let (mut cr, mut cw) = client.split();
    let (mut sr, mut sw) = server.split();
    
    let client_to_server = tokio::io::copy(&mut cr, &mut sw);
    let server_to_client = tokio::io::copy(&mut sr, &mut cw);
    
    // Use select to wait for either direction to finish/error
    tokio::select! {
        res = client_to_server => res?,
        res = server_to_client => res?,
    };

    Ok(())
}

async fn build_circuit(chain: &[ChainHop], target: &str) -> Result<TcpStream> {
    if chain.is_empty() {
        anyhow::bail!("Empty proxy chain");
    }

    // Connect to the first hop
    let first_hop = &chain[0];
    let addr = format!("{}:{}", first_hop.ip, first_hop.port);
    debug!("Connecting to Hop 1: {}", addr);
    let mut stream = TcpStream::connect(&addr).await
        .context(format!("Failed to connect to first hop {}", addr))?;

    // Handshake with Hop 1
    let next_dest = if chain.len() > 1 {
        // If there are more hops, we tell Hop 1 to connect to Hop 2
        let next = &chain[1];
        format!("{}:{}", next.ip, next.port)
    } else {
        // If this is the last hop, we tell it to connect to Target
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

        debug!("Tunneling through Hop {}: {} -> {}", i + 1, current_hop.ip, next_dest);
        
        // At this point, 'stream' is a tunnel TO current_hop
        // We need to tell current_hop to connect to next_dest
        handshake_proxy(&mut stream, current_hop, &next_dest).await?;
    }

    Ok(stream)
}

async fn handshake_proxy(stream: &mut TcpStream, hop: &ChainHop, target: &str) -> Result<()> {
    match hop.proto.to_lowercase().as_str() {
        "socks5" => {
            // SOCKS5 Handshake
            stream.write_all(&[0x05, 0x01, 0x00]).await?;
            let mut buf = [0u8; 2];
            stream.read_exact(&mut buf).await?;
            if buf[0] != 0x05 || buf[1] != 0x00 {
                anyhow::bail!("SOCKS5 handshake failed with {}", hop.ip);
            }

            // Connect Request
            // 05 01 00 03 (Domain) [len] [domain] [port]
            // We use Domain type (0x03) for flexibility so we don't have to resolve DNS locally
            let (host, port_str) = target.rsplit_once(':').unwrap_or((target, "80"));
            let port: u16 = port_str.parse().unwrap_or(80);
            
            let mut req = vec![0x05, 0x01, 0x00, 0x03];
            req.push(host.len() as u8);
            req.extend_from_slice(host.as_bytes());
            req.extend_from_slice(&port.to_be_bytes());
            
            stream.write_all(&req).await?;

            // Read Response
            let mut head = [0u8; 4];
            stream.read_exact(&mut head).await?;
            if head[1] != 0x00 {
                anyhow::bail!("SOCKS5 connect failed on {}", hop.ip);
            }
            
            // Consume rest of response (BND.ADDR/PORT)
            let atyp = head[3];
            match atyp {
                0x01 => { let mut b = [0u8; 6]; stream.read_exact(&mut b).await?; } // IPv4 + Port
                0x03 => { // Domain
                    let mut len = [0u8; 1];
                    stream.read_exact(&mut len).await?;
                    let mut b = vec![0u8; len[0] as usize + 2];
                    stream.read_exact(&mut b).await?;
                }
                0x04 => { let mut b = [0u8; 18]; stream.read_exact(&mut b).await?; } // IPv6
                _ => {} 
            }
        }
        "http" | "https" => {
            // HTTP CONNECT
            let req = format!("CONNECT {} HTTP/1.1\r\nHost: {}\r\n\r\n", target, target);
            stream.write_all(req.as_bytes()).await?;

            // Read Response (look for 200 OK)
            // Simplified reader: read until \r\n\r\n
            let mut header_buf = Vec::new();
            loop {
                let byte = stream.read_u8().await?;
                header_buf.push(byte);
                if header_buf.len() >= 4 && &header_buf[header_buf.len()-4..] == b"\r\n\r\n" {
                    break;
                }
                if header_buf.len() > 4096 {
                     anyhow::bail!("HTTP CONNECT header too large");
                }
            }
            
            let response = String::from_utf8_lossy(&header_buf);
            if !response.contains("200 Connection established") && !response.contains("200 OK") {
                 anyhow::bail!("HTTP CONNECT failed on {}", hop.ip);
            }
        }
        _ => anyhow::bail!("Unknown protocol: {}", hop.proto),
    }
    Ok(())
}
