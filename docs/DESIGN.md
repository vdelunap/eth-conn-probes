# Ethereum Connectivity Probing System
## Technical Design, Protocol Analysis, and Implementation Decisions

> **Document type**: Technical design reference (TFM-style)
> **Project**: eth-conn-probes
> **Last updated**: 2026-05-14

---

## Table of Contents

1. [Abstract](#1-abstract)
2. [Introduction](#2-introduction)
3. [Ethereum Network Architecture](#3-ethereum-network-architecture)
   - 3.1 [The Post-Merge Split: Execution and Consensus Layers](#31-the-post-merge-split-execution-and-consensus-layers)
   - 3.2 [Node Types and Network Participants](#32-node-types-and-network-participants)
4. [Network Protocols In Depth](#4-network-protocols-in-depth)
   - 4.1 [DNS](#41-dns--domain-name-system)
   - 4.2 [TCP](#42-tcp--transmission-control-protocol)
   - 4.3 [TLS](#43-tls--transport-layer-security)
   - 4.4 [HTTP / HTTPS and JSON-RPC](#44-http--https-and-json-rpc)
   - 4.5 [WebSocket and WSS](#45-websocket-and-wss)
   - 4.6 [RLP Encoding](#46-rlp--recursive-length-prefix-encoding)
   - 4.7 [devp2p: the Execution P2P Stack](#47-devp2p--the-execution-layer-p2p-stack)
   - 4.8 [ENR: Ethereum Node Records](#48-enr--ethereum-node-records)
   - 4.9 [DiscV5: Consensus Peer Discovery](#49-discv5--version-5-peer-discovery)
   - 4.10 [libp2p: the Consensus P2P Stack](#410-libp2p--the-consensus-layer-p2p-stack)
   - 4.11 [Beacon REST API](#411-beacon-rest-api)
5. [Censorship Vectors in the Ethereum Network](#5-censorship-vectors-in-the-ethereum-network)
6. [System Architecture](#6-system-architecture)
7. [Implemented Probes](#7-implemented-probes)
8. [Connectivity by Participant Type](#8-connectivity-by-participant-type)
   - 8bis [Probe Result Interpretation: A Worked Example](#8bis-probe-result-interpretation-a-worked-example)
9. [Technical Decisions and Justifications](#9-technical-decisions-and-justifications)
10. [What Is Not Implemented and Why](#10-what-is-not-implemented-and-why)
11. [Target IP and Key Sourcing](#11-target-ip-and-key-sourcing)
12. [Future Work](#12-future-work)
13. [References](#13-references)

---

## 1. Abstract

This document describes the design, rationale, and implementation of **eth-conn-probes**, a cross-platform Ethereum connectivity probing system. The system measures network reachability to the Ethereum network across all of its protocol layers: DNS resolution, TCP/TLS transport, JSON-RPC application endpoints, execution-layer P2P (devp2p/DiscV4/RLPx), and consensus-layer P2P (DiscV5/libp2p/Beacon REST API). Each probe is designed to isolate a specific layer or protocol so that failures can be attributed precisely — distinguishing DNS blocking from port blocking, TLS interception from application-level censorship, and read-access restrictions from write-access restrictions. The system ships as a CLI tool and a Tauri desktop application, both backed by the same Rust core library. Reports are collected by a self-hosted server to aggregate connectivity data across geographic regions.

---

## 2. Introduction

Ethereum is a decentralised public blockchain. Its censorship resistance is a core design goal: no single entity should be able to prevent a user from reading chain state or submitting transactions. However, censorship can occur at multiple layers below the application — at the network level, imposed by ISPs, governments, or corporate firewalls — before any Ethereum-level logic is even reached.

Several real-world censorship events motivate this project:

- **DNS-based blocking**: Some national firewalls respond to DNS queries for known RPC provider domains with NXDOMAIN (domain not found) or with poisoned records (wrong IP). Because DNS is typically unencrypted, ISPs can intercept and modify responses.
- **Port blocking**: Ethereum's execution P2P protocol uses port 30303 (TCP and UDP). This port has no other major use, making it a trivial target for ISP-level blocking, unlike port 443 (HTTPS) which carries general web traffic.
- **TLS SNI blocking**: The Server Name Indication (SNI) field in a TLS ClientHello is sent in plaintext. Firewalls can use it to block specific domains even when the IP address is shared with non-blocked domains (CDN scenarios).
- **Application-layer transaction filtering**: Following the OFAC sanctioning of Tornado Cash in August 2022, major RPC providers (Infura, Alchemy) began rejecting `eth_sendRawTransaction` calls involving sanctioned addresses. This is censorship that passes all network-layer tests but blocks write access at the JSON-RPC level.

eth-conn-probes addresses all of these by running a structured battery of probes at each layer and producing a structured JSON report that can be aggregated server-side to build a geographic picture of Ethereum network accessibility.

---

## 3. Ethereum Network Architecture

### 3.1 The Post-Merge Split: Execution and Consensus Layers

Before September 2022, Ethereum ran as a single process: a proof-of-work node handled both block production and P2P networking. The "Merge" (September 15, 2022) split this into two cooperating processes:

**Execution Layer (EL)**: Runs an execution client (Geth, Reth, Nethermind, Besu). Responsible for the EVM (Ethereum Virtual Machine), state transitions, transaction mempool, and the devp2p P2P network. Communicates with the consensus layer via the Engine API (JSON-RPC over authenticated HTTP, local only).

**Consensus Layer (CL)**: Runs a consensus client (Lighthouse, Prysm, Teku, Nimbus). Responsible for the proof-of-stake protocol (block proposal, attestation, finality), and the libp2p P2P network. Exposes a public Beacon REST API.

A fully functional Ethereum node requires both layers running simultaneously on the same machine. However, from a network reachability standpoint, they are independent: the execution layer's ports (30303 TCP/UDP) are entirely separate from the consensus layer's ports (9000 TCP/UDP, 5052 HTTP).

### 3.2 Node Types and Network Participants

Understanding which type of node is involved dictates which ports and protocols are relevant to probe.

#### 3.2.1 Full Node (Execution Layer)

A full node stores the complete current state (account balances, contract storage, bytecode) and all block headers since genesis, but not necessarily all historical execution traces. It participates in the execution P2P network to download and validate new blocks.

Network behaviour:
- **Outbound**: Discovers peers via DiscV4 (UDP:30303), connects via RLPx (TCP:30303), downloads blocks via the ETH protocol.
- **Inbound**: Accepts connections on TCP:30303 and UDP:30303 from other nodes.
- **Optional**: Exposes JSON-RPC locally (HTTP:8545, WS:8546) for wallets and dApps. Public RPC providers are full nodes with this interface exposed to the internet.

#### 3.2.2 Archive Node (Execution Layer)

An archive node is a full node that additionally retains all historical state — every state root at every block. This requires significantly more disk space (multiple terabytes) but enables historical queries. From a network connectivity standpoint, an archive node is indistinguishable from a full node: same ports, same protocols.

#### 3.2.3 Light Node (Execution Layer) — Deprecated

A light node downloads only block headers and uses Merkle proofs to verify individual state queries, delegating full execution to a serving full node via the LES (Light Ethereum Subprotocol). LES was designed for resource-constrained devices (mobile phones).

**Current status**: LES is functionally deprecated. Geth disabled LES server-mode in 2023 citing lack of demand and maintenance cost. No major client currently serves LES in production. The protocol remains specified but has no live network.

**Portal Network**: The intended replacement for light clients is the Portal Network, a new peer-to-peer network using the uTP (Micro Transport Protocol, an UDP-based reliable transport) and a custom DHT. Portal is still under active development (2025) and has a small, growing node count. It is intentionally not included in the current probe set — see Section 10.

#### 3.2.4 Consensus Node / Validator

A consensus client (Lighthouse, Prysm, Teku, Nimbus) runs the proof-of-stake consensus protocol. Validators additionally hold BLS keys and produce/attest blocks.

Network behaviour:
- **Outbound**: Discovers peers via DiscV5 (UDP:9000), connects via libp2p (TCP:9000), exchanges gossip (attestations, blocks) via GossipSub.
- **Inbound**: Accepts connections on TCP:9000 and UDP:9000.
- **Local Engine API**: Communicates with the execution client on localhost.
- **Optional public Beacon REST API**: Some consensus nodes expose `GET /eth/v1/...` endpoints publicly (port 5052 or behind a reverse proxy on 443).

#### 3.2.5 RPC Providers

Public RPC providers (Infura, Alchemy, QuickNode, PublicNode, LlamaRPC, etc.) are full nodes with their JSON-RPC exposed publicly over HTTPS and/or WSS, typically behind a CDN or load balancer. They accept `eth_chainId`, `eth_blockNumber`, `eth_sendRawTransaction`, etc. from any client.

From a probing standpoint, RPC providers are the most important targets because they are the gateway to Ethereum for the vast majority of users (via wallets and dApps).

#### 3.2.6 Wallets and dApps

Wallets (MetaMask, Rainbow, Coinbase Wallet, hardware wallets) and dApps are not nodes — they do not participate in the P2P network. They connect exclusively through JSON-RPC to a configured provider. Their entire Ethereum connectivity is captured by the `https_jsonrpc`, `wss_jsonrpc`, and `https_jsonrpc_write` probes.

---

## 4. Network Protocols In Depth

### 4.1 DNS — Domain Name System

DNS translates human-readable domain names (`ethereum-rpc.publicnode.com`) into IP addresses. It is the first step in any network connection and also the first point of censorship.

**How it works**: The OS sends a UDP query (typically) to a configured DNS resolver (usually provided by the ISP). The resolver recursively queries authoritative nameservers and returns A records (IPv4) or AAAA records (IPv6).

**Censorship mechanisms**:
- **NXDOMAIN injection**: The ISP resolver returns "domain does not exist" for targeted domains, even though the domain resolves correctly globally.
- **IP poisoning**: The resolver returns a wrong IP (e.g., pointing to a block page or a loopback address).
- **Silent drop**: Queries to certain nameservers are dropped silently.

**DNS over HTTPS (DoH)**: DoH encrypts DNS queries inside HTTPS, bypassing local resolver inspection. Cloudflare's public DoH endpoint at `1.1.1.1` (`https://1.1.1.1/dns-query`) accepts queries in JSON format and responds with A records. Because DoH is tunnelled inside port-443 HTTPS traffic, it is much harder to block than plain DNS without also blocking all HTTPS.

**What the `dns_resolve` probe tests**: Whether the system resolver can resolve a hostname at all. A failure means the domain is either unreachable by DNS or actively blocked.

**What the `dns_compare` probe tests**: It resolves the same hostname with both the system resolver and Cloudflare DoH, then compares the results. If the system resolver returns nothing but DoH returns IPs, this is a strong indicator of DNS blocking. If both return different IPs, this flags `ip_mismatch` for server-side analysis — it could be legitimate CDN anycast routing (normal) or DNS poisoning (censorship). IP mismatches alone are not treated as failures at the probe level.

### 4.2 TCP — Transmission Control Protocol

TCP is a connection-oriented, reliable transport protocol. Establishing a TCP connection requires a three-way handshake:

1. Client sends **SYN** (synchronise).
2. Server responds **SYN-ACK** (synchronise-acknowledge).
3. Client responds **ACK** (acknowledge). Connection is now open.

If the SYN reaches the server and the server is listening, the handshake completes quickly (typically 10–200ms depending on geographic distance). If the port is blocked:
- A stateful firewall may drop the SYN silently → client times out.
- A firewall may send a RST (reset) → client gets an immediate connection refused error.

**What `tcp_connect` tests**: Whether a TCP connection can be established to a given host:port. Success means packets traversed the full network path and the remote is listening. Failure (timeout or refused) means either the port is blocked or the remote is down.

**Diagnostic value**: By pairing `dns_resolve` with `tcp_connect`, we can distinguish DNS failures (DNS probe fails, TCP not attempted) from network-layer blocks (DNS succeeds but TCP fails).

### 4.3 TLS — Transport Layer Security

TLS encrypts the data channel between client and server. It runs on top of TCP and is used by HTTPS (port 443) and WSS (WebSocket Secure). The current TLS version in wide deployment is TLS 1.3, with TLS 1.2 as a fallback.

#### 4.3.1 The TLS Handshake

The TLS 1.3 handshake (simplified):

1. **ClientHello**: Client sends supported cipher suites, TLS version, a random nonce, and the **SNI** (Server Name Indication) — the target hostname in plaintext.
2. **ServerHello + Certificate**: Server selects cipher, sends its X.509 certificate chain and its key share.
3. **Key derivation**: Both sides derive symmetric session keys using Diffie-Hellman (ECDH).
4. **Finished**: Both sides confirm the handshake is complete. All subsequent data is encrypted.

The entire handshake (steps 1–4) completes before any application data is exchanged. This is why a dedicated TLS probe has diagnostic value: it measures the handshake independently of the HTTP request that follows.

#### 4.3.2 SNI — Server Name Indication

SNI is an extension in the ClientHello that tells the server which certificate to present, essential when multiple domains share an IP (CDN/virtual hosting). The critical problem: **SNI is transmitted in plaintext**, even in TLS 1.3. This means any intermediate node (ISP, firewall) can read the target hostname from an otherwise encrypted TLS connection.

**SNI-based blocking**: A censor can block specific domains by inspecting the SNI field and resetting connections to targeted hostnames, while allowing all other traffic on port 443 to pass. This would appear in the probe stack as: DNS succeeds + TCP succeeds + TLS fails → SNI block.

Note: TLS 1.3 introduced **Encrypted Client Hello (ECH)**, which encrypts the SNI. ECH requires DNS support (HTTPS records with ECH config) and server-side deployment. As of 2025, ECH is not deployed by any major Ethereum RPC provider.

#### 4.3.3 X.509 Certificates

TLS servers authenticate themselves with X.509 certificates issued by a Certificate Authority (CA). A certificate contains:
- **Subject**: The entity the certificate was issued to (domain names via Subject Alternative Names).
- **Issuer**: The CA that signed the certificate.
- **Validity period**: `notBefore` and `notAfter` timestamps. An expired certificate is a TLS handshake failure.
- **Public key**: Used to verify the server's signature during the handshake.

**MITM detection via certificate inspection**: If a state-level adversary intercepts TLS traffic, they must present their own certificate (not the legitimate one). A probe that extracts and reports the certificate subject and issuer enables detecting certificate substitution — the server-side aggregator can flag when reported certificates diverge from the expected issuer (e.g., Let's Encrypt / DigiCert for most RPC providers).

**What `tls_handshake` tests**: It completes a TLS handshake without sending any HTTP request, measuring the handshake RTT and extracting the TLS version, cipher suite, and certificate subject/expiry. This isolates TLS from the application layer.

### 4.4 HTTP / HTTPS and JSON-RPC

HTTP (HyperText Transfer Protocol) is the application-layer protocol for the web. HTTPS is HTTP over TLS.

**JSON-RPC**: Ethereum's execution layer exposes a JSON-RPC 2.0 API. A request is a JSON object with fields `jsonrpc` (always "2.0"), `id`, `method`, and `params`. The response contains either `result` (success) or `error` (application-level error with `code` and `message`).

Example `eth_chainId` request:
```json
{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}
```
Successful response:
```json
{"jsonrpc":"2.0","id":1,"result":"0x1"}
```
`0x1` = mainnet.

**What `https_jsonrpc` tests**: Full DNS → TCP → TLS → HTTP → JSON-RPC chain. It calls `eth_chainId` and validates that the response contains a `result` field. This is the complete "wallet read access" test.

**Why `eth_chainId`**: It is a free, read-only call that every Ethereum RPC endpoint must support, with no API key required. It is the standard canary call used by wallets to verify connectivity.

**What `https_jsonrpc_write` tests**: The `eth_sendRawTransaction` method is the only write operation in the Ethereum JSON-RPC API from a client perspective. This probe sends `eth_sendRawTransaction` with an intentionally invalid raw transaction payload (`0x`, empty bytes). A functioning provider will respond with a JSON-RPC error like `{"error":{"code":-32000,"message":"invalid transaction"}}`. The probe considers this a **success** — the error proves the provider is processing write requests. A network-level block (timeout, 403, connection refused) indicates the write endpoint is inaccessible, which is a censorship signal distinct from read-access censorship.

This matters because OFAC-compliant providers selectively reject write requests from/to sanctioned addresses. More broadly, a censor could allow `eth_chainId` (informational) while blocking `eth_sendRawTransaction` (transactional). The two probes distinguish these cases.

### 4.5 WebSocket and WSS

WebSocket is a full-duplex communication protocol over a single TCP connection. Unlike HTTP (request/response), WebSocket allows the server to push data to the client. WSS is WebSocket over TLS.

#### 4.5.1 The WebSocket Handshake

WebSocket begins as an HTTP/1.1 connection and upgrades via the `Upgrade` header:

Client sends:
```
GET / HTTP/1.1
Host: ethereum-rpc.publicnode.com
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Key: <base64-nonce>
Sec-WebSocket-Version: 13
```

Server responds:
```
HTTP/1.1 101 Switching Protocols
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Accept: <derived from nonce>
```

After the 101 response, the connection is a WebSocket channel, framed in WebSocket frames rather than HTTP.

**What `wss_jsonrpc` tests**: Connects to a WSS endpoint, performs the WebSocket handshake (which includes TLS first, then the HTTP upgrade), sends `eth_chainId` as a JSON-RPC payload over the WebSocket frame, and validates the response.

#### 4.5.2 Real-time Subscriptions

WebSocket enables Ethereum subscriptions via `eth_subscribe`. Unlike polling (`eth_blockNumber` over HTTP every N seconds), subscriptions push data to the client as it arrives.

The `eth_subscribe` request:
```json
{"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":["newHeads"]}
```

The confirmation response (immediate):
```json
{"jsonrpc":"2.0","id":1,"result":"0x9cef478923ff08bf67fde6c64013158d"}
```
`result` is the subscription ID. Subsequent block notifications are pushed asynchronously:
```json
{"jsonrpc":"2.0","method":"eth_subscription","params":{"subscription":"0x9cef...","result":{...block header...}}}
```

**What `wss_subscribe` tests**: Connects via WSS, sends an `eth_subscribe` for `newHeads`, and checks that the subscription confirmation (with a `result` subscription ID) is received. The probe does **not** wait for a block notification — Ethereum produces a new block every ~12 seconds, which exceeds the default 5-second probe timeout. The subscription confirmation alone proves that the WSS subscription mechanism is fully operational end-to-end.

### 4.6 RLP — Recursive Length Prefix Encoding

RLP is Ethereum's canonical binary serialisation format, used for all data on the execution layer (transactions, blocks, receipts, P2P messages). It is simpler than protobuf or ASN.1.

**Encoding rules**:
- A single byte in `[0x00, 0x7f]` is its own encoding.
- A byte string of 0–55 bytes: `(0x80 + length)` followed by the bytes.
- A byte string of 56+ bytes: `(0xb7 + len(len))` followed by the big-endian length, followed by the bytes.
- A list: collect all RLP encodings of elements, then:
  - If total payload is 0–55 bytes: `(0xc0 + length)` followed by the payload.
  - If total payload is 56+ bytes: `(0xf7 + len(len))` followed by big-endian length, followed by payload.
- Integers are encoded as their minimal big-endian representation (no leading zero bytes), except for zero which is `0x80` (empty byte string).

RLP is used in the project for:
1. **DiscV4 PING/PONG packets**: The packet data payload is RLP-encoded.
2. **RLPx auth message**: The EIP-8 auth body is RLP-encoded before ECIES encryption.

The project implements a minimal RLP encoder in `src/rlp.rs` covering byte strings (`rlp_bytes`), unsigned integers (`rlp_uint`), and lists (`rlp_list`). This avoids an external dependency while covering exactly the structures needed for DiscV4 and RLPx.

### 4.7 devp2p — the Execution Layer P2P Stack

devp2p is the collection of protocols that form the Ethereum execution layer's peer-to-peer network. It sits entirely outside the TCP/IP application layer model — it is a custom P2P stack.

#### 4.7.1 DiscV4 — Peer Discovery (UDP)

DiscV4 (Discovery Version 4) is the peer discovery protocol for execution nodes. It runs over **UDP port 30303** and is based on Kademlia DHT.

A node wishing to join the network sends **PING** packets to known boot nodes. The boot node, upon receiving a PING, verifies the sender's identity and responds with a **PONG**. After the ping-pong exchange (called the "endpoint proof"), the boot node sends a `FIND_NODE` response with the addresses of other peers in its routing table.

**Packet format**:
```
packet = hash(32) || signature(65) || packet-type(1) || RLP-data
hash = keccak256(signature || packet-type || RLP-data)
signature = secp256k1.sign(keccak256(packet-type || RLP-data)) [65 bytes: r(32)+s(32)+recovery(1)]
```

The hash allows quick integrity checking. The signature allows the recipient to recover the sender's secp256k1 public key (the node's identity).

**PING RLP-data**: `[version=4, from=[ip,udp,tcp], to=[ip,udp,tcp], expiration]`
**PONG RLP-data**: `[to, ping-hash, expiration]`

**Endpoint proof**: Some nodes perform an "endpoint proof" before responding with a PONG — they first send a PING back to the initiator. The initiator must respond with a PONG to prove its IP is reachable. This protects against IP spoofing in DDoS amplification attacks. The `discv4_ping` probe handles this by responding to incoming PINGs before waiting for the PONG.

**What `discv4_ping` tests**: Whether the UDP-based peer discovery protocol is reachable. If TCP:30303 works but UDP:30303 does not, the execution P2P network would be unreachable for peer discovery (a node would have no way to find peers), even though the RLPx transport port is open. This also tests for UDP-specific blocking.

#### 4.7.2 enode:// — Node Identifiers for Execution Nodes

An enode URL is the canonical identifier for an execution node:
```
enode://PUBKEY@IP:PORT
```
- `PUBKEY`: 128 hex characters = 64 bytes = uncompressed secp256k1 public key (x||y, without the 0x04 prefix).
- `IP`: IPv4 or IPv6 address.
- `PORT`: TCP port (typically 30303). UDP port is assumed to be the same.

The public key serves as the node's permanent identity (node ID). The node ID is used as the key in the Kademlia DHT for peer discovery. It is derived from a secp256k1 private key that the node generates on first launch and persists. The node ID is `keccak256(pubkey)[0:32]`.

Enode URLs are published by client teams for their boot nodes, allowing new nodes to join the network. The Ethereum Foundation maintains a set of stable boot nodes listed in the `params/bootnodes.go` file of go-ethereum.

**Why the `rlpx_handshake` probe needs enode URLs**: The RLPx auth message is ECIES-encrypted to the remote node's static public key (from the enode). Without the remote's public key, the ECIES encryption cannot be completed. This is fundamentally different from TCP or DiscV4 probes that need only IP:port.

#### 4.7.3 RLPx — Encrypted Transport

RLPx is the encrypted transport layer that runs over TCP for the execution P2P network. It establishes an encrypted and authenticated channel between two nodes before any application-level messages (ETH protocol) are exchanged.

**The RLPx handshake (EIP-8 format)**:

The handshake proves mutual knowledge of each node's private key and establishes forward-secret session keys. The initiator (prober) performs:

1. **Generate keys**:
   - `static_key`: Fresh secp256k1 keypair (the prober's identity for this session).
   - `auth_ephemeral_key`: Fresh secp256k1 keypair used to sign the auth body (allows the receiver to recover our ephemeral pubkey via ECDSA recovery).
   - `ecies_ephemeral_key`: Fresh keypair used for ECIES encryption.
   - `nonce`: 32 random bytes.

2. **Compute static shared secret**:
   ```
   static_shared = ECDH(static_key.priv, remote_static_pubkey).x  [32 bytes]
   msg = static_shared XOR nonce
   sig = sign_prehash(msg, auth_ephemeral_key.priv)  [65 bytes]
   ```

3. **Build auth body** (RLP-encoded):
   ```
   auth_body = RLP([sig(65), static_pubkey_raw(64), nonce(32), version=4])
   ```
   Optional random padding is appended for EIP-8 forward compatibility.

4. **ECIES encrypt** auth body to `remote_static_pubkey`:
   - **ECDH**: `shared = ECDH(ecies_ephemeral.priv, remote_static_pubkey).x`
   - **KDF** (ANSI X9.63, SHA-256, counter=1): `K = SHA256([0,0,0,1] || shared)` → 32 bytes
   - **enc_key** = `K[0:16]` (AES-128 key)
   - **mac_key** = `SHA256(K[16:32])` (32-byte HMAC key)
   - **IV** = random 16 bytes
   - **ciphertext** = `AES-128-CTR(enc_key, IV, auth_body)`
   - **MAC** = `HMAC-SHA256(mac_key, IV || ciphertext)`
   - **ECIES packet** = `ecies_ephemeral_pubkey(65) || IV(16) || ciphertext || MAC(32)`

5. **Size-prefix** (EIP-8):
   ```
   packet = uint16_BE(len(ecies_packet)) || ecies_packet
   ```

6. Send packet over TCP:30303. If the remote responds with **any data** (auth-ack or disconnect message), the probe reports success. A RST, timeout, or no response indicates the RLPx layer is unreachable or filtered.

**What `rlpx_handshake` tests**: Whether the execution P2P application protocol is reachable, beyond mere TCP connectivity. A censor performing Deep Packet Inspection could allow TCP:30303 to connect but reset the connection upon detecting the RLPx auth message format (known byte patterns). If TCP connect succeeds but RLPx receives no response, this indicates protocol-level filtering.

**Implementation note**: The RLPx probe generates fresh keypairs on every attempt. The prober has no persistent node identity. The boot nodes will reject the auth (since the prober does not complete the full handshake and ETH protocol exchange), but will respond with an auth-ack or disconnect message — both constitute valid "I am alive and processing RLPx" responses.

#### 4.7.4 ETH Protocol

After a successful RLPx handshake, nodes exchange ETH protocol messages. The first message is a `Status` message containing `network_id`, `genesis_hash`, `fork_id`, and current head information. This mutual `Status` exchange is what actually makes two nodes peers.

This layer is intentionally not probed — see Section 10 for the reasoning.

### 4.8 ENR — Ethereum Node Records

ENR (Ethereum Node Record, EIP-778) is the canonical identity format for Ethereum nodes, particularly for the consensus layer. An ENR is a signed, versioned record containing key-value pairs describing a node's network parameters.

**Format**: An ENR is RLP-encoded and then base64url-encoded with an `enr:` prefix:
```
enr:-AAAAA...
```

**Key-value pairs** include:
- `id`: Identity scheme (`"v4"` for secp256k1).
- `secp256k1`: The node's compressed public key (33 bytes).
- `ip`: IPv4 address (4 bytes).
- `ip6`: IPv6 address (16 bytes).
- `tcp`: TCP port (2 bytes, uint16).
- `udp`: UDP port (2 bytes, uint16).
- `eth2` / `attnets` / `syncnets`: Consensus-layer specific fields indicating which beacon chain the node is on.

**Signature**: The record is signed by the node's private key (matching the `secp256k1` public key field). This prevents tampering: any modification invalidates the signature.

**Comparison with enode://**:
- `enode://` is the execution layer format: pubkey + IP + port, no signature, no versioning, no extensible fields.
- ENR is the consensus layer format (also used in DiscV5 for execution): signed, versioned, extensible, self-describing.

**Why ENR matters for the probe**: The `discv5_consensus` probe targets are specified as ENR strings. The ENR contains the node's IP address (`ip` field) and both TCP and UDP ports (`tcp`, `udp`). When the `discv5` feature is enabled, the project decodes these ENRs to extract IP and TCP port for the `beacon_tcp_connect` probe, avoiding the need to maintain a separate list of IPs.

### 4.9 DiscV5 — Version 5 Peer Discovery

DiscV5 is the peer discovery protocol used by Ethereum consensus nodes (and also intended as the long-term replacement for DiscV4 on the execution layer). Like DiscV4, it runs over **UDP** and is Kademlia-based, but it uses ENR instead of enode and has improved security and flexibility.

**Key differences from DiscV4**:
- Uses ENR for node identity (signed, structured).
- Topic discovery: nodes can advertise capabilities (e.g., "I am a consensus node on mainnet").
- Better NAT traversal (WHOAREYOU challenge-response for unknown sources).
- Port is typically **9000** for consensus nodes, **30303** for execution nodes.

**PING/PONG**: Like DiscV4, DiscV5 uses PING and PONG to verify reachability. The `discv5_ping` and `beacon_discv5_ping` probes initiate a DiscV5 session (bind a local UDP socket, start a DiscV5 instance with a fresh identity), then send a PING to the target ENR and await a PONG.

**Implementation**: The `discv5` feature uses the `discv5` Rust crate (by Sigma Prime, the Lighthouse team). This crate implements the full DiscV5 protocol and is the same code used in Lighthouse production. This guarantees protocol compatibility.

**What `discv5_ping` / `beacon_discv5_ping` tests**: Whether UDP-based peer discovery on port 9000 (or 30303 for execution) is reachable and whether the target boot node responds to a valid DiscV5 PING.

### 4.10 libp2p — the Consensus Layer P2P Stack

libp2p is a modular, multi-protocol P2P networking framework developed by Protocol Labs (IPFS). Ethereum consensus clients adopted it as their transport stack. It runs over **TCP port 9000** for consensus nodes.

The libp2p stack for Ethereum consensus consists of several layers:

#### 4.10.1 Noise Protocol

The Noise Protocol Framework is a set of cryptographic handshake patterns. Ethereum consensus uses **Noise_XX_25519_ChaChaPoly_SHA256** — the Noise XX pattern with:
- **X25519** Diffie-Hellman for key agreement.
- **ChaCha20-Poly1305** for AEAD encryption.
- **SHA-256** for hashing.

The XX pattern involves three messages:
1. Initiator sends: `e` (ephemeral public key, 32 bytes).
2. Responder sends: `e, ee, s, es` (ephemeral key, ECDH(e,e), static key, ECDH(e,static-initiator)).
3. Initiator sends: `s, se` (static key, ECDH(static-initiator, e-responder)).

After the handshake, both sides have a shared secret and all subsequent communication is encrypted with ChaCha20-Poly1305.

#### 4.10.2 Multistream-Select

Multistream-select is a protocol negotiation layer. Before starting the Noise handshake, both sides must agree on which protocol to use. Each side proposes protocols in order:

```
Client → Server: <varint-length>/multistream/1.0.0\n
Server → Client: <varint-length>/multistream/1.0.0\n  [acknowledgement]
Client → Server: <varint-length>/noise\n               [proposal]
Server → Client: <varint-length>/noise\n               [accepted]
  OR
Server → Client: <varint-length>na\n                   [not available]
```

Each message is prefixed by its length as an unsigned varint (variable-length integer; for all messages used here, the length fits in a single byte since it is < 128).

`/multistream/1.0.0\n` is 19 bytes → length prefix `0x13`.
`/noise\n` is 7 bytes → length prefix `0x07`.

**What `libp2p_handshake` tests**: The probe connects to TCP:9000 and completes the multistream-select negotiation up to the point of proposing `/noise`. If the remote acknowledges `/multistream/1.0.0` (proving it is a live libp2p node), the probe reports success and additionally reports whether it accepted `/noise`. This is sufficient to confirm:
1. TCP:9000 is reachable.
2. The remote is running libp2p software (not just a TCP listener).
3. The remote's multistream-select protocol layer is functional.

**Why not the full Noise XX handshake**: Completing the Noise handshake would require X25519 key generation and ChaCha20-Poly1305 encryption. While these are available in Rust (`x25519-dalek`, `chacha20poly1305`), adding them as dependencies significantly increases the crate's weight. More importantly, for connectivity detection purposes, a positive multistream acknowledgement is already definitive proof of libp2p reachability. The Noise handshake would not add diagnostic information for the censorship detection use case.

#### 4.10.3 Yamux / Mplex

After the Noise handshake, libp2p uses a stream multiplexer (either yamux or mplex) to allow multiple logical streams over a single TCP connection. These are not relevant to connectivity detection and are not probed.

#### 4.10.4 GossipSub

GossipSub is the pub/sub protocol used for disseminating beacon blocks, attestations, and other consensus messages. It runs inside the multiplexed libp2p connection. Not probed (requires full participation in the consensus network).

### 4.11 Beacon REST API

The Ethereum Beacon API (standardised by the Ethereum Foundation) is a REST HTTP API served by consensus clients on port 5052 (or exposed via a reverse proxy on 443). Public nodes (PublicNode, ChainSafe Lodestar) expose this without an API key.

The endpoint `GET /eth/v1/node/version` returns the client version:
```json
{"data":{"version":"Lighthouse/v4.5.0-..."}}
```

This is the lightest possible read — no authentication, no consensus state required, just a version string. It is always available on synced nodes.

**What `beacon_https` tests**: Whether the consensus data plane is reachable over HTTPS. This is the consensus-layer equivalent of the `https_jsonrpc` probe. If an adversary blocks access to consensus data (block headers, attestations, finality), users cannot verify the chain state independently.

---

## 5. Censorship Vectors in the Ethereum Network

This section maps each possible censorship technique to the probes that detect it.

### 5.1 DNS Blocking

**Technique**: ISP or national DNS resolver returns NXDOMAIN or a wrong IP for Ethereum-related domains.
**Detected by**: `dns_resolve` (failure) + `dns_compare` (system empty, DoH returns IPs → `dns_block`).
**Distinguishing normal failure**: Both resolvers failing simultaneously suggests a network outage, not targeted censorship.

### 5.2 Port / IP Blocking (Firewall)

**Technique**: Firewall drops or resets TCP/UDP packets to specific ports.
**Detected by**:
- Port 443 blocked: `tcp_connect` fails despite DNS succeeding.
- Port 30303 TCP blocked: `p2p_tcp_connect` fails.
- Port 30303 UDP blocked: `discv4_ping` fails despite TCP working.
- Port 9000 TCP blocked: `beacon_tcp_connect` / `libp2p_handshake` fail.
- Port 9000 UDP blocked: `beacon_discv5_ping` fails.

**Key diagnostic pattern**: `tcp_connect` (443) succeeds but `p2p_tcp_connect` (30303) fails → selective Ethereum P2P port blocking, while general HTTPS traffic is unaffected.

### 5.3 SNI-Based TLS Blocking

**Technique**: Firewall inspects TLS ClientHello SNI field and resets connections to targeted Ethereum domains on port 443.
**Detected by**: `tcp_connect` succeeds (TCP:443 open) but `tls_handshake` fails → SNI blocking.
**Note**: This is invisible to probes that only test TCP. The dedicated TLS probe is essential to isolate this vector.

### 5.4 Deep Packet Inspection (DPI) — Protocol Blocking

**Technique**: Stateful firewall with DPI capability identifies protocol-specific byte patterns and resets connections.
**Detected by**:
- `rlpx_handshake` fails despite TCP:30303 connecting → DPI blocking RLPx auth message pattern.
- `libp2p_handshake` fails despite TCP:9000 connecting → DPI blocking multistream-select pattern.
**Rarity**: DPI-based Ethereum blocking is not documented in the wild (as of 2025), but the probes are included as the technique is technically feasible and has been used against other protocols (Tor, OpenVPN).

### 5.5 Application-Layer Transaction Filtering

**Technique**: RPC provider accepts connections and returns data for read requests, but rejects specific write requests (e.g., transactions involving sanctioned addresses, as per OFAC compliance).
**Detected by**: `https_jsonrpc` (read) succeeds but `https_jsonrpc_write` is blocked (HTTP 403, timeout, or connection closed instead of JSON-RPC error response).
**Note**: When a provider correctly rejects a write on policy grounds, it still returns a JSON-RPC error response (code -32000 or similar) — the connection is not cut. A genuine write block would manifest as a network-level failure, not a JSON-RPC error.

### 5.6 DNS Poisoning (Distinct from Blocking)

**Technique**: ISP resolver returns a wrong but valid-looking IP, routing traffic to a block page or dead end.
**Detected by**: `dns_compare` with `ip_mismatch: true` + TCP or TLS failure on the returned IP.
**Complication**: CDN-based providers (Cloudflare, AWS CloudFront) legitimately return different IPs for different geographic regions (anycast). A mismatch alone is not a censorship signal; it must be combined with a subsequent TCP/TLS failure.

---

## 6. System Architecture

### 6.1 Workspace Structure

The project is a Cargo workspace with three members:

```
eth-conn-probes/
├── crates/
│   ├── prober_core/       # Library: probes, config, model, reporting
│   └── prober_cli/        # Binary: CLI entry point
├── app/
│   └── src-tauri/         # Tauri desktop application
├── server/                # Docker Compose: Postgres + HTTP report collector
└── config/
    └── default.toml       # Default CLI configuration
```

### 6.2 prober_core

The core library. Its public API surface is minimal:

```rust
// Entry point: run all probes from a Config, produce a Report.
pub async fn run_plan(cfg: config::Config) -> anyhow::Result<model::Report>
```

Internally:
- `config.rs`: TOML-deserialised configuration types + `default_config()` for the Tauri app.
- `model.rs`: `Report`, `ProbeRun`, `AttemptResult`, `ProbeSummary`, `ProbeKind` types.
- `rlp.rs`: Minimal RLP encoder (byte strings, unsigned integers, lists).
- `probes/mod.rs`: `build_jobs()` (creates all probe jobs from config) + `run_jobs()` (executes with tokio semaphore).
- `probes/*.rs`: One file per probe type, each implementing `ProbeFn`.
- `reporting.rs`: `send_report()` — HTTP POST to the collector server.

#### Feature Flags

prober_core uses Cargo features to gate optional, heavy dependencies:

| Feature | Added dependencies | What it enables |
|---------|-------------------|-----------------|
| `discv4` | `k256`, `sha3`, `rand` | DiscV4 UDP ping (secp256k1 signing, keccak256) |
| `discv5` | `discv5` crate | DiscV5 ping + ENR decoding for beacon TCP |
| `rlpx` | `k256`, `sha3`, `rand`, `aes`, `ctr`, `cipher`, `hmac`, `sha2` | RLPx ECIES auth handshake |

TLS, write probe, WSS subscribe, and libp2p multistream use only dependencies already present in the crate (`rustls`, `tokio-rustls`, `webpki-roots`, `tokio`, `reqwest`, `tokio-tungstenite`) and require no feature gate.

### 6.3 prober_cli

A thin binary crate. Provides two subcommands:
- `run --config <path> [--no-send] [--pretty]`: Loads config, runs probes, prints JSON, optionally sends to server.
- `print-default-config`: Dumps the embedded default TOML.

### 6.4 Tauri Desktop Application

The Tauri app bundles prober_core and exposes two commands to the frontend (Svelte/TypeScript):
- `run_probes(no_send: bool)`: Runs all probes with the hardcoded `default_config()`. If `send_report` fails, appends the report to `queued_reports.jsonl` in AppData for later retry.
- `flush_queued_reports()`: Drains the offline queue.

The app generates and persists a `client_id` UUID in AppData on first launch. This UUID is included in every report, allowing the server to correlate multiple runs from the same device without collecting any PII.

### 6.5 Report Collector Server

Docker Compose stack: Postgres 16 + Rust HTTP server.
- `POST /report`: Accepts JSON reports from CLI and Tauri clients. Intended for GeoIP enrichment and storage.
- `GET /ping`: Health check → `ok`.

The Postgres instance is internal-only (no exposed port). The server mounts a GeoIP database for server-side location attribution.

### 6.6 Data Model

```
Report
├── run_id: UUID
├── timestamp: ISO 8601
├── started_at_ms / finished_at_ms: u128
├── client: ClientInfo { os, arch, client_id, app_channel }
├── run: RunConfig { attempts, min_successes, timeout_ms, parallelism }
└── results: Vec<ProbeRun>
    └── ProbeRun
        ├── kind: ProbeKind
        ├── target: String (human-readable label)
        ├── attempts: Vec<AttemptResult>
        │   └── AttemptResult { ok, rtt_ms, error, meta: JSON }
        └── summary: ProbeSummary { success_count, failure_count, min/avg/max_rtt_ms, ok }
```

`ok` in `ProbeSummary` is `true` iff `success_count >= min_successes` (default: 1 out of 3 attempts).

### 6.7 Retry and Parallelism Mechanism

All probes run concurrently, limited by a tokio semaphore of size `parallelism` (default: 16 for CLI, 20 for Tauri). Each probe runs `attempts` times sequentially (default: 3). The `min_successes` threshold (default: 1) determines the overall pass/fail for a probe.

The three sequential attempts per probe are intentional: intermittent failures (due to packet loss or transient server errors) should not trigger false positives. A probe that succeeds on attempt 3 of 3 is still reported as `ok`.

---

## 7. Implemented Probes

### 7.1 `dns_resolve`

**Target type**: `[[probes.tcp]]` entries (auto-derived alongside TCP probes).
**Transport**: UDP/TCP (system resolver).
**What it tests**: System DNS resolution of a hostname.
**Success**: At least one A/AAAA record returned.
**Meta**: List of resolved IP addresses.

### 7.2 `dns_compare`

**Target type**: `[[probes.dns_compare]]`.
**Transport**: UDP (system resolver) + HTTPS to `1.1.1.1`.
**What it tests**: Compares system resolver output to Cloudflare DoH.
**Success**: System resolver returns at least one IP.
**Meta**: `system_ips`, `doh_ips`, `ip_mismatch`, `category` (`ok` / `dns_block` / `dns_failure`).

### 7.3 `tcp_connect`

**Target type**: `[[probes.tcp]]` (auto-derived alongside DNS probes).
**Transport**: TCP.
**What it tests**: TCP 3-way handshake to port 443.
**Success**: Connection established.

### 7.4 `tls_handshake`

**Target type**: Auto-derived from `[[probes.tcp]]` entries (same hosts, same ports).
**Transport**: TCP then TLS.
**What it tests**: Full TLS handshake (ClientHello → ServerHello → certificate verification → Finished). No HTTP request is sent.
**Success**: Handshake completes without error.
**Meta**: `tls_version`, `cipher`, `cert.subject`, `cert.issuer`, `cert.not_after`.
**Design decision**: Auto-derived from TCP targets because TLS probes the next layer above TCP; all TCP:443 targets should also support TLS. This mirrors how DNS probes are auto-derived from TCP targets.

### 7.5 `https_json_rpc` (read)

**Target type**: `[[probes.https_jsonrpc]]`.
**Transport**: HTTPS.
**Method**: `eth_chainId`.
**Success**: HTTP 200 with a JSON-RPC `result` field.
**Meta**: HTTP status, category, full JSON response.

### 7.6 `https_json_rpc_write`

**Target type**: Auto-derived from `[[probes.https_jsonrpc]]` entries (same URLs).
**Transport**: HTTPS.
**Method**: `eth_sendRawTransaction` with payload `"0x"` (empty/invalid).
**Success**: Any JSON-RPC response received (even `{"error":{...}}`). This proves the write endpoint is accessible and processing requests.
**Failure**: Network-level error (timeout, 403, connection refused) indicating the write endpoint is blocked.
**Meta**: HTTP status, category, response body.
**Design decision**: Auto-derived because write-access censorship targets the same URLs as read access. Keeping them as one config section avoids duplication.

### 7.7 `wss_json_rpc`

**Target type**: `[[probes.wss_jsonrpc]]`.
**Transport**: WSS.
**Method**: `eth_chainId`.
**Success**: WebSocket handshake completes and JSON-RPC result received.

### 7.8 `wss_subscribe`

**Target type**: Auto-derived from `[[probes.wss_jsonrpc]]` entries.
**Transport**: WSS.
**Method**: `eth_subscribe` with `["newHeads"]`.
**Success**: Subscription confirmation received (response contains `result` with a subscription ID string).
**Design decision**: Checks only the subscription confirmation, not a block notification. Block time (~12s) exceeds the default probe timeout (5s). Confirming `eth_subscribe` works end-to-end is sufficient — it requires a live WebSocket connection, a functioning RPC endpoint, and a running eth_subscribe handler.

### 7.9 `http_control`

**Target type**: `[probes.control_http]` (single target, disabled by default).
**Transport**: HTTP.
**What it tests**: The project's own report collector server (`/ping`). Disabled until the server is deployed.

### 7.10 `p2p_tcp_connect`

**Target type**: `[[probes.p2p_boot_nodes]]`.
**Transport**: TCP to port 30303.
**What it tests**: TCP reachability of Ethereum Foundation execution boot nodes on the P2P port.

### 7.11 `discv4_ping`

**Target type**: `[[probes.discv4_execution]]`.
**Transport**: UDP to port 30303.
**What it tests**: Full DiscV4 UDP handshake (PING → PONG, with endpoint proof handling).
**Feature**: Requires `discv4` Cargo feature.

### 7.12 `rlpx_handshake`

**Target type**: `[[probes.rlpx_targets]]` (enode:// URLs).
**Transport**: TCP to port 30303, then RLPx ECIES auth.
**What it tests**: Whether the RLPx protocol layer is reachable — i.e., whether a DPI firewall is dropping RLPx auth packets despite TCP being open.
**Success**: Remote sends any data back after receiving the auth message.
**Feature**: Requires `rlpx` Cargo feature.

### 7.13 `beacon_tcp_connect`

**Target type**: Auto-derived from `[[probes.discv5_consensus]]` ENRs (when `discv5` feature is enabled). IP and TCP port are extracted from the ENR's `ip4` and `tcp4` fields using the discv5 crate's accessor methods.
**Transport**: TCP to port 9000.
**What it tests**: TCP reachability of consensus boot nodes on the libp2p port.
**Design decision**: ENR is the single source of truth for consensus node parameters. Deriving the TCP target from the ENR avoids duplicating IPs in the config, which would create a maintenance burden and risk divergence.

### 7.14 `beacon_discv5_ping`

**Target type**: `[[probes.discv5_consensus]]` (ENR strings).
**Transport**: UDP to port 9000 (from ENR).
**What it tests**: DiscV5 peer discovery to consensus boot nodes.
**Feature**: Requires `discv5` Cargo feature.

### 7.15 `beacon_https`

**Target type**: `[[probes.beacon_https]]`.
**Transport**: HTTPS.
**Endpoint**: `GET /eth/v1/node/version`.
**Success**: HTTP 200 with JSON containing `data.version` field.

### 7.16 `libp2p_handshake`

**Target type**: Auto-derived from `[[probes.discv5_consensus]]` ENRs (same IP + TCP port as `beacon_tcp_connect`).
**Transport**: TCP to port 9000, then multistream-select.
**What it tests**: Whether the remote is a live libp2p node responding to the multistream negotiation protocol. Specifically: sends `/multistream/1.0.0\n` and `/noise\n` proposals, checks for acknowledgement.
**Success**: Remote acknowledges `/multistream/1.0.0` (confirming it is a libp2p node).
**Meta**: Whether `/noise` was also accepted.
**Feature**: Requires `discv5` feature (to decode ENR for IP:port). The libp2p probe itself has no additional deps.
**Design decision**: Shares ENR-derived targets with `beacon_tcp_connect` and `beacon_discv5_ping`. One config entry (the ENR) generates three probes: DiscV5 UDP ping, TCP connect, and libp2p multistream. This is consistent with how one `[[probes.tcp]]` entry generates DNS + TCP + TLS probes.

---

## 8. Connectivity by Participant Type

This section maps each Ethereum participant type to the probes that cover their connectivity requirements.

### 8.1 As a Wallet / dApp User

A wallet connects to Ethereum exclusively through JSON-RPC. It does not participate in P2P networks. Its connectivity requirements are:
- DNS resolution of provider domains → `dns_resolve`, `dns_compare`.
- TCP:443 to provider → `tcp_connect`.
- TLS handshake → `tls_handshake`.
- Read access: `eth_chainId`, `eth_blockNumber`, `eth_getBalance` → `https_json_rpc`.
- Write access: `eth_sendRawTransaction` → `https_json_rpc_write`.
- Real-time subscriptions (DApp): `eth_subscribe` → `wss_subscribe`.

**Verdict**: Fully covered by the combination of these probes. The `https_json_rpc_write` probe specifically catches the OFAC-type censorship that is most relevant to wallet users.

### 8.2 As a Full Node Operator

A full node operator needs:
- RPC provider access (same as wallet) for bootstrapping.
- Execution P2P: DiscV4 peer discovery (UDP:30303) → `discv4_ping`.
- Execution P2P: RLPx transport (TCP:30303) → `p2p_tcp_connect`, `rlpx_handshake`.
- (Not probed): ETH protocol block sync — see Section 10.

**Verdict**: The combination of `discv4_ping` + `p2p_tcp_connect` + `rlpx_handshake` covers discovery, transport, and protocol-layer reachability. The one uncovered gap is the ETH protocol `Status` exchange, which would confirm full node sync capability.

### 8.3 As a Consensus Node / Validator

A consensus node needs:
- Beacon chain access: Beacon HTTPS API → `beacon_https`.
- Consensus P2P discovery: DiscV5 UDP:9000 → `beacon_discv5_ping`.
- Consensus P2P transport: TCP:9000 → `beacon_tcp_connect`.
- Consensus P2P protocol: libp2p → `libp2p_handshake`.

**Verdict**: Discovery, transport, and protocol-layer reachability are all covered. The uncovered layer is full GossipSub participation (attestation/block gossip), which requires completing the Noise handshake and stream multiplexing.

### 8.4 As an Archive Node

An archive node is a superset of a full node from a networking perspective. The same probes apply. Archive nodes serve the same P2P protocols on the same ports.

---

## 8bis. Probe Result Interpretation: A Worked Example

This section provides a rigorous, structured interpretation of a representative probe run. The results shown were collected on 2026-05-14 from Spain (AS3352, Telefónica de España), and are used to demonstrate what each result pattern means, how to cross-validate results across probe types, and which combinations indicate genuine anomalies vs expected behaviour. The interpretation methodology is general and applies to any run.

---

### Section 1 — RPC Provider Availability (44 probes: 42 OK, 2 FAIL)

#### 1.1 DNS Resolution (8 probes: all OK)

| Provider | avg RTT | Range | Interpretation |
|---|---|---|---|
| publicnode, cloudflare, llamarpc, drpc, flashbots, 1rpc | ~7 ms | 0–22 ms | Cache hit on most attempts; 0 ms minimum confirms the local resolver has the entry cached |
| mevblocker | 59 ms | 0–178 ms | High variance: the 0 ms minimum proves the cache is populated on some attempts, the 178 ms maximum proves at least one attempt was a cache miss, hitting the recursive resolver cold. Indicates a short DNS TTL (common for providers that use anycast with frequent IP rotation) |
| blast | 0 ms | 0–1 ms | Always cached — very stable DNS infrastructure with long TTLs |

**Interpretation**: All 8 providers are DNS-resolvable. No DNS blocking is occurring at the ISP level. The variance in mevblocker's DNS is informational only — it reflects TTL policy, not censorship. A censorship event would show a permanent DNS failure, not a cache-miss delay.

#### 1.2 TCP Connect to Port 443 (8 probes: all OK)

The RTT values reveal the geographic topology of each provider's infrastructure:

| Provider | TCP avg RTT | Inference |
|---|---|---|
| drpc | 5 ms | CDN PoP within the same metropolitan area or country |
| publicnode, cloudflare, llamarpc, blast | 10–12 ms | CDN PoP in Spain or Iberian Peninsula |
| mevblocker | 62 ms (0–181 ms) | Inconsistent routing — some attempts hit a close PoP, others route to a distant one. High variance is a sign of load-balancer geography inconsistency, not censorship. |
| flashbots | 127 ms | No European CDN; traffic routes trans-Atlantic to US origin |
| 1rpc | 154 ms | Same: US-hosted without European presence |

**Cross-validation with DNS**: All TCP probes succeed for providers whose DNS also succeeds. The correlation is clean — DNS success followed by TCP success confirms that neither the naming layer nor the transport layer is blocked. If DNS returned NXDOMAIN but TCP to the literal IP succeeded, that would indicate DNS-level censorship while leaving the transport layer open.

#### 1.3 TLS Handshake (8 probes: all OK)

TLS RTTs include the full chain: TCP 3-way handshake + TLS ClientHello/ServerHello/Certificate/Finished. Expected relationship: `TLS RTT ≈ TCP RTT + ½ round trip` (because TLS 1.3 completes in one additional round trip above TCP setup).

| Provider | TCP avg | TLS avg | TLS overhead | Interpretation |
|---|---|---|---|---|
| publicnode | 12 ms | 21 ms | +9 ms | Correct: TLS adds ~½ round trip (~5 ms one-way) plus crypto |
| cloudflare | 12 ms | 22 ms | +10 ms | Same pattern |
| **llamarpc** | **12 ms** | **12 ms** | **0 ms** | **Anomaly: TLS adds nothing** |
| drpc | 5 ms | 22 ms | +17 ms | Higher than expected: suggests TLS hits a different server than TCP |
| mevblocker | 62 ms | 13 ms | −49 ms | Apparent negative overhead: mevblocker's TCP has high variance (2–181 ms); the TLS probe consistently hit a closer PoP (12–15 ms range), bringing the average down |
| flashbots | 127 ms | 241 ms | +114 ms | Expected for a US server: one extra round trip ≈ 127 ms |
| 1rpc | 154 ms | 286 ms | +132 ms | Same — one extra round trip |
| blast | 12 ms | 16 ms | +4 ms | Very fast TLS, close CDN PoP |

**LlamaRPC TLS anomaly**: TLS probe averages 12 ms, identical to TCP. This is physically impossible unless TLS session resumption (0-RTT in TLS 1.3) is occurring. The probe opens a fresh TCP connection, but if the CDN issues a session ticket that allows 0-RTT on reconnect, and if our rustls client presents that ticket, the TLS handshake can complete without an extra round trip. The tight RTT range (11–13 ms) vs the TCP range (4–27 ms) suggests the TLS probe consistently hits a specific close PoP where session resumption is active. This is a measurement artefact, not a result of censorship.

**What TLS failures would look like**: TLS probe failing when TCP succeeds indicates TLS interception or certificate substitution. A MITM proxy terminating TLS would present a certificate that fails verification. Successful TCP + failed TLS is the signature of deep packet inspection with SSL interception.

#### 1.4 HTTPS RPC — Read Path (8 probes: 7 OK, 1 FAIL)

The HTTPS RPC probe sends `eth_chainId` and checks for a JSON-RPC `result` field. It covers the complete application stack: TCP + TLS + HTTP + JSON-RPC server-side processing.

**Latency decomposition** (comparing HTTPS RTT to TCP RTT reveals backend processing time):

| Provider | TCP | HTTPS RPC | Backend overhead | Interpretation |
|---|---|---|---|---|
| cloudflare | 12 ms | 34 ms | ~10 ms | Minimal backend — fast proxy to execution node |
| mevblocker | 62 ms | 62 ms | ~0 ms | Backend responds as fast as the network RTT — probably caching `eth_chainId` |
| drpc | 5 ms | 62 ms | ~52 ms | Significant backend processing — drpc does per-request routing across multiple providers |
| blast | 12 ms | 152 ms | ~128 ms | High backend latency — possible deep validation pipeline or US-origin backend |
| 1rpc | 154 ms | 467 ms | ~159 ms | Very high backend overhead — 1RPC's privacy routing adds significant latency |
| flashbots | 127 ms | 347 ms | ~93 ms | US backend overhead |
| **llamarpc** | **12 ms** | **FAIL (HTTP 503)** | — | **CDN up, backend down** |

**LlamaRPC failure analysis**: DNS OK → TCP OK → TLS OK → HTTPS FAIL(503). This is the canonical CDN/backend split pattern. The CDN edge is fully operational (all lower layers succeed), but the CDN receives the HTTP request, attempts to forward it to the origin, finds the origin unreachable, and returns 503 Service Unavailable. This is a backend outage, not censorship. Censorship at the application layer would typically manifest as a TCP or TLS failure rather than a 503 from the CDN.

#### 1.5 HTTPS Write — Write Path (8 probes: 7 OK, 1 FAIL)

The write probe sends `eth_sendRawTransaction` with payload `"0x"` (an intentionally invalid transaction). Any JSON-RPC response (including an error body) is treated as success — it proves the write endpoint is reachable and processing requests.

**Write RTT vs Read RTT comparison reveals write-path handling architecture:**

| Provider | Read RTT | Write RTT | Write/Read ratio | Interpretation |
|---|---|---|---|---|
| flashbots | 347 ms | 350 ms | 1.01× | Write path is as fast as read: Flashbots is a MEV relay — their entire business is transaction submission, so the write path is a primary performance concern |
| mevblocker | 62 ms | 63 ms | 1.02× | Same: MEV Blocker's purpose is transaction routing; write path is highly optimised |
| cloudflare | 34 ms | 44 ms | 1.29× | Slight overhead: transaction validation is light, likely just syntax checking |
| publicnode | 60 ms | 144 ms | 2.40× | 2.4× overhead: backend validates the transaction structure before returning an error |
| drpc | 62 ms | 207 ms | 3.34× | High write overhead: drpc may route write requests through multiple providers or apply deeper validation |
| blast | 152 ms | 135 ms | 0.89× | Write appears faster than read — statistical noise or different backend routing for POST vs GET; not significant |
| 1rpc | 467 ms | 711 ms (max 1060 ms) | 1.52× avg, 2.27× max | Very high variance: 1RPC applies privacy routing and possibly tx simulation, which has variable latency. The 531–1060 ms range suggests non-deterministic routing latency |

**Write failures**: LlamaRPC fails for the same reason as read (backend 503). No provider actively blocked the write path beyond what was already visible at the read probe level. This means no OFAC-style write censorship is detectable from Spain as of this measurement.

#### 1.6 WSS JSON-RPC and Subscription (4 probes: all OK)

WSS probes cover: TCP + TLS + HTTP Upgrade to WebSocket + JSON-RPC request (WSS RPC) or `eth_subscribe` for new block heads (WSS Subscribe).

| Probe | Provider | RTT | Overhead above TLS | Interpretation |
|---|---|---|---|---|
| WSS RPC | publicnode | 229 ms | +208 ms | WebSocket upgrade + server-side request processing adds ~200 ms |
| WSS RPC | drpc | 275 ms | +253 ms | Same order of magnitude |
| WSS Subscribe | publicnode | 224 ms | +203 ms | Subscribe request slightly faster than regular RPC — consistent |
| WSS Subscribe | drpc | 247 ms | +225 ms | Same pattern |

The symmetry between WSS RPC and WSS Subscribe RTTs for each provider confirms these are measuring the same application path. A significant discrepancy (e.g., subscribe taking 10× longer) would indicate the subscription machinery has a different latency profile than regular requests.

**Summary of Section 1 results**: From Spain on Telefónica (AS3352), Ethereum RPC access is globally healthy. 42/44 probes succeed. The 2 failures are both LlamaRPC (backend outage, not ISP or network-level blocking). No DNS manipulation, no TLS interception, no write-path blocking is detected. Latency patterns are geographically consistent with provider infrastructure locations.

---

### Section 2 — Execution Layer P2P (18 probes: 18 OK)

#### 2.1 P2P TCP Connect to Port 30303 (4 probes: all OK)

The RTTs reveal geographic distance to each EF execution boot node:

| Node | IP | avg TCP RTT | Distance model | Ratio | Assessment |
|---|---|---|---|---|---|
| EF-hetzner-fsn | 157.90.35.166 (Falkenstein, DE) | 34 ms | ~2,100 km → ~21 ms one-way → ~42 ms expected | 0.81 | Faster than physical model — well-peered European routing |
| EF-hetzner-hel | 65.108.70.101 (Helsinki, FI) | 56 ms | ~3,000 km → ~30 ms one-way → ~60 ms expected | 0.93 | Close to physical model |
| EF-us-east | 3.209.45.79 (Virginia, US) | 102 ms | ~6,500 km → ~65 ms one-way → ~130 ms expected | 0.78 | Trans-Atlantic cable optimisation; faster than simple distance model |
| EF-ap-southeast | 18.138.108.67 (Singapore) | 156 ms | ~11,000 km → ~110 ms one-way → ~220 ms expected | 0.71 | Well below physical limit, consistent with optimised undersea cable routing |

**All 4 nodes succeed**, confirming that TCP:30303 is not filtered by the user's ISP. This is the critical baseline for detecting execution-layer P2P censorship: if TCP:443 succeeds but TCP:30303 fails uniformly, it would indicate selective port-level blocking of Ethereum P2P traffic.

**RTT ordering consistency**: Falkenstein (34 ms) < Helsinki (56 ms) < Virginia (102 ms) < Singapore (156 ms). This ordering is strictly consistent with geographic distance from Spain. Any deviation (e.g., a European node showing a US-equivalent RTT) would indicate routing anomalies worth investigating.

#### 2.2 DiscV4 UDP Ping (4 probes: all OK)

DiscV4 ping sends a signed PING datagram and waits for a signed PONG. The remote node must perform secp256k1 ECDH and signing before responding.

| Node | TCP RTT | DiscV4 UDP RTT | Δ | Interpretation |
|---|---|---|---|---|
| EF-hetzner-fsn | 34 ms | 39 ms | +5 ms | UDP 5 ms slower than TCP — server-side signing overhead (~5 ms for secp256k1) |
| EF-hetzner-hel | 56 ms | 52 ms | −4 ms | Measurement noise (UDP occasionally faster due to routing variation) |
| EF-us-east | 102 ms | 108 ms | +6 ms | Consistent with signing overhead |
| EF-ap-southeast | 156 ms | 158 ms | +2 ms | Nearly identical — signing is fast relative to network latency at this distance |

The DiscV4 RTTs are within ±10 ms of TCP RTTs for the same nodes. This is expected: UDP and TCP to the same remote IP traverse the same physical path, and DiscV4 processing overhead (secp256k1) is small relative to network latency. The fact that DiscV4 succeeds for all 4 nodes confirms that UDP:30303 is not blocked. Combined with TCP:30303 success, both the discovery and transport layers of execution P2P are fully reachable.

#### 2.3 RLPx ECIES Auth Handshake (4 probes: all OK)

RLPx auth = TCP connect + send ECIES-encrypted auth packet + wait for response (FIN, RST, or auth-ack). The total RTT covers: TCP 3-way handshake + one-way packet delivery + remote processing + one-way response delivery ≈ 2 × TCP RTT + processing.

| Node | TCP RTT | RLPx RTT | Ratio | Expected ~2× | Assessment |
|---|---|---|---|---|---|
| EF-hetzner-fsn | 34 ms | 75 ms | 2.21× | ~68 ms + 7 ms processing | Consistent |
| EF-hetzner-hel | 56 ms | 106 ms | 1.89× | ~112 ms − 6 ms | Within noise |
| EF-us-east | 102 ms | 215 ms | 2.11× | ~204 ms + 11 ms processing | Consistent |
| EF-ap-southeast | 156 ms | 325 ms | 2.08× | ~312 ms + 13 ms processing | Consistent |

All 4 ratios cluster around 2.0–2.2×, confirming the theoretical model. The excess over 2.0 (5–13 ms depending on distance) represents the remote ECIES decryption and signature verification time. This time is independent of network distance (pure CPU) and should be roughly constant across nodes — and indeed, the absolute processing time (not ratio) varies little: 7, −6, 11, 13 ms.

**Critical interpretation**: RLPx auth success means:
1. TCP:30303 is open (prerequisite)
2. The ECIES-encrypted payload was not dropped by DPI (if a DPI firewall dropped RLPx packets, no response would arrive at all, producing a timeout)
3. The remote node received the payload, attempted to decrypt it, failed (because our ephemeral key has no persistent node ID), and sent back FIN or RST

A timeout here — with TCP:30303 succeeding — would be the signature of a protocol-aware DPI firewall that specifically targets RLPx auth packets while leaving the TCP connection open. None of the 4 nodes showed this behaviour.

#### 2.4 DNS Compare (6 probes: all OK)

All 6 domains return matching results between the local resolver (Telefónica) and Cloudflare DoH (1.1.1.1). No IP mismatches detected.

| Domain | Avg RTT | Interpretation |
|---|---|---|
| All 6 | 19–51 ms | RTT covers both queries: system resolver + DoH |

**What a mismatch would mean**: If the local resolver returns different IPs than Cloudflare DoH for the same domain, it indicates DNS manipulation — either the ISP redirects queries for Ethereum domains to different servers, or there is a court-ordered DNS block implemented at the resolver level. This is a common censorship technique in some jurisdictions. The clean match across all 6 tested domains (covering both execution and consensus layer providers) confirms no DNS manipulation is active for Ethereum on Telefónica as of this measurement.

---

### Section 3 — Consensus Layer P2P (11 probes: 6 OK, 5 FAIL)

#### 3.1 DiscV5 Beacon Ping (3 probes: all OK)

| Node | IP | avg RTT | Network model | Assessment |
|---|---|---|---|---|
| teku-aws-ohio | 3.147.37.0 (Ohio, US) | 241 ms | ~100 ms one-way → ~200 ms net → +41 ms processing | Server-side DiscV5 crypto adds ~41 ms |
| teku-aws-sydney | 3.107.124.68 (Sydney, AU) | 557 ms | ~170 ms one-way → ~340 ms net → +217 ms processing | High processing overhead — JVM-based Teku may have higher DiscV5 response latency |
| nimbus-frankfurt | 3.120.104.18 (Frankfurt, DE) | 100 ms | ~18 ms one-way → ~36 ms net → +64 ms processing | Significant processing relative to network; Frankfurt is close, most of the RTT is server-side crypto |

DiscV5 success confirms UDP:9000 is reachable to all 3 hosts. The large server-side processing component (especially for Teku Sydney at +217 ms) is consistent with JVM garbage collection or cold code paths in the DiscV5 implementation. Nimbus (Go/Nim native) shows less processing overhead per round trip despite similar infrastructure.

#### 3.2 Beacon TCP Connect (3 probes: 2 FAIL connection refused, 1 OK)

| Node | TCP RTT | Reason | Interpretation |
|---|---|---|---|
| teku-aws-ohio | 2585 ms per attempt | Connection refused (errno 10061) | See below |
| teku-aws-sydney | 3398 ms per attempt | Connection refused (errno 10061) | See below |
| nimbus-frankfurt | 27 ms | OK — TCP:9100 connects | Port 9100 is Prometheus metrics, not libp2p (see §3.3) |

**Teku connection refused with anomalous latency**: Connection refused (errno 10061 / WSAECONNREFUSED) means the remote OS kernel sent a TCP RST in response to the SYN. This normally takes one round trip: if the host is in Ohio (~100 ms round trip), a connection refused should arrive in ~100 ms. The observed 2585 ms per attempt — 25× longer than the geographic model — reveals a specific firewall behaviour pattern:

The most consistent explanation is that the AWS security group (or host-level firewall) is configured as a DROP rule for TCP:9000. The SYN is silently discarded. The client's OS retransmits the SYN after ~1 second (Windows default SYN retransmission timeout). The second SYN hits a different rule or the same firewall, which this time sends RST. The total elapsed time at the client becomes: initial SYN wait (~1000 ms) + second SYN + round trip for RST (~100 ms) ≈ 1100 ms. Since all 3 attempts consistently produce 2577–2593 ms, the actual timing involves likely 2 SYN retransmissions before RST:

```
t=0 ms:     SYN sent → silently dropped by firewall
t=1000 ms:  SYN retransmit #1 → silently dropped
t=1100 ms:  [based on Ohio's ~100 ms one-way]
t=~2400 ms: SYN retransmit #2 → RST sent by firewall
t=~2500 ms: RST received by client → ECONNREFUSED raised
```

This matches the observed 2577–2593 ms. The distinction between immediate REJECT (would produce ~100 ms RTT) and this "DROP then RST after retransmits" behaviour is meaningful: it indicates a stateful firewall that tracks SYN count and only responds after repeated attempts, likely as an anti-scan measure. The key diagnostic outcome is the same — TCP:9000 is blocked for inbound libp2p connections — but the timing pattern reveals the underlying firewall mechanism.

For Teku Sydney at 3398 ms, the same pattern applies with longer round-trip time (~170 ms to Sydney), making each retransmit cycle longer.

#### 3.3 libp2p Multistream-Select Handshake (3 probes: all FAIL)

| Node | Port | Result | Reason |
|---|---|---|---|
| teku-aws-ohio | 9000 | Connection refused (errno 10061) | TCP fails before libp2p bytes are sent |
| teku-aws-sydney | 9000 | Connection refused (errno 10061) | Same — TCP blocked |
| nimbus-frankfurt | 9100 | Timeout | TCP connects (port 9100 = Prometheus) but multistream bytes go unanswered |

The Teku failures are upstream TCP failures — the libp2p probe never reaches the protocol layer because TCP connect itself fails. The RTT (2582–2585 ms for Ohio, 3379–3400 ms for Sydney) matches the Beacon TCP RTT exactly, confirming both probes hit the same TCP connection failure.

Nimbus is a different failure mode: TCP:9100 connects successfully (as shown by the Beacon TCP probe), but port 9100 is the Prometheus metrics HTTP server. The libp2p probe sends the multistream negotiation header (`/multistream/1.0.0\n`) into an HTTP server that is waiting for an HTTP GET request. The HTTP server either ignores the non-HTTP bytes or waits for a complete HTTP request before responding. Either way, the probe reaches its 5000 ms timeout with no response, producing a timeout rather than a refused connection.

**Combined interpretation of Section 3 failures**: None of these libp2p failures represent network-level censorship. They are all deliberate operator configuration choices:
- Teku explicitly firewalls inbound TCP:9000 — boot node is discovery-only
- Nimbus publishes `tcp4=9100` in its ENR pointing to a non-libp2p port

The DiscV5 UDP success for all 3 nodes rules out host-level unreachability — the issue is port-specific and TCP-specific.

#### 3.4 Beacon API (2 probes: both OK)

| Provider | avg RTT | Interpretation |
|---|---|---|
| publicnode | 83 ms | CDN-cached or close edge for the beacon API endpoint |
| chainsafe-lodestar | 168 ms | US-hosted Lodestar node without European CDN |

Both serve `GET /eth/v1/node/version` successfully, confirming consensus chain data is accessible over HTTPS without authentication or censorship from Spain.

---

### Overall Assessment and Cross-Section Conclusions

**Connectivity profile from Spain (AS3352, Telefónica), 2026-05-14**:

| Layer | Status | Notes |
|---|---|---|
| DNS (Ethereum domains) | ✓ Clean | No manipulation vs Cloudflare DoH; one provider has short TTL causing high variance |
| TCP:443 (HTTPS/TLS) | ✓ Fully reachable | All 8 RPC providers; CDN topology consistent with geographic model |
| TLS negotiation | ✓ No interception | Valid certificates; no MITM proxy detected |
| HTTPS RPC read | ✓ 7/8 providers | LlamaRPC backend outage (not censorship) |
| HTTPS RPC write | ✓ 7/8 providers | No OFAC-style write blocking detected |
| WebSocket (WSS) | ✓ Fully functional | Subscriptions working on 2 providers tested |
| TCP:30303 (execution P2P) | ✓ Fully reachable | 4 EF boot nodes; latency consistent with geography |
| DiscV4 UDP:30303 | ✓ Fully functional | Discovery layer reachable |
| RLPx ECIES auth | ✓ No DPI filtering | Auth packets reach remote nodes; no DPI drop signature detected |
| DiscV5 UDP:9000 | ✓ Fully functional | 3 consensus boot nodes reachable |
| TCP:9000 (libp2p) | ✗ Firewalled at targets | Teku deliberately blocks inbound; not ISP-level |
| libp2p multistream | ✗ Not reached | Blocked by TCP or wrong port |
| Beacon HTTPS API | ✓ 2/2 providers | Consensus data accessible |

**Outbound connectivity**: Essentially unconstrained from this network for Ethereum use. All critical layers (DNS, TCP:443, TCP:30303, UDP:30303, UDP:9000) are open. No evidence of ISP-level filtering of any Ethereum protocol.

**Gap not measured**: All probes above test *outbound* connectivity — whether the client can reach Ethereum infrastructure. The complementary question — whether Ethereum infrastructure (or peers) can reach the client — is not currently tested. This is the most common gap for residential users operating full or consensus nodes, where inbound connections to port 30303 or 9000 may be blocked by CGNAT or ISP policy regardless of outbound freedom. See §12 for the reverse connectivity design.

---

## 9. Technical Decisions and Justifications

### 9.1 Separate `[[probes.rlpx_targets]]` with enode:// URLs

**Why not extend `p2p_boot_nodes`?** The `p2p_boot_nodes` section contains `TcpTarget` entries with only `name`, `host`, and `port`. The RLPx handshake additionally requires the remote node's static public key for ECIES encryption. The enode:// URL is the standard format for carrying this information in the execution layer (analogous to ENR in the consensus layer). Adding an optional `pubkey` field to `TcpTarget` would violate the single-responsibility principle of each config type. A dedicated `RlpxTarget` with an `enode` string is self-contained and maps directly to the protocol's identity format.

### 9.2 Beacon TCP and libp2p Targets Derived from ENR

**Why not a separate `[[probes.beacon_boot_tcp]]` section?** ENR is the canonical source of truth for consensus node parameters. The `ip4` and `tcp4` fields in an ENR are signed by the node's private key — they are authoritative and tamper-evident. Duplicating IPs in a separate config section would create two sources of truth that could diverge. By deriving TCP targets from ENRs programmatically (using the discv5 crate's accessor methods when the `discv5` feature is enabled), we maintain a single source of truth and ensure consistency. When the feature is disabled, beacon TCP and libp2p probes are simply omitted.

### 9.3 TLS Probes Auto-Derived from TCP Entries

TLS is the mandatory next layer above TCP on port 443. Every TCP:443 target in the config necessarily supports TLS — if it did not, the HTTPS and WSS probes that follow would fail for a different reason. Auto-deriving TLS targets from the `tcp` list avoids redundant config entries and follows the same pattern used for DNS probes.

### 9.4 Write and Subscribe Probes Auto-Derived from Existing Targets

The `https_jsonrpc_write` probe targets the same URLs as `https_jsonrpc`. The `wss_subscribe` probe targets the same URLs as `wss_jsonrpc`. These are companion probes that test the same endpoint from a different angle (write access vs. read access; subscription vs. one-shot query). Keeping them as separate config sections would add no user value while increasing config verbosity.

### 9.5 libp2p Multistream Only (not Noise XX)

The Noise XX handshake for libp2p would require X25519 key generation and ChaCha20-Poly1305 encryption — two new cryptographic primitives. The `snow` crate (a Rust Noise implementation) or `chacha20poly1305` from RustCrypto would be needed. The diagnostic value added over multistream-select completion is minimal: if a DPI firewall were specifically targeting the libp2p Noise pattern (as opposed to the multistream pattern), it would be an extraordinarily targeted and rare attack. For practical censorship detection, confirming `/multistream/1.0.0` is acknowledged is sufficient to establish that libp2p is reachable and running.

### 9.6 RLPx Feature-Gated

The RLPx probe requires AES-128-CTR, HMAC-SHA256, and SHA-256 beyond what DiscV4 already uses. These add four compilation units (`aes`, `ctr`, `hmac`, `sha2`) plus the shared `cipher` crate. Because these crates add measurable compile time and binary size, the probe is gated behind a `rlpx` Cargo feature, consistent with the existing `discv4` and `discv5` feature gates.

### 9.7 rustls CryptoProvider Selection

`rustls 0.23` removed the historical behaviour of automatically selecting a crypto backend at link time. When a binary compiles in more than one backend — `ring` (pulled directly by `prober_core`) and `aws-lc-rs` (pulled transitively by `reqwest`) — calling `ClientConfig::builder()` panics at runtime with:

```
Could not automatically determine the process-level CryptoProvider from Rustls crate features.
Call CryptoProvider::install_default() before this point or ensure exactly one of the
`ring` and `aws-lc-rs` features is enabled.
```

Because `reqwest` uses `aws-lc-rs` as its default TLS backend and `prober_core` explicitly requests `rustls/ring` for its own TLS handshake probe, both backends end up in the same binary. Two measures are required:

1. **Process-wide default** (`lib.rs`, `run_plan`): `let _ = rustls::crypto::ring::default_provider().install_default();`  
   This installs `ring` as the global provider before any tokio tasks are spawned. It returns `Err` if already set, so `let _` silently ignores double-calls (safe for re-entrant use). This also fixes WSS probes — `tokio-tungstenite` calls `ClientConfig::builder()` internally.

2. **Explicit provider at build site** (`tls_handshake.rs`, `make_connector`): `ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))`.  
   This makes the TLS connector construction immune to the state of the global default, which may not yet be installed if `make_connector` is ever called before `run_plan`.

**Why prefer `ring` over `aws-lc-rs`**: `ring` is a pure-Rust crate (no C compilation step, no system library dependency). `aws-lc-rs` requires a C toolchain and cmake for its C/assembly core. `ring` is therefore safer for cross-compilation and simpler CI, which is important given the project's cross-platform (Linux, macOS, Windows) target. The choice is locked in `Cargo.toml` by `rustls = { default-features = false, features = ["ring"] }`.

### 9.8 RLPx Auth Success Condition — Any Response Means No DPI

The RLPx Auth probe's original success condition required reading at least one byte from the remote after sending the auth packet (`read_exact`). This was wrong: boot nodes always reject our ephemeral identity (we have no persistent node ID) by closing the connection with FIN or RST immediately after receiving the auth packet. `read_exact` propagates both EOF and RST as errors, so the probe reported failure for every boot node even when they clearly processed our auth.

The corrected condition (using `read()` instead of `read_exact()`):
- **Ok(0)** (FIN/EOF): remote processed auth and closed gracefully → **ok**.
- **Ok(n>0)** (data): remote sent auth-ack or disconnect → **ok**.
- **Err(RST)**: remote processed auth and reset abruptly → **ok**.
- **timeout**: auth packet may have been dropped by DPI before reaching remote → **fail**.

The only censorship signal is a timeout on an otherwise-open TCP:30303 connection. FIN and RST both prove the auth packet traversed the full network path and was received by the remote — no DPI firewall intercepted it. The error message on timeout was updated to "rlpx handshake timed out — auth packet may be DPI-filtered" to make the diagnostic meaning explicit.

### 9.10 Tokio Task Panic Recovery in run_jobs

Prior to this fix, `run_jobs` collected `JoinHandle<ProbeRun>` values and unpacked them with `if let Ok(r) = h.await`. Any tokio task that panicked (e.g., due to a rustls `ClientConfig::builder()` panic) produced `Err(JoinError::Panicked)`, which `if let Ok` silently discarded. The result was that all TLS and WSS probes simply disappeared from the output — no error message, no entry in the report.

The fix stores `(ProbeKind, target_label, JoinHandle)` tuples alongside each handle and uses a `match` on `h.await`:
- `Ok(r)` → normal result, pushed to output.
- `Err(e)` → synthetic `ProbeRun` with `ok=false` and `error=Some(format!("probe task panicked: {e}"))`, so the failure appears in the report and in the server-side aggregation.

This is strictly more correct: a probe that fails due to an internal bug should appear as a failed probe, not vanish. The tracing call (`tracing::error!`) additionally surfaces the panic in log output.

### 9.11 Write Probe Success Condition: JSON-RPC Body Wins Over HTTP Status

The `https_jsonrpc_write` probe originally matched on HTTP status code first, treating only `HTTP 200` + JSON-RPC body as success. Flashbots exposed the flaw: it validates raw transaction format at the HTTP layer and returns `HTTP 400` (with a JSON error body, e.g. `{"error":"invalid params"}`) instead of `HTTP 200` + JSON-RPC error. The status-first match classified this as a generic HTTP error.

The corrected logic checks for a JSON-RPC body first, regardless of HTTP status:
- If the response body parses as JSON and contains `error` or `result` → **ok** (write endpoint is alive and processing).
- If no JSON-RPC body → fall back to status-based categorisation (`401`/`403` = auth_required, `429` = rate_limited, other = http_error).

This correctly handles Flashbots (HTTP 400 + JSON body → ok), standard providers (HTTP 200 + JSON body → ok), and genuine censorship signals (HTTP 503 with HTML body → http_error, timeout → network error).

**Why Flashbots returns HTTP 400 specifically**: Flashbots runs a relay that validates transaction format and MEV-boost rules before forwarding. Submitting `"0x"` (empty bytes) fails their payload validation at the HTTP request handling layer, before any JSON-RPC dispatcher runs. This is a provider-specific implementation choice, not censorship.

### 9.12 eth_sendRawTransaction with "0x" Payload

Sending a real Ethereum transaction to test write access would require:
1. An account with ETH balance (to pay gas).
2. Signature infrastructure (private key management).
3. On-chain traces (spam risk, dust accumulation).

Sending an invalid payload (`"0x"`) avoids all of these problems. All major RPC providers respond to invalid raw transactions with a JSON-RPC error, not with a network-level block. This makes `"0x"` a reliable write-access canary: the error code and message don't matter, only whether a JSON-RPC response (vs. a network failure) is returned.

---

## 10. What Is Not Implemented and Why

### 10.1 ETH Protocol (Full Node Sync Simulation)

After a successful RLPx handshake, two nodes exchange ETH protocol `Status` messages: `network_id`, `genesis_hash`, and the current `fork_id` (EIP-2124) which encodes which hard forks the node has applied. If both `Status` messages match, the nodes become peers and exchange block headers.

**Why not implemented**: The `Status` exchange would require knowing the current head block and fork ID — information that requires a local or remote eth node to obtain. A prober that probes connectivity should not itself need to be a full node. More fundamentally, censors blocking Ethereum P2P connectivity do so at the TCP/UDP/protocol layer, not at the ETH `Status` message layer — no known censorship implementation targets the ETH `Status` message specifically. The `rlpx_handshake` probe already captures any DPI-based protocol filtering.

### 10.2 LES (Light Ethereum Subprotocol)

LES is the protocol that full nodes used to serve light clients. It has been disabled in Geth (2023) and is not served by any major client. Adding a LES probe would test a protocol with no live network to test against.

### 10.3 Portal Network

Portal Network is the modern replacement for light clients, using a custom DHT over uTP (UDP-based reliable transport). It is actively developed (2025) with a small but growing node count. It is omitted for two reasons: (1) the network is small enough that connectivity failures would be ambiguous (node unavailability vs. censorship), and (2) the uTP transport is novel and would require substantial implementation work. It is listed as future work.

### 10.4 Full Noise XX Handshake (libp2p)

See Section 9.5. The multistream-select layer is sufficient for connectivity detection.

### 10.5 GossipSub Participation

Full participation in the consensus gossip network (subscribing to beacon block and attestation topics) would require completing the full libp2p stack: Noise handshake, yamux multiplexing, and GossipSub protocol negotiation. This is the level of a production consensus client. It is inappropriate for a connectivity probe: it would require syncing the beacon chain state, consuming significant bandwidth, and would constitute meaningful participation in the consensus network (generating load on boot nodes).

### 10.6 Reverse Connectivity (Inbound Testing)

A full node needs to be reachable from the internet (inbound connections on TCP/UDP:30303, TCP/UDP:9000). Testing inbound reachability from a client probe requires external infrastructure: a server that attempts to connect back to the client's IP. This would require NAT traversal logic and coordination between the client and server components. It is architecturally different from all current probes (which are purely outbound) and is listed as future work, requiring the server component to actively probe discovered client IPs.

### 10.7 WalletConnect Relay Testing

WalletConnect is a protocol that relays communication between a desktop dApp and a mobile wallet via a WebSocket relay server. It is an ecosystem service, not a core Ethereum protocol. Probing WalletConnect relay servers would test infrastructure operated by WalletConnect (currently Reown), not Ethereum itself.

---

## 11. Target IP and Key Sourcing

This section documents the provenance of every network target used in the probe configuration.

### 11.0 How Configuration Works (Two-Config Architecture)

The project has **two parallel representations** of the default configuration that must be kept in sync manually:

1. **`config/default.toml`** — Used by the CLI binary (`cargo run -- run`). The CLI reads this file from disk at startup. It can also be overridden via `--config <path>`. This file is human-readable and easy to diff.

2. **`crates/prober_core/src/config.rs::default_config()`** — Used by the Tauri desktop app. The app does not read any file from disk; it calls `default_config()` which returns a `Config` struct built directly in Rust. This avoids file system path problems on Windows/macOS where the working directory at app launch is unpredictable.

**When targets are added or changed, both files must be updated.** Forgetting to update `config.rs` is the most common mistake — the TOML change looks correct but the app still uses the old values.

The `print-default-config` CLI subcommand is a developer aid: it serializes the Rust `default_config()` struct back to TOML, so you can verify both representations are in sync.

### 11.0b What ENRs Are and How They Are Parsed

An **ENR (Ethereum Node Record)** is a self-signed, self-describing record that identifies an Ethereum node. It is defined in [EIP-778](https://eips.ethereum.org/EIPS/eip-778). The format is:

```
enr:-<base64url(RLP([signature, seq, key1, val1, key2, val2, ...]))>
```

Key fields relevant to connectivity probing:

| ENR key | Meaning | Example |
|---------|---------|---------|
| `ip` | IPv4 address (4 bytes) | `ip4()` → `Some(3.147.37.0)` |
| `ip6` | IPv6 address (16 bytes) | `ip6()` → `Some(2400:8907::...)` |
| `tcp` | TCP port for libp2p (2 bytes) | `tcp4()` → `Some(9000)` |
| `tcp6` | TCP port for libp2p over IPv6 | `tcp6()` → `None` |
| `udp` | UDP port for DiscV5 (2 bytes) | `udp4()` → `Some(9000)` |
| `secp256k1` | Node's compressed public key (33 bytes) | used to verify signature |
| `eth2` / `attnets` | Consensus-layer metadata | fork version, attestation subnets |

The ENR is signed with the node's secp256k1 private key, so the IP/port fields are authenticated — a node cannot forge another node's ENR. The signature is verified by the `discv5` crate when parsing.

**How IPs are extracted in build_jobs**: At probe build time, `build_jobs()` calls `enr.ip4()`, `enr.tcp4()`, `enr.ip6()`, `enr.tcp6()`, `enr.udp4()`, `enr.udp6()` on each parsed ENR. These return `Option<T>`. The priority for TCP probes is: `ip4+tcp4 → ip4+udp4 (fallback) → [ip6]+tcp6/udp6`. The diagnostic test `cargo test -p prober_core --features discv5 -- enr_decode --nocapture` decodes all ENRs and prints the extracted fields, which is how the decoded-IP table in §11.4 was produced.

### 11.0c Where ENRs and Enode Pubkeys Come From

**Execution enode:// pubkeys** are sourced from [`go-ethereum/params/bootnodes.go`](https://github.com/ethereum/go-ethereum/blob/master/params/bootnodes.go). The `enode://` URL format is:

```
enode://PUBKEY_HEX@IP:PORT
```

where `PUBKEY_HEX` is the 64-byte (128 hex char) uncompressed secp256k1 public key of the remote node (without the `0x04` prefix). This pubkey is required by the RLPx probe to ECIES-encrypt the auth message so that only the remote node (which holds the matching private key) can decrypt it. The pubkey is permanent and does not change when the node moves to a new IP — which is why the EF-hetzner-hel node kept its pubkey when it moved from `52.187.207.27` to `65.108.70.101`.

**Consensus ENR strings** are sourced from each client team's official repository:

| Source file | Nodes |
|------------|-------|
| [`prysmaticlabs/prysm` `config/params/mainnet_config.go`](https://github.com/prysmaticlabs/prysm/blob/develop/config/params/mainnet_config.go) | Teku, Lighthouse, Nimbus, EF, Prysm, Pryslab |
| [`Consensys/teku`](https://github.com/Consensys/teku) | Teku nodes |
| [`sigp/lighthouse`](https://github.com/sigp/lighthouse) | Lighthouse nodes |
| [`status-im/nimbus-eth2`](https://github.com/status-im/nimbus-eth2) | Nimbus nodes |

The Prysm `mainnet_config.go` is used as the authoritative aggregator because it contains boot node ENRs from all major client teams in one file (each team contributes their own entries to a shared list). ENRs are signed by the node's private key and contain the node's public key in the `secp256k1` field, so their authenticity is verifiable — there is no trust required in the source file beyond confirming it is from the official repository.

### 11.1 RPC Provider Domains (TCP, HTTPS, WSS, DNS Compare, TLS, Beacon HTTPS)

All domain names (`ethereum-rpc.publicnode.com`, `cloudflare-eth.com`, `eth.llamarpc.com`, etc.) are resolved at connection time by the system DNS resolver or DoH. No hardcoded IPs are used for these targets. The domains are sourced from each provider's public documentation and verified to have public, unauthenticated JSON-RPC endpoints.

Providers included: PublicNode, Cloudflare, LlamaRPC, dRPC, Flashbots, 1RPC, MevBlocker, Blast API, ChainSafe Lodestar.

### 11.2 Execution Boot Node IPs (p2p_boot_nodes, discv4_execution)

Raw IPv4 addresses. Source: [`params/bootnodes.go`](https://github.com/ethereum/go-ethereum/blob/master/params/bootnodes.go) in go-ethereum (Ethereum Foundation).

| Name | IP | Region | Notes |
|------|-----|--------|-------|
| EF-ap-southeast | 18.138.108.67 | AWS ap-southeast-1 | |
| EF-us-east | 3.209.45.79 | AWS us-east-1 | |
| EF-hetzner-hel | 65.108.70.101 | Hetzner Helsinki | Formerly `EF-southeast-asia` at `52.187.207.27` (AWS); same pubkey, node relocated |
| EF-hetzner-fsn | 157.90.35.166 | Hetzner Falkenstein | New 4th EF execution boot node (`bootnode-hetzner-fsn` in go-ethereum) |

These boot nodes are maintained by the Ethereum Foundation. `EF-hetzner-hel` and `EF-hetzner-fsn` were added/updated 2026-05-14 based on current `go-ethereum` source; the old AWS Southeast Asia IP (`52.187.207.27`) was confirmed dead (TCP/DiscV4/RLPx all timeout).

### 11.3 Execution Boot Node Enode URLs (rlpx_targets)

Full enode:// URLs include the node's 64-byte secp256k1 public key. Source: same `params/bootnodes.go`. The public key is required to construct the ECIES-encrypted RLPx auth message.

```
enode://d860a01f9722d78051619d1e2351aba3f43f943f6f00718d1b9baa4101932a1f5011f16bb2b1bb35db20d6fe28fa0bf09636d26a87d31de9ec6203eeedb1f666@18.138.108.67:30303
enode://22a8232c3abc76a16ae9d6c3b164f98775fe226f0917b0ca871128a74a8e9630b458460865bab457221f1d448dd9791d24c4e5d88786180ac185df813a68d4de@3.209.45.79:30303
enode://2b252ab6a1d0f971d9722cb839a42cb81db019ba44c08754628ab4a823487071b5695317c8ccd085219c3a03af063495b2f1da8d18218da2d6a82981b45e6ffc@65.108.70.101:30303  (same pubkey as old 52.187.207.27)
enode://4aeb4ab6c14b23e2c4cfdce879c04b0748a20d8e9b59e25ded2a08143e265c6c25936e74cbc8e641e3312ca288673d91f2f93f8e277de3cfa444ecdaaf982052@157.90.35.166:30303
```

### 11.4 Active Consensus Boot Node ENRs (discv5_consensus)

After empirical testing of 13 candidate nodes, 3 remain. See §11.4b for the detailed rejection rationale for all removed candidates.

| Name | IP | tcp4 | udp4 | Source | libp2p result |
|------|-----|------|------|--------|---------------|
| teku-aws-ohio | 3.147.37.0 | 9000 | 9000 | Teku (Consensys/teku repo) | FAIL — TCP:9000 connection refused (firewalled) |
| teku-aws-sydney | 3.107.124.68 | 9000 | 9000 | Teku | FAIL — TCP:9000 connection refused (firewalled) |
| nimbus-frankfurt | 3.120.104.18 | 9100 | 9100 | Nimbus (status-im/nimbus-eth2) | FAIL — TCP:9100 connects but port is Prometheus metrics |

The Teku nodes are the most useful surviving targets: DiscV5 ping succeeds (proving UDP:9000 is reachable), and the Beacon TCP/libp2p connection-refused response proves the host is alive and actively blocking inbound TCP. This is a deliberate firewall policy, not censorship. Nimbus provides DiscV5 coverage plus the interesting data point that TCP connects on a port that turns out to be an HTTP metrics endpoint.

ENRs are self-authenticating (signed by the node's secp256k1 private key). IPs for `beacon_tcp_connect` and `libp2p_handshake` are extracted at probe build time via `Enr::ip4()` / `Enr::tcp4()` / `Enr::udp4()`. Nodes without a `tcp` ENR key fall back to the `udp4` port for TCP probes.

### 11.4b Consensus Boot Node Candidates Evaluated and Rejected

All 13 candidates were tested on 2026-05-14 from Spain (AS3352, Telefónica). ENR fields were decoded with `cargo test -p prober_core --features discv5 -- enr_decode --nocapture` before probing. The probe results drove the removal decisions.

#### Architectural constraint that affects all removals

The probe builder always generates **3 probes per `discv5_consensus` entry**: DiscV5 ping + Beacon TCP connect + libp2p handshake. There is no "DiscV5-only" mode for a given target. When a node has no `tcp` ENR key, the builder falls back to the `udp4` port for TCP probes. If that port is blocked for TCP (as is normal for UDP-only discovery nodes), every run produces 2 permanently-failing probes per node and wastes 3 × 5 s × 3 attempts = 45 s of timeout per pair. This constraint makes it impractical to keep nodes that are useful only for DiscV5 without also paying for 2 always-failing TCP/libp2p probes. A future enhancement is a `discv5_only: true` config flag that skips TCP/libp2p probe generation per entry.

---

#### Lighthouse Sydney + London (Sigma Prime)

**ENR decode:**
```
lighthouse-sydney:  ip4=172.105.173.25  ip6=2400:8907::f03c:92ff:fe6b:a13  tcp4=None  tcp6=None  udp4=9000  udp6=9090
lighthouse-london:  ip4=139.162.196.49  ip6=2a01:7e00::f03c:92ff:fe6b:1eb9  tcp4=None  tcp6=None  udp4=9000  udp6=9090
```

**Probe results:**
- DiscV5 ping: OK (875 ms Sydney, 699 ms London — high but within timeout)
- Beacon TCP (172.105.173.25:9000 and 139.162.196.49:9000): **timeout** — TCP SYN silently dropped
- libp2p handshake: **timeout** — same

**Technical analysis:** Lighthouse explicitly omits the `tcp` ENR key. Per EIP-778, the `tcp` key signals willingness to accept inbound TCP connections. Its absence means the node does not expose a libp2p listener. Our code falls back to `udp4=9000` for TCP probing, which confirms TCP:9000 is silently dropped (DROP firewall rule, not REJECT). DiscV5 UDP:9000 and UDP:9090 are open. These are pure DiscV5 discovery nodes. DiscV5 coverage for Australia and UK is already provided by Teku and Nimbus at lower latency. **Removed:** no `tcp` key → libp2p structurally impossible; TCP fallback always timeouts → 2 × 15 s of wasted run time per probe cycle with zero informational value beyond "UDP-only node blocks TCP."

---

#### Nimbus Frankfurt 2 (Status)

**ENR decode:**
```
nimbus-frankfurt-2:  ip4=3.64.117.223  tcp4=9100  udp4=9100
```

**Probe results:**
- DiscV5 ping: OK (73 ms)
- Beacon TCP (3.64.117.223:9100): OK — TCP connects
- libp2p handshake: **timeout**

**Technical analysis:** Identical port and behaviour pattern to nimbus-frankfurt (3.120.104.18). Status operates all their boot nodes with `tcp4=9100` and `udp4=9100`. Port 9100 is the Prometheus metrics port in Nimbus's default configuration. TCP connects because the HTTP server on 9100 accepts any TCP connection, then libp2p multistream-select bytes are sent into an HTTP server that simply ignores them, causing a timeout. This is not a libp2p node. The node sits in the same AWS eu-central-1 (Frankfurt) region as nimbus-frankfurt, providing no geographic diversity. **Removed:** exact duplicate of nimbus-frankfurt in behavior, operator, and geography.

---

#### Prysm Azure 1 + 2 (Prysmatic Labs)

**ENR decode:**
```
prysm-azure-1:  ip4=4.157.240.54   tcp4=9000  udp4=9000
prysm-azure-2:  ip4=4.196.214.4    tcp4=9000  udp4=9000
```

These were the **only candidates** in the entire evaluated set with `tcp4=9000` explicitly advertised — the prerequisite for a functional libp2p probe. Both IPs are in Microsoft Azure address ranges.

**Probe results:**
- DiscV5 ping: **timeout** (1013 ms, 1019 ms) — the entire 1 s DiscV5 timeout exhausted on all 3 attempts
- Beacon TCP (4.157.240.54:9000 and 4.196.214.4:9000): **timeout**
- libp2p handshake: **timeout**

**Technical analysis:** The critical failure here is that DiscV5 UDP ping also times out, not just TCP. This rules out a "TCP-only firewall" scenario. The node's UDP:9000 port is completely unreachable — either the node is offline, decommissioned, behind a network that blocks all inbound traffic from Spanish IP ranges (AS3352), or the ENR entries in Prysm's `mainnet_config.go` point to IPs that are no longer operated at those addresses. The ENR is self-signed by the node's private key but contains no timestamp; a stale ENR can remain in a client's config indefinitely after a node is retired. Since DiscV5 ping (the lowest-level reachability probe) fails, running TCP or libp2p probes against these IPs is meaningless. **Removed:** completely unreachable — all 3 probe types timeout. No data of any kind is obtained from either node.

---

#### EF Consensus US-1, US-2, JP-1, JP-2 (Ethereum Foundation)

**ENR decode:**
```
ef-consensus-us-1:  ip4=3.17.30.69       tcp4=None  udp4=9000   (AWS us-east-2, Ohio)
ef-consensus-us-2:  ip4=18.216.248.220   tcp4=None  udp4=9000   (AWS us-east-2, Ohio)
ef-consensus-jp-1:  ip4=54.178.44.198    tcp4=None  udp4=9000   (AWS ap-northeast-1, Tokyo)
ef-consensus-jp-2:  ip4=54.65.172.253    tcp4=None  udp4=9000   (AWS ap-northeast-1, Tokyo)
```

Source: `prysmaticlabs/prysm` `config/params/mainnet_config.go`, labeled as EF-operated boot nodes.

**Probe results:**
- DiscV5 ping: OK (256–478 ms — consistent with trans-Atlantic + trans-Pacific latency)
- Beacon TCP on `udp4` fallback port: **timeout** on all 4 nodes
- libp2p handshake: **timeout** on all 4 nodes

**Technical analysis:** These are EF-operated, production consensus boot nodes — likely running Prysm or a multi-client setup. They have no `tcp` ENR key, confirming they are configured as UDP-only DiscV5 discovery nodes. TCP:9000 SYN packets are silently dropped (DROP firewall rule, not REJECT — no RST is sent). This is the standard configuration for dedicated discovery infrastructure that intentionally does not participate in libp2p peering. The DiscV5 results are geographically interesting (US East and Japan), but keeping them creates 8 permanently-failing probes (2 per node × 4 nodes) and extends run time by approximately 15 s per cycle due to the TCP fallback timeouts running at full 5 s × 3 attempts. **Removed:** same architectural constraint as Lighthouse — no `tcp` key means the TCP/libp2p probes are structurally guaranteed to fail, and the cost in run time and result noise outweighs the DiscV5 geographic value.

---

#### Pryslab Ohio (Prysmatic Labs)

**ENR decode:**
```
pryslab-ohio:  ip4=18.223.219.100  tcp4=None  udp4=9000   (AWS us-east-2, Ohio)
```

**Probe results:**
- DiscV5 ping: OK (253 ms)
- Beacon TCP (18.223.219.100:9000 via udp4 fallback): **timeout**
- libp2p handshake: **timeout**

**Technical analysis:** The IP 18.223.219.100 hosts multiple separate DiscV5 instances for Pryslab: three ENRs in Prysm's config point to the same IP address but different UDP ports (9000, 10000, 11000), indicating a single machine running several DiscV5 daemon instances. No `tcp` key in any of them. TCP:9000 is silently dropped. The US East geographic region is already covered by the EF consensus-us nodes (when they were active), and DiscV5 pings at similar latency (~250 ms) from that region are not uniquely informative compared to ef-consensus-us-1/2 which were also removed. **Removed:** same structural reason as all UDP-only nodes; additionally redundant in geography with the EF US East nodes.

---

#### Conclusion: libp2p probe coverage gap

After exhaustive evaluation of all available public boot node ENRs from every major consensus client team (Teku, Lighthouse, Nimbus, Prysm/Pryslab, Ethereum Foundation), **zero nodes accept inbound libp2p connections**. The fundamental reason is architectural: boot nodes serve only peer discovery via DiscV5/DiscV4 UDP and explicitly do not expose inbound libp2p TCP. Nodes that do accept libp2p are full peer nodes (validators, beacon sync nodes) that join the network dynamically and do not publish stable, long-lived ENRs in any official repository. Obtaining a working libp2p target requires either: (a) finding a stable public consensus node operated by a community member with a fixed IP and known ENR, or (b) running a local beacon node and probing localhost — neither of which applies to the current remote-probing use case.

### 11.5 Beacon REST API Domains (beacon_https)

Domain names resolved at connection time. Source: provider documentation (PublicNode, ChainSafe). Confirmed to serve `GET /eth/v1/node/version` without authentication.

---

## 11bis. Known Boot Node Behaviours (Non-Bugs)

This section records observed probe results that look like failures but are expected given the boot nodes' configuration. This prevents these from being misinterpreted as regressions or censorship in future runs.

### LlamaRPC HTTP 503 with DNS/TCP/TLS all OK

LlamaRPC uses a CDN (reverse proxy / load balancer) in front of their backend application servers. When the backend is down but the CDN is still running, the probe layers diverge predictably:

- **DNS** → OK: the CDN infrastructure's DNS is up.
- **TCP** → OK: the CDN accepts TCP connections on port 443.
- **TLS** → OK: the CDN terminates TLS.
- **HTTPS RPC / HTTPS Write** → FAIL HTTP 503: the CDN receives the HTTP request, attempts to forward it to the backend, finds it unreachable, and returns 503 Service Unavailable.

This is a real service outage, not a censorship signal. It demonstrates precisely why layered probes are valuable: without the HTTPS probe, LlamaRPC would appear fully healthy (all green) despite being non-functional for wallets and dApps. The CDN layer hides the backend failure from TCP-level tests.

**Expected probe result**: DNS, TCP, TLS → ok; HTTPS RPC, HTTPS Write → fail (HTTP 503). This is a backend outage, not a censorship event.

### Active Consensus Boot Nodes — Expected libp2p Failures

All 3 remaining `discv5_consensus` nodes produce failing `libp2p_handshake` probes. This is expected and not a censorship signal:

- **teku-aws-ohio / teku-aws-sydney**: DiscV5 ping → OK. Beacon TCP → **connection refused** (RST, errno 10061). libp2p → **connection refused**. Teku boot nodes explicitly firewall inbound TCP:9000. The RST response proves the host is alive and the port is actively blocked, which is distinguishable from censorship (which would be a DROP/timeout). This is a deliberate operator choice, not a network issue.

- **nimbus-frankfurt**: DiscV5 ping → OK. Beacon TCP → **OK** (TCP:9100 connects). libp2p → **timeout**. The Nimbus Frankfurt ENR contains `tcp4=9100`, which is the Prometheus metrics HTTP endpoint, not the libp2p port. The metrics server accepts TCP connections but ignores multistream-select bytes, so the libp2p probe waits until timeout. This is a node configuration choice by the Nimbus team — advertising the metrics port in the ENR — not a censorship signal.

For the full technical investigation of all 13 evaluated candidates (10 of which were removed), see §11.4b. The conclusion is that no public boot node accepts inbound libp2p connections; libp2p testing requires stable full peer node targets, which do not currently exist in this config.

### EF Execution Boot Nodes — RLPx Auth and the DPI Detection Semantics

`rlpx_handshake` probes send an EIP-8 ECIES auth packet to boot nodes. The probe's purpose is to detect DPI filtering — a firewall that specifically drops RLPx auth packets on otherwise-open TCP:30303 connections. The original success condition (`read_exact` for ≥1 byte) was wrong: it failed on FIN and RST, which are how boot nodes legitimately reject unknown identities.

**Corrected success semantics** (implemented):
- **TCP connect fails** → fail (TCP-level block).
- **Auth sent, remote responds with data** → ok (full auth-ack or disconnect message; RLPx is reachable).
- **Auth sent, remote sends FIN (early eof)** → ok (boot node received auth, rejected unknown identity gracefully; no DPI).
- **Auth sent, remote sends RST (os error 10054/WSAECONNRESET)** → ok (boot node received auth, abruptly closed; no DPI).
- **Auth sent, timeout** → fail (auth packet may have been dropped by DPI before reaching the remote).

Boot nodes always reject our ephemeral identity (the prober generates a fresh key per attempt and has no persistent node ID that would be in any routing table). FIN/RST after auth is the normal rejection response. The distinction between FIN and RST is a node implementation detail, not a censorship signal.

**Expected probe result after fix**: `rlpx_handshake` → ok for EF-ap-southeast and EF-us-east (they respond to auth). The former EF-southeast-asia (52.187.207.27) timed out because the node was decommissioned; updated to EF-hetzner-hel (65.108.70.101).

**What timeout actually means**: If TCP:30303 connects but the auth packet triggers a timeout (no response at all), a DPI firewall is specifically dropping RLPx auth packets. Combined with `p2p_tcp_connect` succeeding, this would be strong evidence of protocol-specific deep packet inspection filtering RLPx traffic.


---

## 12. Future Work

### 12.1 Reverse Connectivity — Inbound Probing from the Server

**Motivation**

All current probes are *outbound*: the client measures whether it can reach Ethereum infrastructure. This covers the wallet and dapp use case completely (wallets only initiate outbound connections). It also covers the first half of the node operator use case: "can I discover peers and initiate connections?" But it leaves untested the second, equally important half: "can peers discover and connect to me?"

This distinction matters because common network conditions block inbound connections while leaving outbound entirely free:

- **CGNAT (Carrier-Grade NAT)**: Many residential ISPs (including parts of Telefónica's network) place customers behind a shared NAT. The client has no public IP; all inbound TCP/UDP is impossible without an explicit port mapping. A user under CGNAT can access all RPC providers and reach all boot nodes, but cannot run a publicly reachable execution or consensus node — peers cannot connect back to them.
- **Residential ISP inbound port blocking**: Even with a public IP, many ISPs block inbound traffic on port ranges outside 80/443. Port 30303 and 9000 are commonly filtered.
- **Home router / consumer firewall**: The router may not have UPnP configured, so port 30303/9000 is not forwarded. The user may not be aware.
- **VPN or tunnel**: A user behind a VPN can reach outbound targets normally but inbound connections to the VPN's tunnel interface may not be routed correctly.

None of these conditions appear in the current probe results. A user with all green outbound probes but zero inbound reachability cannot participate as an Ethereum node — they are wallet-only by force.

**Design**

The existing server infrastructure (DigitalOcean, public IP) is in an ideal position to perform reverse probes: it knows the client's external IP (from the HTTP connection that delivered the report), and it has an unrestricted outbound connection from a data centre with a clean IP reputation.

The proposed flow:

```
Client                               Server (DigitalOcean)
  |                                        |
  |--- POST /report (outbound probes) ---->|
  |                                        |
  |    Server extracts client_ip           |
  |    from the HTTP connection            |
  |                                        |
  |    Server runs inbound probes:         |
  |      TCP connect to client_ip:30303    |
  |      UDP DiscV4 ping to client_ip:30303|
  |      TCP connect to client_ip:9000     |
  |      UDP DiscV5 ping to client_ip:9000 |
  |                                        |
  |<-- 200 OK { inbound_results: [...] } --|
  |                                        |
  Client displays inbound section          |
```

The inbound probes are performed server-side synchronously before the HTTP response is returned (or asynchronously with a follow-up poll if latency becomes a concern). The results are returned to the client in the same response body or via a second `GET /inbound/{run_id}` endpoint.

**What each inbound probe tests**

| Probe | What it detects |
|---|---|
| TCP connect to client:30303 | Client's execution P2P port is publicly reachable; not behind CGNAT, not blocked by ISP, router port-forwarded correctly |
| DiscV4 ping to client:30303 | Execution peer discovery is possible; the Ethereum execution network could find and connect to this node |
| TCP connect to client:9000 | Client's consensus P2P port is publicly reachable; prerequisite for running a validator or beacon node |
| DiscV5 ping to client:9000 | Consensus peer discovery is possible; the Ethereum consensus network could find this beacon node |

**Roles a participant can play, and the probes that qualify them**

| Role | Outbound probes required | Inbound probes required |
|---|---|---|
| Wallet / dapp user | Section 1 (all OK) | None |
| Execution node (no inbound) | TCP:30303, DiscV4, RLPx | None (degrades to outbound-only peer, limited peers) |
| Execution node (full peer) | Same | TCP:30303 reachable, DiscV4 reachable |
| Consensus node (no inbound) | TCP:9000, DiscV5, Beacon API | None (outbound-only peer) |
| Consensus node / validator | Same | TCP:9000 reachable, DiscV5 reachable |
| Archive node | Same as execution node | Same as execution node |

This gives rise to a natural "participation tier" output the UI could display:

```
Wallet connectivity:          ✓ Full access
Execution node outbound:      ✓ Can discover and connect to peers
Execution node inbound:       ? Not yet tested (requires server probe)
Consensus node outbound:      ✓ DiscV5 reachable
Consensus node inbound:       ? Not yet tested
```

**Implementation notes**

On the server side, the inbound probes are straightforward to implement:
- TCP connect is a standard `TcpStream::connect(client_ip:port)` with a short timeout (1–2 s is enough; there is no authentication to perform)
- DiscV4 ping requires the same secp256k1 signing already implemented in `discv4_ping.rs` — the server would reuse prober_core as a library
- DiscV5 ping requires the discv5 crate — also already a dependency in prober_core

One subtlety: the server cannot know whether a failed inbound TCP connect means "no listener" or "ISP blocks inbound to this IP". From the server's perspective both look the same (timeout or refused). Additional signals (e.g., whether the TCP RST arrives quickly vs. timeout) help distinguish "port actively closed by kernel" (fast RST, router is accessible) from "packet never arrives" (timeout, CGNAT or ISP block).

**Privacy consideration**: The server already knows the client's IP (it received the HTTP connection). Performing reverse probes to that IP does not reveal any additional information — it only tests connectivity properties that are already public (whether a port is open or not). No PII is involved. The probe results are associated with the `client_id` UUID that is already stored per report.

### 12.2 Protocol Depth Extensions

- **ETH protocol Status exchange**: Complete the execution P2P handshake beyond RLPx auth — after auth succeeds, send an ETH `Status` message and validate the response. This proves the node is running the correct Ethereum mainnet and is synced (or close to synced). Currently, RLPx success only proves the transport layer is reachable, not that the node is a functioning mainnet peer.

- **Full Noise XX handshake (libp2p)**: Complete the consensus P2P handshake beyond multistream-select — perform Noise XX key exchange and stream setup. This proves the peer supports the full libp2p security protocol, not just the protocol negotiation header.

- **GossipSub subscription**: After a full libp2p session, subscribe to the `beacon_block` topic and verify that block announcements are received. This proves the node participates in actual consensus gossip.

### 12.3 Additional Probe Types

- **Portal Network probes**: uTP-based probes to test light client reachability as the Portal Network matures.
- **ECH (Encrypted Client Hello) detection**: Detect whether a TLS connection uses ECH (encrypting the SNI) vs. plaintext SNI. A MITM proxy that intercepts TLS without ECH would be detectable; with ECH it cannot inspect the SNI.
- **Additional JSON-RPC methods**: Probe `eth_blockNumber` (is the node synced and at chain tip?) and `eth_getLogs` (are archive queries supported?) in addition to `eth_chainId`.
- **IPv6 end-to-end**: IPv6 connectivity is independently blockable. Many ISPs support IPv6 but block specific IPv6 ranges or protocols. Running parallel IPv4/IPv6 probes would detect IPv6-specific censorship.

### 12.4 Multi-Region Correlation

The most powerful use of aggregated reports is detecting censorship that is geographically constrained — present in some countries/ISPs but not others. This requires:
- Server-side GeoIP enrichment of client IPs (already planned)
- Per-provider, per-probe aggregation by AS number and country
- Statistical anomaly detection: if provider X fails for 80% of AS3352 (Telefónica Spain) but succeeds for 99% of all other ASes, that is a strong censorship signal even if individual runs appear ambiguous

---

## 13. References

- **EIP-8**: RLPx Version 8 with forward compatibility. https://eips.ethereum.org/EIPS/eip-8
- **EIP-778**: Ethereum Node Records. https://eips.ethereum.org/EIPS/eip-778
- **devp2p RLPx spec**: https://github.com/ethereum/devp2p/blob/master/rlpx.md
- **devp2p DiscV4 spec**: https://github.com/ethereum/devp2p/blob/master/discv4.md
- **DiscV5 spec**: https://github.com/ethereum/devp2p/blob/master/discv5/discv5.md
- **Ethereum Beacon API**: https://ethereum.github.io/beacon-APIs/
- **libp2p specs**: https://github.com/libp2p/specs
- **Noise Protocol Framework**: https://noiseprotocol.org/noise.html
- **go-ethereum boot nodes**: https://github.com/ethereum/go-ethereum/blob/master/params/bootnodes.go
- **Lighthouse boot nodes**: https://github.com/sigp/lighthouse/blob/stable/boot_node/src/config.rs
- **Teku boot nodes**: Consensys/teku repository, `eth-reference-tests/src/referenceTestResources`
- **OFAC Tornado Cash sanctions**: U.S. Treasury press release, August 8, 2022
- **Portal Network specification**: https://github.com/ethereum/portal-network-specs
- **Cloudflare DoH API**: https://developers.cloudflare.com/1.1.1.1/encryption/dns-over-https/
- **RFC 8446**: The TLS 1.3 Protocol. https://tools.ietf.org/html/rfc8446
- **RustCrypto AES/CTR**: https://github.com/RustCrypto/block-ciphers
- **k256 crate (secp256k1)**: https://docs.rs/k256
- **discv5 Rust crate (Sigma Prime)**: https://docs.rs/discv5
