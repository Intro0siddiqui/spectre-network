package main

/*
#include <stdlib.h>
extern unsigned char* encrypt_with_counter_c(const char* key_hex, const char* nonce_hex, unsigned long long counter, const unsigned char* plaintext, size_t plaintext_len, size_t* out_len);
extern unsigned char* decrypt_with_counter_c(const char* key_hex, const char* nonce_hex, unsigned long long counter, const unsigned char* ciphertext, size_t ciphertext_len, size_t* out_len);
extern void free_byte_array(unsigned char* ptr, size_t len);
*/
import "C"

import (
	"encoding/binary"
	"fmt"
	"io"
	"net"
	"strconv"
	"strings"
	"sync"
	"time"
	"unsafe"
)

const (
	ChunkSize = 16 * 1024
)

// encryptedPipe pumps data between client and server with AES-GCM encryption.
func encryptedPipe(client, server net.Conn, keyHex, nonceHex string) error {
	errCh := make(chan error, 2)

	// Outbound: client -> server (encrypt)
	go func() {
		buf := make([]byte, ChunkSize)
		var counter uint64 = 0
		for {
			n, err := client.Read(buf)
			if n > 0 {
				encrypted, err := encryptWithCounter(keyHex, nonceHex, counter, buf[:n])
				if err != nil {
					errCh <- err
					return
				}

				// Frame: [8-byte counter][4-byte LE length][ciphertext]
				frameHead := make([]byte, 12)
				binary.LittleEndian.PutUint64(frameHead[0:8], counter)
				binary.LittleEndian.PutUint32(frameHead[8:12], uint32(len(encrypted)))
				
				if _, err := server.Write(frameHead); err != nil {
					errCh <- err
					return
				}
				if _, err := server.Write(encrypted); err != nil {
					errCh <- err
					return
				}
				counter++
			}
			if err != nil {
				if err != io.EOF {
					errCh <- err
				} else {
					errCh <- nil
				}
				return
			}
		}
	}()

	// Inbound: server -> client (decrypt)
	go func() {
		var counter uint64
		for {
			// Read 8-byte counter
			counterBuf := make([]byte, 8)
			if _, err := io.ReadFull(server, counterBuf); err != nil {
				if err != io.EOF {
					errCh <- err
				} else {
					errCh <- nil
				}
				return
			}
			receivedCounter := binary.LittleEndian.Uint64(counterBuf)

			// Read 4-byte length
			lenBuf := make([]byte, 4)
			if _, err := io.ReadFull(server, lenBuf); err != nil {
				errCh <- err
				return
			}
			length := binary.LittleEndian.Uint32(lenBuf)
			if length == 0 || length > ChunkSize*2 {
				errCh <- fmt.Errorf("invalid frame length: %d", length)
				return
			}

			// Read ciphertext
			ciphertext := make([]byte, length)
			if _, err := io.ReadFull(server, ciphertext); err != nil {
				errCh <- err
				return
			}

			// Decrypt
			decrypted, err := decryptWithCounter(keyHex, nonceHex, receivedCounter, ciphertext)
			if err != nil {
				errCh <- err
				return
			}

			if _, err := client.Write(decrypted); err != nil {
				errCh <- err
				return
			}
			counter++
		}
	}()

	// Wait for any side to finish or error
	return <-errCh
}

func encryptWithCounter(keyHex, nonceHex string, counter uint64, plaintext []byte) ([]byte, error) {
	cKey := C.CString(keyHex)
	defer C.free(unsafe.Pointer(cKey))
	cNonce := C.CString(nonceHex)
	defer C.free(unsafe.Pointer(cNonce))

	var outLen C.size_t
	cOut := C.encrypt_with_counter_c(
		cKey,
		cNonce,
		C.ulonglong(counter),
		(*C.uchar)(unsafe.Pointer(&plaintext[0])),
		C.size_t(len(plaintext)),
		&outLen,
	)

	if cOut == nil {
		return nil, fmt.Errorf("encryption failed in Rust")
	}
	defer C.free_byte_array((*C.uchar)(cOut), outLen)

	return C.GoBytes(unsafe.Pointer(cOut), C.int(outLen)), nil
}

func decryptWithCounter(keyHex, nonceHex string, counter uint64, ciphertext []byte) ([]byte, error) {
	cKey := C.CString(keyHex)
	defer C.free(unsafe.Pointer(cKey))
	cNonce := C.CString(nonceHex)
	defer C.free(unsafe.Pointer(cNonce))

	var outLen C.size_t
	cOut := C.decrypt_with_counter_c(
		cKey,
		cNonce,
		C.ulonglong(counter),
		(*C.uchar)(unsafe.Pointer(&ciphertext[0])),
		C.size_t(len(ciphertext)),
		&outLen,
	)

	if cOut == nil {
		return nil, fmt.Errorf("decryption failed in Rust")
	}
	defer C.free_byte_array((*C.uchar)(cOut), outLen)

	return C.GoBytes(unsafe.Pointer(cOut), C.int(outLen)), nil
}

// startSOCKS5Server starts the SOCKS5 server with live rotation.
func startSOCKS5Server(port int, initialDecision RotationDecision, dnsPool, nonDNSPool, combinedPool []Proxy) error {
	addr := fmt.Sprintf("127.0.0.1:%d", port)
	listener, err := net.Listen("tcp", addr)
	if err != nil {
		return err
	}
	defer listener.Close()
	fmt.Printf("%s Spectre Tunnel (SOCKS5) listening on %s\n", col(green, "✓"), addr)

	// Protected by a mutex for live rotation
	var mu sync.RWMutex
	currentDecision := initialDecision

	// Spawn health monitor for live rotation
	go func() {
		ticker := time.NewTicker(5 * time.Minute)
		defer ticker.Stop()
		for range ticker.C {
			mu.RLock()
			mode := currentDecision.Mode
			mu.RUnlock()

			fmt.Printf("%s Health check: rotating chain for mode %s\n", col(cyan, "◈"), mode)
			newDecision, err := buildChainDecision(mode, dnsPool, nonDNSPool, combinedPool)
			if err == nil && newDecision != nil {
				mu.Lock()
				currentDecision = *newDecision
				mu.Unlock()
				fmt.Printf("%s Chain rotated successfully: %s\n", col(green, "✓"), newDecision.ChainID[:12]+"…")
			}
		}
	}()

	for {
		client, err := listener.Accept()
		if err != nil {
			fmt.Printf("%s Accept error: %v\n", col(red, "✗"), err)
			continue
		}

		mu.RLock()
		d := currentDecision
		mu.RUnlock()

		go func(c net.Conn, d RotationDecision) {
			if err := handleSOCKS5Client(c, d, dnsPool, nonDNSPool, combinedPool); err != nil {
				// Silently log or handle connection errors
			}
		}(client, d)
	}
}

// handleSOCKS5Client handles the initial SOCKS5 handshake and request parsing.
func handleSOCKS5Client(conn net.Conn, decision RotationDecision, dnsPool, nonDNSPool, combinedPool []Proxy) error {
	defer conn.Close()

	// 1. SOCKS5 Handshake
	// Read version and nmethods
	buf := make([]byte, 2)
	if _, err := io.ReadFull(conn, buf); err != nil {
		return err
	}
	if buf[0] != 0x05 {
		return fmt.Errorf("invalid SOCKS version: %d", buf[0])
	}

	nmethods := int(buf[1])
	methods := make([]byte, nmethods)
	if _, err := io.ReadFull(conn, methods); err != nil {
		return err
	}

	// We only support NO AUTH (0x00)
	foundNoAuth := false
	for _, m := range methods {
		if m == 0x00 {
			foundNoAuth = true
			break
		}
	}

	if !foundNoAuth {
		conn.Write([]byte{0x05, 0xFF}) // No acceptable methods
		return fmt.Errorf("no acceptable SOCKS5 auth methods")
	}

	if _, err := conn.Write([]byte{0x05, 0x00}); err != nil {
		return err
	}

	// 2. Request details
	head := make([]byte, 4)
	if _, err := io.ReadFull(conn, head); err != nil {
		return err
	}

	ver := head[0]
	cmd := head[1]
	// rsv := head[2]
	atyp := head[3]

	if ver != 0x05 || cmd != 0x01 {
		// Only support CONNECT (0x01)
		return fmt.Errorf("unsupported SOCKS command: %d", cmd)
	}

	var targetAddr string
	switch atyp {
	case 0x01: // IPv4
		ipBytes := make([]byte, 4)
		if _, err := io.ReadFull(conn, ipBytes); err != nil {
			return err
		}
		portBytes := make([]byte, 2)
		if _, err := io.ReadFull(conn, portBytes); err != nil {
			return err
		}
		port := uint16(portBytes[0])<<8 | uint16(portBytes[1])
		targetAddr = fmt.Sprintf("%d.%d.%d.%d:%d", ipBytes[0], ipBytes[1], ipBytes[2], ipBytes[3], port)

	case 0x03: // Domain name
		lenByte := make([]byte, 1)
		if _, err := io.ReadFull(conn, lenByte); err != nil {
			return err
		}
		length := int(lenByte[0])
		domainBytes := make([]byte, length)
		if _, err := io.ReadFull(conn, domainBytes); err != nil {
			return err
		}
		portBytes := make([]byte, 2)
		if _, err := io.ReadFull(conn, portBytes); err != nil {
			return err
		}
		port := uint16(portBytes[0])<<8 | uint16(portBytes[1])
		targetAddr = fmt.Sprintf("%s:%d", string(domainBytes), port)

	default:
		return fmt.Errorf("unsupported SOCKS5 address type: %d", atyp)
	}

	fmt.Printf("%s Target requested: %s\n", col(cyan, "◈"), targetAddr)

	// 3. Build circuit through the chain
	server, err := buildCircuit(decision.Chain, targetAddr, dnsPool, nonDNSPool, combinedPool, decision.Mode)
	if err != nil {
		fmt.Printf("%s Failed to build circuit: %v\n", col(red, "✗"), err)
		return fmt.Errorf("failed to build circuit: %v", err)
	}
	defer server.Close()
	fmt.Printf("%s Circuit built successfully to %s\n", col(green, "✓"), targetAddr)

	// 4. Send success to client
	if _, err := conn.Write([]byte{0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0}); err != nil {
		return err
	}

	// 5. Pipe data — with AES-GCM encryption if keys are available
	if len(decision.Encryption) > 0 {
		// Use the exit hop's crypto material (last in chain)
		exitCrypto := decision.Encryption[len(decision.Encryption)-1]
		return encryptedPipe(conn, server, exitCrypto.KeyHex, exitCrypto.NonceHex)
	}

	// Fallback to plain pipe
	errCh := make(chan error, 2)
	go func() {
		_, err := io.Copy(server, conn)
		errCh <- err
	}()
	go func() {
		_, err := io.Copy(conn, server)
		errCh <- err
	}()
	return <-errCh
}

// buildCircuit builds a multi-hop proxy circuit with retries and live rotation.
func buildCircuit(chain []ChainHop, target string, dnsPool, nonDNSPool, combinedPool []Proxy, mode string) (net.Conn, error) {
	if len(chain) == 0 {
		return nil, fmt.Errorf("empty proxy chain")
	}

	maxRetries := 3
	var lastErr error
	currentChain := chain

	for attempt := 0; attempt < maxRetries; attempt++ {
		if attempt > 0 {
			fmt.Printf("%s Circuit build attempt %d/%d (rotating proxies)...\n", col(dim, "→"), attempt+1, maxRetries)
			// Rotate the chain on failure
			newDecision, err := buildChainDecision(mode, dnsPool, nonDNSPool, combinedPool)
			if err == nil && newDecision != nil {
				currentChain = newDecision.Chain
			}
		}

		conn, err := buildCircuitInternal(currentChain, target)
		if err == nil {
			return conn, nil
		}
		
		lastErr = err
		fmt.Printf("%s Attempt %d failed: %v\n", col(yellow, "⚠"), attempt+1, err)
		time.Sleep(500 * time.Millisecond)
	}

	return nil, fmt.Errorf("all retries failed: %v", lastErr)
}

func buildCircuitInternal(chain []ChainHop, target string) (net.Conn, error) {
	fmt.Printf("%s Building circuit through %d hops to %s\n", col(dim, "→"), len(chain), target)

	// Connect to first hop
	first := chain[0]
	addr := fmt.Sprintf("%s:%d", first.IP, first.Port)
	conn, err := net.DialTimeout("tcp", addr, 5*time.Second)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to first hop %s: %v", addr, err)
	}

	// Handshake with first hop
	nextDest := target
	if len(chain) > 1 {
		next := chain[1]
		nextDest = fmt.Sprintf("%s:%d", next.IP, next.Port)
	}

	fmt.Printf("%s Handshaking with hop 1 (%s) -> %s\n", col(dim, "  →"), first.IP, nextDest)
	if err := handshakeProxy(conn, first, nextDest); err != nil {
		conn.Close()
		return nil, err
	}

	// Iterate through remaining hops
	for i := 1; i < len(chain); i++ {
		current := chain[i]
		nextDest = target
		if i < len(chain)-1 {
			next := chain[i+1]
			nextDest = fmt.Sprintf("%s:%d", next.IP, next.Port)
		}

		fmt.Printf("%s Handshaking with hop %d (%s) -> %s\n", col(dim, "  →"), i+1, current.IP, nextDest)
		if err := handshakeProxy(conn, current, nextDest); err != nil {
			conn.Close()
			return nil, err
		}
	}

	return conn, nil
}


// handshakeProxy performs the protocol-specific handshake (SOCKS5 or HTTP CONNECT).
func handshakeProxy(conn net.Conn, hop ChainHop, target string) error {
	// Set deadline for the whole handshake
	conn.SetDeadline(time.Now().Add(10 * time.Second))
	defer conn.SetDeadline(time.Time{})

	proto := strings.ToLower(hop.Proto)
	switch proto {
	case "socks5":
		// 1. Send version and methods (NO AUTH)
		if _, err := conn.Write([]byte{0x05, 0x01, 0x00}); err != nil {
			return err
		}
		// 2. Read selected method
		buf := make([]byte, 2)
		if _, err := io.ReadFull(conn, buf); err != nil {
			return fmt.Errorf("socks5 read method: %v", err)
		}
		if buf[0] != 0x05 || buf[1] != 0x00 {
			return fmt.Errorf("socks5 handshake failed with %s: got %x %x", hop.IP, buf[0], buf[1])
		}

		// 3. Send CONNECT request
		host, portStr, err := net.SplitHostPort(target)
		if err != nil {
			host = target
			portStr = "80"
		}
		port, _ := strconv.Atoi(portStr)

		var req []byte
		ip := net.ParseIP(host)
		if ip != nil && ip.To4() != nil {
			// IPv4 address
			req = []byte{0x05, 0x01, 0x00, 0x01}
			req = append(req, ip.To4()...)
		} else {
			// Domain name
			req = []byte{0x05, 0x01, 0x00, 0x03, byte(len(host))}
			req = append(req, []byte(host)...)
		}
		req = append(req, byte(port>>8), byte(port&0xFF))
		
		if _, err := conn.Write(req); err != nil {
			return fmt.Errorf("socks5 write connect: %v", err)
		}

		// 4. Read response
		head := make([]byte, 4)
		if _, err := io.ReadFull(conn, head); err != nil {
			return fmt.Errorf("socks5 read connect response head: %v", err)
		}
		if head[1] != 0x00 {
			return fmt.Errorf("socks5 connect failed on %s: status %d (target: %s)", hop.IP, head[1], target)
		}

		// Skip address
		atyp := head[3]
		switch atyp {
		case 0x01: // IPv4
			io.ReadFull(conn, make([]byte, 6))
		case 0x03: // Domain
			lenBuf := make([]byte, 1)
			io.ReadFull(conn, lenBuf)
			io.ReadFull(conn, make([]byte, int(lenBuf[0])+2))
		case 0x04: // IPv6
			io.ReadFull(conn, make([]byte, 18))
		}
		return nil

	case "http", "https":
		req := fmt.Sprintf("CONNECT %s HTTP/1.1\r\nHost: %s\r\n\r\n", target, target)
		if _, err := conn.Write([]byte(req)); err != nil {
			return err
		}

		// Read response until \r\n\r\n
		headerBuf := make([]byte, 0, 4096)
		oneByte := make([]byte, 1)
		for {
			if _, err := io.ReadFull(conn, oneByte); err != nil {
				return err
			}
			headerBuf = append(headerBuf, oneByte[0])
			if len(headerBuf) >= 4 && string(headerBuf[len(headerBuf)-4:]) == "\r\n\r\n" {
				break
			}
			if len(headerBuf) > 4096 {
				return fmt.Errorf("HTTP CONNECT header too large")
			}
		}

		resp := string(headerBuf)
		if !strings.Contains(resp, "200 Connection established") && !strings.Contains(resp, "200 OK") {
			return fmt.Errorf("HTTP CONNECT failed on %s: %s", hop.IP, resp)
		}
		return nil

	default:
		return fmt.Errorf("unknown protocol: %s", hop.Proto)
	}
}
