package main

/*
#include <stdlib.h>
extern unsigned char* encrypt_with_counter_c(const char* key_hex, const char* nonce_hex, unsigned long long counter, const unsigned char* plaintext, size_t plaintext_len, size_t* out_len);
extern unsigned char* decrypt_with_counter_c(const char* key_hex, const char* nonce_hex, unsigned long long counter, const unsigned char* ciphertext, size_t ciphertext_len, size_t* out_len);
extern unsigned char* encrypt_layered_c(const void* keys_ptr, const void* nonces_ptr, size_t num_hops, unsigned long long counter, const unsigned char* plaintext, size_t plaintext_len, size_t* out_len);
extern unsigned char* decrypt_layered_c(const void* keys_ptr, const void* nonces_ptr, size_t num_hops, unsigned long long counter, const unsigned char* ciphertext, size_t ciphertext_len, size_t* out_len);
extern void free_byte_array(unsigned char* ptr, size_t len);
*/
import "C"

import (
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"io"
	"math/rand"
	"net"
	"strconv"
	"strings"
	"sync"
	"time"
	"unsafe"

	"gitlab.com/yawning/obfs4.git/transports/obfs4"
)

const (
	ChunkSize = 16 * 1024
)

type CryptoSession struct {
	Keys   [][32]byte
	Nonces [][12]byte
}

func NewCryptoSession(hops []CryptoHop) (*CryptoSession, error) {
	s := &CryptoSession{
		Keys:   make([][32]byte, len(hops)),
		Nonces: make([][12]byte, len(hops)),
	}
	for i, h := range hops {
		k, err := hex.DecodeString(h.KeyHex)
		if err != nil || len(k) != 32 {
			return nil, fmt.Errorf("invalid key hex at hop %d", i+1)
		}
		copy(s.Keys[i][:], k)

		n, err := hex.DecodeString(h.NonceHex)
		if err != nil || len(n) != 12 {
			return nil, fmt.Errorf("invalid nonce hex at hop %d", i+1)
		}
		copy(s.Nonces[i][:], n)
	}
	return s, nil
}

// encryptedPipe pumps data between client and server with multi-layered AES-GCM encryption.
func encryptedPipeGarlic(client, serverOut, serverIn net.Conn, cryptoHops []CryptoHop, garlic bool, obfuscation *ObfuscationConfig) error {
	session, err := NewCryptoSession(cryptoHops)
	if err != nil {
		return err
	}

	// Determine obfuscation parameters
	paddingMin := 512
	paddingMax := 1024
	if obfuscation != nil && obfuscation.PaddingRange[1] > 0 {
		paddingMin = obfuscation.PaddingRange[0]
		paddingMax = obfuscation.PaddingRange[1]
		if paddingMax < paddingMin {
			paddingMax = paddingMin
		}
	}

	jitterMs := 3000
	if obfuscation != nil && obfuscation.JitterRange > 0 {
		jitterMs = obfuscation.JitterRange
	}

	errCh := make(chan error, 2)

	// Outbound: client -> serverOut (encrypt in reverse order: from exit hop to entry hop)
	go func() {
		buf := make([]byte, ChunkSize)
		var counter uint64 = 0
		
		// Chaffing ticker: send a dummy packet at randomized intervals if idle
		nextChaff := time.Duration(jitterMs/2 + rand.Intn(jitterMs)) * time.Millisecond
		chaffTicker := time.NewTicker(nextChaff)
		defer chaffTicker.Stop()
		
		for {
			select {
			case <-chaffTicker.C:
				if !garlic {
					continue
				}
				
				// Randomized chaff size
				chaffSize := paddingMin
				if paddingMax > paddingMin {
					chaffSize = paddingMin + rand.Intn(paddingMax-paddingMin+1)
				}
				if chaffSize < 2 {
					chaffSize = 2
				}
				
				// Send a dummy packet (chaff)
				payload := make([]byte, chaffSize)
				// length = 0 means dummy
				binary.LittleEndian.PutUint16(payload[0:2], 0)
				// random data in chaff
				rand.Read(payload[2:])
				
				encrypted, err := encryptLayered(session, counter, payload)
				if err == nil {
					frameHead := make([]byte, 12)
					binary.LittleEndian.PutUint64(frameHead[0:8], counter)
					binary.LittleEndian.PutUint32(frameHead[8:12], uint32(len(encrypted)))
					serverOut.Write(frameHead)
					serverOut.Write(encrypted)
					counter++
				}
				
				// Reset with new jitter
				nextChaff = time.Duration(jitterMs/2 + rand.Intn(jitterMs)) * time.Millisecond
				chaffTicker.Reset(nextChaff)
				
			default:
				client.SetReadDeadline(time.Now().Add(50 * time.Millisecond))
				n, err := client.Read(buf)
				client.SetReadDeadline(time.Time{})
				
				if n > 0 {
					payload := buf[:n]

					// Garlic padding: Pad payload to a randomized size to obscure traffic shape
					if garlic {
						// Calculate target size (multiple of some block or within range)
						// For advanced obfuscation, we pad to a random size in range
						targetSize := paddingMin
						if paddingMax > paddingMin {
							targetSize = paddingMin + rand.Intn(paddingMax-paddingMin+1)
						}
						
						// If original payload is already larger than target, pad to next target increment
						if len(payload) + 2 > targetSize {
							increment := 512
							targetSize = ((len(payload) + 2 + increment - 1) / increment) * increment
						}
						
						padLen := targetSize - (len(payload) + 2)
						if padLen < 0 {
							padLen = 0
						}
						
						// 2 bytes for original length, then payload, then padding
						padded := make([]byte, 2+len(payload)+padLen)
						binary.LittleEndian.PutUint16(padded[0:2], uint16(len(payload)))
						copy(padded[2:], payload)
						// randomize padding data
						rand.Read(padded[2+len(payload):])
						payload = padded
					}

					// Apply all encryption layers in one FFI call
					payload, err = encryptLayered(session, counter, payload)
					if err != nil {
						errCh <- err
						return
					}

					// Frame: [8-byte counter][4-byte LE length][ciphertext]
					frameHead := make([]byte, 12)
					binary.LittleEndian.PutUint64(frameHead[0:8], counter)
					binary.LittleEndian.PutUint32(frameHead[8:12], uint32(len(payload)))

					if _, err := serverOut.Write(frameHead); err != nil {
						errCh <- err
						return
					}
					if _, err := serverOut.Write(payload); err != nil {
						errCh <- err
						return
					}
					counter++
					
					// Reset idle timer with jitter
					nextChaff = time.Duration(jitterMs/2 + rand.Intn(jitterMs)) * time.Millisecond
					chaffTicker.Reset(nextChaff)
				}
				
				if err != nil {
					if err != io.EOF && !strings.Contains(err.Error(), "timeout") {
						errCh <- err
						return
					}
					if err == io.EOF {
						errCh <- nil
						return
					}
					// If timeout, just loop back and wait for more data or chaff
				}
			}
		}
	}()

	// Inbound: serverIn -> client (decrypt in forward order: entry hop to exit hop)
	go func() {
		for {
			// Read 8-byte counter
			counterBuf := make([]byte, 8)
			if _, err := io.ReadFull(serverIn, counterBuf); err != nil {
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
			if _, err := io.ReadFull(serverIn, lenBuf); err != nil {
				errCh <- err
				return
			}
			length := binary.LittleEndian.Uint32(lenBuf)
			if length == 0 || length > ChunkSize*2 { // Allow larger chunks for multi-layer overhead
				errCh <- fmt.Errorf("invalid frame length: %d", length)
				return
			}

			// Read ciphertext
			payload := make([]byte, length)
			if _, err := io.ReadFull(serverIn, payload); err != nil {
				errCh <- err
				return
			}

			// Decrypt all layers in one FFI call
			var err error
			payload, err = decryptLayered(session, receivedCounter, payload)
			if err != nil {
				errCh <- err
				return
			}

			// Remove Garlic padding
			if garlic {
				if len(payload) >= 2 {
					origLen := binary.LittleEndian.Uint16(payload[0:2])
					if origLen == 0 {
						// This is a dummy (chaff) packet, discard and continue
						continue
					}
					if int(origLen) <= len(payload)-2 {
						payload = payload[2 : 2+origLen]
					}
				}
			}

			if _, err := client.Write(payload); err != nil {
				errCh <- err
				return
			}
		}
	}()

	// Wait for any side to finish or error
	return <-errCh
}

func encryptLayered(s *CryptoSession, counter uint64, plaintext []byte) ([]byte, error) {
	var outLen C.size_t
	cOut := C.encrypt_layered_c(
		unsafe.Pointer(&s.Keys[0]),
		unsafe.Pointer(&s.Nonces[0]),
		C.size_t(len(s.Keys)),
		C.ulonglong(counter),
		(*C.uchar)(unsafe.Pointer(&plaintext[0])),
		C.size_t(len(plaintext)),
		&outLen,
	)

	if cOut == nil {
		return nil, fmt.Errorf("layered encryption failed in Rust")
	}
	defer C.free_byte_array((*C.uchar)(cOut), outLen)

	return C.GoBytes(unsafe.Pointer(cOut), C.int(outLen)), nil
}

func decryptLayered(s *CryptoSession, counter uint64, ciphertext []byte) ([]byte, error) {
	var outLen C.size_t
	cOut := C.decrypt_layered_c(
		unsafe.Pointer(&s.Keys[0]),
		unsafe.Pointer(&s.Nonces[0]),
		C.size_t(len(s.Keys)),
		C.ulonglong(counter),
		(*C.uchar)(unsafe.Pointer(&ciphertext[0])),
		C.size_t(len(ciphertext)),
		&outLen,
	)

	if cOut == nil {
		return nil, fmt.Errorf("layered decryption failed in Rust")
	}
	defer C.free_byte_array((*C.uchar)(cOut), outLen)

	return C.GoBytes(unsafe.Pointer(cOut), C.int(outLen)), nil
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
func startSOCKS5Server(port int, initialDecision RotationDecision, dnsPool, nonDNSPool, combinedPool []Proxy, obfuscation *ObfuscationConfig, mimic *MimicConfig) error {
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
			newDecision, err := buildChainDecision(mode, dnsPool, nonDNSPool, combinedPool, currentDecision.Garlic, obfuscation, mimic)
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

		go func(c net.Conn, d RotationDecision, obf *ObfuscationConfig, mim *MimicConfig) {
			if err := handleSOCKS5Client(c, d, dnsPool, nonDNSPool, combinedPool, obf, mim); err != nil {
				// Silently log or handle connection errors
			}
		}(client, d, obfuscation, mimic)
	}
}

// handleSOCKS5Client handles the initial SOCKS5 handshake and request parsing.
func handleSOCKS5Client(conn net.Conn, decision RotationDecision, dnsPool, nonDNSPool, combinedPool []Proxy, obfuscation *ObfuscationConfig, mimic *MimicConfig) error {
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
	server, err := buildCircuit(decision.Chain, targetAddr, dnsPool, nonDNSPool, combinedPool, decision.Mode, decision.Garlic, obfuscation, mimic)
	if err != nil {
		fmt.Printf("%s Failed to build circuit: %v\n", col(red, "✗"), err)
		return fmt.Errorf("failed to build circuit: %v", err)
	}
	defer server.Close()
	fmt.Printf("%s Circuit built successfully to %s\n", col(green, "✓"), targetAddr)

	var serverIn net.Conn = server
	if decision.Garlic {
		fmt.Printf("%s Garlic Mode: Building secondary inbound circuit...\n", col(cyan, "◈"))
		// Attempt to build a second circuit for the inbound path
		server2, err2 := buildCircuit(decision.Chain, targetAddr, dnsPool, nonDNSPool, combinedPool, decision.Mode, decision.Garlic, obfuscation, mimic)
		if err2 == nil {
			defer server2.Close()
			serverIn = server2
			fmt.Printf("%s Secondary circuit built (Dual-Path Active)\n", col(green, "✓"))
		} else {
			fmt.Printf("%s Secondary circuit failed, falling back to single path: %v\n", col(yellow, "⚠"), err2)
		}
	}

	// 4. Send success to client
	if _, err := conn.Write([]byte{0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0}); err != nil {
		return err
	}

	// 5. Pipe data — with AES-GCM encryption if keys are available
	if len(decision.Encryption) > 0 {
		return encryptedPipeGarlic(conn, server, serverIn, decision.Encryption, decision.Garlic, obfuscation)
	}

	// Fallback to plain pipe
	errCh := make(chan error, 2)
	go func() {
		_, err := io.Copy(server, conn)
		errCh <- err
	}()
	go func() {
		_, err := io.Copy(conn, serverIn)
		errCh <- err
	}()
	return <-errCh
}

// buildCircuit builds a multi-hop proxy circuit with retries and live rotation.
func buildCircuit(chain []ChainHop, target string, dnsPool, nonDNSPool, combinedPool []Proxy, mode string, garlic bool, obfuscation *ObfuscationConfig, mimic *MimicConfig) (net.Conn, error) {
	if len(chain) == 0 {
		return nil, fmt.Errorf("empty proxy chain")
	}

	maxRetries := 3
	var lastErr error
	currentChain := chain

	for attempt := 0; attempt < maxRetries; attempt++ {
		if attempt > 0 {
			fmt.Printf("%s Circuit build attempt %d/%d (rotating proxies)....\n", col(dim, "→"), attempt+1, maxRetries)
			// Rotate the chain on failure
			newDecision, err := buildChainDecision(mode, dnsPool, nonDNSPool, combinedPool, garlic, obfuscation, mimic)
			if err == nil && newDecision != nil {
				currentChain = newDecision.Chain
			}
		}

		conn, err := buildCircuitInternal(currentChain, target, mimic)
		if err == nil {
			return conn, nil
		}
		
		lastErr = err
		fmt.Printf("%s Attempt %d failed: %v\n", col(yellow, "⚠"), attempt+1, err)
		time.Sleep(500 * time.Millisecond)
	}

	return nil, fmt.Errorf("all retries failed: %v", lastErr)
}

func buildCircuitInternal(chain []ChainHop, target string, mimic *MimicConfig) (net.Conn, error) {
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
	if err := handshakeProxy(conn, first, nextDest, mimic); err != nil {
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
		if err := handshakeProxy(conn, current, nextDest, mimic); err != nil {
			conn.Close()
			return nil, err
		}
	}

	return conn, nil
}


// handshakeProxy performs the protocol-specific handshake (SOCKS5 or HTTP CONNECT).
func handshakeProxy(conn net.Conn, hop ChainHop, target string, mimic *MimicConfig) error {
	// Set deadline for the whole handshake
	conn.SetDeadline(time.Now().Add(10 * time.Second))
	defer conn.SetDeadline(time.Time{})

	// Apply mimicry wrapper if configured
	var currentConn net.Conn = conn
	if mimic != nil && mimic.Protocol != "" {
		// Mimicry will be handled in the next task, but we pass it through here
	}

	// Apply obfs4 wrapper if configured
	if hop.Obfuscation != nil && hop.Obfuscation.Mode == "obfs4" {
		var err error
		currentConn, err = wrapObfs4Client(conn, fmt.Sprintf("%s:%d", hop.IP, hop.Port), hop.Obfuscation)
		if err != nil {
			return fmt.Errorf("obfs4 wrap failed: %v", err)
		}
	}

	proto := strings.ToLower(hop.Proto)
	switch proto {
	case "socks5":
		// 1. Send version and methods (NO AUTH)
		if _, err := currentConn.Write([]byte{0x05, 0x01, 0x00}); err != nil {
			return err
		}
		// 2. Read selected method
		buf := make([]byte, 2)
		if _, err := io.ReadFull(currentConn, buf); err != nil {
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
		
		if _, err := currentConn.Write(req); err != nil {
			return fmt.Errorf("socks5 write connect: %v", err)
		}

		// 4. Read response
		head := make([]byte, 4)
		if _, err := io.ReadFull(currentConn, head); err != nil {
			return fmt.Errorf("socks5 read connect response head: %v", err)
		}
		if head[1] != 0x00 {
			return fmt.Errorf("socks5 connect failed on %s: status %d (target: %s)", hop.IP, head[1], target)
		}

		// Skip address
		atyp := head[3]
		switch atyp {
		case 0x01: // IPv4
			io.ReadFull(currentConn, make([]byte, 6))
		case 0x03: // Domain
			lenBuf := make([]byte, 1)
			io.ReadFull(currentConn, lenBuf)
			io.ReadFull(currentConn, make([]byte, int(lenBuf[0])+2))
		case 0x04: // IPv6
			io.ReadFull(currentConn, make([]byte, 18))
		}
		return nil

	case "http", "https":
		req := fmt.Sprintf("CONNECT %s HTTP/1.1\r\nHost: %s\r\n\r\n", target, target)
		if _, err := currentConn.Write([]byte(req)); err != nil {
			return err
		}

		// Read response until \r\n\r\n
		headerBuf := make([]byte, 0, 4096)
		oneByte := make([]byte, 1)
		for {
			if _, err := io.ReadFull(currentConn, oneByte); err != nil {
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

func wrapObfs4Client(conn net.Conn, addr string, config *ObfuscationConfig) (net.Conn, error) {
	t := &obfs4.Transport{}
	args := make(map[string][]string)
	if config.NodeID != "" {
		args["node-id"] = []string{config.NodeID}
	}
	if config.PublicKey != "" {
		args["public-key"] = []string{config.PublicKey}
	}
	if config.Cert != "" {
		args["cert"] = []string{config.Cert}
	}
	args["iat-mode"] = []string{strconv.Itoa(config.IATMode)}

	cf, err := t.ClientFactory("")
	if err != nil {
		return nil, err
	}

	return cf.Dial("tcp", addr, func(network, address string) (net.Conn, error) {
		return conn, nil
	}, args)
}
