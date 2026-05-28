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
   - 8.0 [Taxonomy of Ethereum Participants](#80-taxonomy-of-ethereum-participants)
   - 8.1 [Wallet and dApp User](#81-wallet-and-dapp-user)
   - 8.2 [Light Client](#82-light-client)
   - 8.3 [Execution Full Node and Archive Node](#83-execution-full-node-and-archive-node)
   - 8.4 [Consensus Full Node](#84-consensus-full-node)
   - 8.5 [Validator (Staker)](#85-validator-staker)
   - 8.6 [MEV Ecosystem (Searchers, Builders)](#86-mev-ecosystem-searchers-builders)
   - 8.7 [Coverage Matrix](#87-coverage-matrix)
   - 8.8 [Censorship Interpretation Key](#88-censorship-interpretation-key)
   - 8.9 [Target Reference: What Is Probed and Why](#89-target-reference-what-is-probed-and-why)
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

**Why ENR matters for the probe**: The `discv5_consensus` probe targets are specified as ENR strings. The ENR contains the node's IP address (`ip` field) and both TCP and UDP ports (`tcp`, `udp`). When the `discv5` feature is enabled, the project decodes these ENRs for the `beacon_discv5_ping` probe. ENRs are no longer used to derive `beacon_tcp_connect` or `libp2p_handshake` probe targets — those are now generated dynamically at runtime from live Beacon API peers (see §12.0).

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

Docker Compose stack: Postgres 16-alpine + FastAPI/uvicorn HTTP server.

**Endpoints:**
- `POST /report`: Accepts JSON reports from CLI and Tauri clients. Validates, enriches with GeoIP (MaxMind GeoLite2), and persists.
- `GET /ping`: Health check → `ok`.
- `GET /api/geo-reports`: Returns a GeoJSON `FeatureCollection` of the last 12 months of reports with coordinates. Intended for future MapLibre map display in the app (not yet wired to the app).

**Database (`dbalfa`):**
- Postgres 16-alpine, exposed on port **15432** (host) → 5432 (container). Tuned for 1 GB droplet: `shared_buffers=128MB`, `work_mem=4MB`, `max_connections=20`.
- Three roles: `ethprobes` (full access to `ethconnprobes` schema — used by the app), `admin` (Docker `POSTGRES_USER`, superuser), `dbuser` (read-only — for psql/pgAdmin inspection).
- Passwords are never in environment variables. `admin` password is set via Docker secret (`pgfile`) mounted as `POSTGRES_PASSWORD_FILE`. All role passwords are set in `server/secrets/postgres/auth.sql` via `ALTER USER ... WITH PASSWORD`, applied by `init/dbinit.sh` after schema creation.
- Schema (`base.sql`) is initialised first, then passwords applied (`auth.sql`). Users are created without passwords in `base.sql`; `auth.sql` only sets them.
- App connects via **PGSERVICEFILE**: `server/secrets/postgres/pgs` is mounted as a Docker secret and its path exported as `PGSERVICEFILE` in the app container. The `[main]` service in `pgs` points to the internal Compose hostname `postgres:5432` with `user=ethprobes` and `options=-csearch_path=ethconnprobes`.
- For local access (psql/pgAdmin), copy `server/pg_service.conf` to `%APPDATA%\postgresql\pg_service.conf` (Windows) and set `PGSERVICEFILE` to that path permanently (System Properties → Environment Variables). Services `main`, `admin`, and `dbuser` point to the server at port 15432.

**GeoIP:** Two MaxMind GeoLite2 `.mmdb` files (`GeoLite2-City.mmdb`, `GeoLite2-ASN.mmdb`) must be placed in `server/geoip/` and are mounted read-only at `/geoip` inside the container. If the files are absent, GeoIP fields are stored as `NULL` (non-fatal).

### 6.6 Data Model

**Client-side (Rust structs → JSON payload):**
```
Report
├── run_id: UUID
├── timestamp: ISO 8601
├── started_at_ms / finished_at_ms: u128
├── client: ClientInfo { os, arch, client_id, app_channel }
├── run: RunConfig { attempts, min_successes, timeout_ms, parallelism }
└── results: Vec<ProbeRun>
    └── ProbeRun
        ├── kind: ProbeKind (snake_case string)
        ├── target: String (human-readable label)
        ├── attempts: Vec<AttemptResult>
        │   └── AttemptResult { ok, rtt_ms, error, meta: JSON }
        └── summary: ProbeSummary { success_count, failure_count, min/avg/max_rtt_ms, ok }
```

`ok` in `ProbeSummary` is `true` iff `success_count >= min_successes` (default: 1 out of 3 attempts).

**Server-side (PostgreSQL schema `ethconnprobes`):**

All tables live in the `ethconnprobes` schema. The Python app sets `search_path=ethconnprobes` on the connection pool, so queries use unqualified names.

```
ethconnprobes.reports          — one row per submitted run (unique on run_id)
ethconnprobes.probe_runs       — one row per (kind × target) within a report
ethconnprobes.probe_attempts   — one row per individual attempt; meta stored as JSONB
```

The `kind` column is plain `TEXT`, so new `ProbeKind` variants require no schema migration — they are stored as their snake_case string representation.

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

**Target type**: Generated dynamically at runtime from live Beacon API peers (inbound-only). Not derived from static ENR config entries.
**Transport**: TCP to the peer's advertised port (typically 9000, but any port the peer listens on).
**What it tests**: TCP reachability of a real consensus peer node that is actively connected to a known beacon node.
**Design decision**: Boot nodes (the static ENR entries in `discv5_consensus`) deliberately block inbound TCP:9000. Probing them produces no useful censorship signal — only a known firewall response. Real full peer nodes, fetched live from the Beacon API's `/eth/v1/node/peers` endpoint, accept inbound libp2p connections and provide meaningful TCP reachability data. See §9.2 for the full design rationale.

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

**Target type**: Generated dynamically at runtime from live Beacon API peers (inbound-only). Not derived from static ENR config entries.
**Transport**: TCP connect then multistream-select negotiation.
**What it tests**: Whether the remote is a live libp2p node responding to the multistream negotiation protocol. Specifically: sends `/multistream/1.0.0\n` and `/noise\n` proposals, checks for acknowledgement.
**Success**: Remote acknowledges `/multistream/1.0.0` (confirming it is a live libp2p node).
**Meta**: Whether `/noise` was also accepted.
**Design decision**: Paired with `beacon_tcp_connect` — both target the same live peers from the Beacon API. One peer generates two probes: TCP connect and libp2p multistream, analogous to how one `[[probes.tcp]]` entry generates DNS + TCP + TLS probes. The libp2p probe itself has no feature dependencies; only the peer fetch depends on `beacon_https` targets being configured. See §9.2 for the full rationale for moving from static ENR-derived targets to dynamic live peers.

---

## 8. Connectivity by Participant Type

This section is the core of the design document. It maps every Ethereum participant role to the probes that cover their connectivity requirements, explains what each probe result means for that role, and identifies what is not yet covered. The goal is to answer the question: **given a set of probe results, what can a user in this location do on Ethereum?**

### 8.0 Taxonomy of Ethereum Participants

The Ethereum network is composed of participants with different roles and connectivity requirements:

| Participant | P2P participation | Uses JSON-RPC | Uses Beacon API |
|---|---|---|---|
| Wallet user | No | Yes (reads + sends txs) | No |
| dApp (frontend) | No | Yes (reads + sends txs) | Sometimes |
| Light client | No (Portal optional) | Via provider | Yes (consensus data) |
| Execution full node | Yes (devp2p) | Self-hosted | No |
| Execution archive node | Yes (devp2p) | Self-hosted | No |
| Consensus full node | Yes (libp2p) | Via paired exec node | Self-hosted |
| Validator (staker) | Yes (libp2p) | Via paired exec node | Self-hosted |
| MEV searcher | No | Yes (mempool access, tx submission) | No |
| MEV block builder | No | Yes (tx bundles, relay HTTPS) | No |

---

### 8.1 Wallet and dApp User

**What they need**: A wallet or dApp accesses Ethereum exclusively through JSON-RPC over HTTPS/WSS. It never participates in P2P networks. All communication goes through public RPC providers (Infura, Cloudflare, publicnode, etc.) via standard HTTPS port 443.

**Connectivity stack** (in order, each layer is a prerequisite for the next):

| Layer | Need | Probe | Section |
|---|---|---|---|
| 1 | DNS resolution of provider domain | `dns_resolve` | 1 |
| 2 | DNS integrity (not manipulated by ISP) | `dns_compare` | 2 |
| 3 | TCP:443 reachable | `tcp_connect` | 1 |
| 4 | TLS handshake (no certificate injection) | `tls_handshake` | 1 |
| 5 | JSON-RPC read access | `https_jsonrpc` | 1 |
| 6 | Transaction submission (write access) | `https_jsonrpc_write` | 1 |
| 7 | Real-time event subscriptions | `wss_jsonrpc` + `wss_subscribe` | 1 |

**Coverage**: Complete. Every censorship vector between the user's machine and the RPC provider is covered. The `https_jsonrpc_write` probe specifically catches OFAC-style write censorship — the most commercially relevant form, where providers accept reads but reject transaction forwarding based on address blacklists.

**How to read your results for this role**:
- All Section 1 probes OK → Full wallet/dApp access, no censorship detected at any layer.
- Any `dns_resolve` fails → RPC provider domain is being blocked at DNS. Wallet cannot connect.
- `dns_compare` shows mismatch → ISP DNS is returning different IP addresses. Possible DNS hijacking.
- `tcp_connect` fails on 443 while other ports work → IP-level blocking of the CDN serving this provider.
- `tls_handshake` fails with certificate error → TLS interception (MITM proxy). The ISP or corporate network is terminating SSL.
- `https_jsonrpc` fails (non-503) while TCP/TLS succeed → Application-layer block (geographic restriction, rate limit, service blocked).
- `https_jsonrpc` OK but `https_jsonrpc_write` fails → Write-path censorship. The provider accepts queries but rejects transaction forwarding. Classic OFAC compliance (as Infura did for sanctioned addresses in 2022).
- Both HTTPS OK, `wss_subscribe` fails → WebSocket specifically blocked. Corporate proxies often proxy HTTP but not WebSocket upgrades.

**Gaps**: None. This participant type is fully covered.

---

### 8.2 Light Client

**What they need**: A light client (Helios, EIP-4444 clients) does not sync the full chain. It fetches consensus state (sync committee updates, finalized block headers) from a beacon node via the Beacon REST API, and serves execution queries via JSON-RPC using Merkle proofs against those headers.

**Connectivity stack**:

| Layer | Need | Probe | Section |
|---|---|---|---|
| 1–6 | All wallet layers (needs JSON-RPC for execution queries) | Same as §8.1 | 1 |
| 7 | Beacon REST API (sync committee, finalized header) | `beacon_https` | 3 |
| 8 | Portal Network (stateless block/receipt data) | ❌ Not implemented | — |

**Coverage**: Layers 1–7 are covered (Section 1 + Beacon API probe). The Portal Network layer is not implemented.

**How to read your results for this role**:
- Section 1 OK + `beacon_https` OK → Light client has access to both consensus state and execution data. Full functionality.
- `beacon_https` fails → Cannot sync consensus headers. Light client cannot verify block validity.
- Section 1 fails → Cannot serve execution queries to the user.

**Gaps**: Portal Network (uTP-based) would complete coverage.

---

### 8.3 Execution Full Node and Archive Node

**What they need**: An execution node (Geth, Erigon, Nethermind, Besu) participates in the devp2p P2P network to sync blocks and propagate transactions. An archive node is identical from a networking perspective — the difference is in storage, not P2P protocol.

**Connectivity stack**:

| Layer | Need | Probe | Section |
|---|---|---|---|
| 1 | Peer discovery via DiscV4 (UDP:30303) | `discv4_ping` | 2 |
| 2 | P2P transport (TCP:30303) | `p2p_tcp_connect` | 2 |
| 3 | RLPx ECIES auth (no DPI filtering) | `rlpx_handshake` | 2 |
| 4 | ETH protocol Status exchange | ❌ Not implemented | — |
| 5 | DNS non-manipulation (for RPC provider domains) | `dns_compare` | 2 |

**Coverage**: Layers 1–3 and 5 are covered. Layer 4 (the ETH `Status` message exchange) is the only gap — it would confirm the node can participate in block propagation at the application protocol level, not just establish the transport connection.

**How to read your results for this role**:

- All Section 2 probes OK → Execution P2P fully reachable. You can run a full node.
- `discv4_ping` fails while Section 1 (TCP:443) succeeds → UDP:30303 is specifically blocked. Peer discovery is impossible; node would only connect to hardcoded boot nodes.
- `p2p_tcp_connect` fails while `tcp_connect` (port 443) succeeds → TCP:30303 is selectively blocked. Strong censorship signal — this is port-level filtering, not a service being down.
- `p2p_tcp_connect` OK but `rlpx_handshake` times out → **DPI filtering of RLPx auth packets**. This is the most targeted form of execution P2P censorship: the firewall allows TCP connections on port 30303 but inspects the payload and drops packets matching the RLPx ECIES pattern. A connection reset (RST) means the port is open but the remote rejected your ephemeral identity (expected — boot nodes always do this); only a timeout indicates DPI.
- `dns_compare` shows mismatch → DNS manipulation affecting provider domain resolution. Doesn't affect P2P but affects RPC access.

**Severity ordering** (most to least targeted censorship):
1. RLPx DPI filtering — allows TCP:30303 but drops protocol-specific bytes
2. TCP:30303 blocked while UDP:30303 open — transport-level block
3. UDP:30303 blocked while TCP:30303 open — discovery-only block
4. Both TCP and UDP:30303 blocked — complete execution P2P isolation
5. DNS manipulation — affects RPC provider access, not P2P

**Gaps**: ETH `Status` exchange (would confirm block sync is possible, not just transport connectivity).

---

### 8.4 Consensus Full Node

**What they need**: A consensus node (Lighthouse, Prysm, Teku, Nimbus, Lodestar) participates in the libp2p network to receive blocks and attestations via GossipSub, and uses DiscV5 for peer discovery.

**Connectivity stack**:

| Layer | Need | Probe | Section |
|---|---|---|---|
| 1 | Peer discovery via DiscV5 (UDP:9000) | `beacon_discv5_ping` | 3 |
| 2 | P2P transport (TCP:9000) | `beacon_tcp_connect` | 3 |
| 3 | libp2p multistream-select (protocol negotiation) | `libp2p_handshake` | 3 |
| 4 | Noise XX handshake (encryption layer) | ❌ Not implemented | — |
| 5 | GossipSub subscription (block/attestation gossip) | ❌ Not implemented | — |
| 6 | Beacon REST API (checkpoint sync) | `beacon_https` | 3 |

**Coverage**: Layers 1–3 and 6 are covered. Layers 4 and 5 are gaps — the libp2p probe confirms that a peer speaks the multistream protocol, but does not complete the Noise XX handshake (the encryption layer required before any gossip messages are exchanged).

**Practical significance of coverage**: The libp2p multistream handshake succeeding means the remote node is a live libp2p node that accepted our protocol negotiation. A DPI firewall that targets libp2p would drop our bytes before this exchange completes. In practice, coverage of layers 1–3 is sufficient to detect ISP-level and network-level censorship; layers 4–5 would only add detection of protocol-specific filtering that targets Noise specifically — an extremely advanced and rare attack.

**How to read your results for this role**:
- All Section 3 probes OK → Consensus P2P reachable. You can run a consensus full node.
- `beacon_discv5_ping` fails → UDP:9000 blocked. Peer discovery is impossible.
- `beacon_discv5_ping` OK, `beacon_tcp_connect` fails → TCP:9000 blocked while UDP:9000 is open. Discovery works but cannot establish libp2p connections. Cannot participate in block propagation.
- `beacon_tcp_connect` OK, `libp2p_handshake` fails → **DPI filtering of libp2p protocol bytes**. Most targeted form: allows TCP:9000 connections but inspects the payload and drops multistream negotiation.
- `beacon_https` fails → Beacon REST API inaccessible. Checkpoint sync is blocked.

**Severity ordering**:
1. libp2p DPI filtering — allows TCP:9000 but drops multistream bytes
2. TCP:9000 blocked while UDP:9000 open
3. Both TCP and UDP:9000 blocked
4. Beacon API blocked

---

### 8.5 Validator (Staker)

**What they need**: A validator runs a consensus full node and additionally must submit attestations and block proposals. The network stack is identical to the consensus full node.

| Layer | Need | Probe | Same as |
|---|---|---|---|
| 1–6 | All consensus full node layers | Same as §8.4 | §8.4 |
| 7 | Block proposal via GossipSub | ❌ Not implemented | — |
| 8 | Attestation submission via GossipSub | ❌ Not implemented | — |
| 9 | MEV-boost relay (HTTPS) | Partially: `https_jsonrpc` + `https_jsonrpc_write` to Flashbots | 1 |

**Coverage**: Same as consensus full node (§8.4). GossipSub participation (the actual attestation/block submission path) is not implemented. MEV-boost relay access is partially covered by the Flashbots probes in Section 1 — which test whether the relay's HTTPS endpoint accepts read and write requests — but does not test the builder-specific relay API.

**How to read your results for this role**: Same as §8.4. Add: Flashbots/MEV Blocker `https_jsonrpc_write` failing → transaction submission to private mempools is blocked, affecting MEV revenue.

---

### 8.6 MEV Ecosystem (Searchers, Builders)

**What they need**: MEV participants do not run P2P nodes. They interact with the Ethereum network through private RPC endpoints and relay APIs over HTTPS.

**Connectivity stack**:

| Layer | Need | Probe | Section |
|---|---|---|---|
| 1–6 | Standard wallet/dApp layers | Same as §8.1 | 1 |
| 7 | Flashbots RPC read access | `https_jsonrpc` (flashbots) | 1 |
| 8 | Flashbots bundle/tx submission | `https_jsonrpc_write` (flashbots) | 1 |
| 9 | MEV Blocker (private mempool) | `https_jsonrpc` + `https_jsonrpc_write` (mevblocker) | 1 |
| 10 | Builder-specific relay APIs | ❌ Not implemented (private/permissioned) | — |

**Coverage**: Standard HTTPS access to Flashbots and MEV Blocker is fully covered. The write probe specifically tests whether `eth_sendRawTransaction` is accepted — the most relevant path for transaction submission. Builder-specific relay APIs (e.g., Flashbots `eth_sendBundle`) are not covered because they use non-standard method names and often require authentication.

**How to read your results for this role**:
- `https_jsonrpc_write` fails for Flashbots while `https_jsonrpc` (read) succeeds → Write-path censorship at the RPC level. Cannot submit transactions through this provider.
- All Flashbots probes fail while other providers work → Flashbots-specific blocking (geographic restriction or OFAC policy applied to the relay endpoint).
- All providers' write probes fail → Write path blocked for all providers. Cannot submit any transactions.

---

### 8.7 Coverage Matrix

| Connectivity need | Probe(s) | Wallet | Light | Exec node | Cons node | Validator | MEV |
|---|---|---|---|---|---|---|---|
| DNS resolution | `dns_resolve` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| DNS integrity | `dns_compare` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| TCP:443 reachable | `tcp_connect` | ✓ | ✓ | — | — | — | ✓ |
| TLS non-interception | `tls_handshake` | ✓ | ✓ | — | — | — | ✓ |
| JSON-RPC read | `https_jsonrpc` | ✓ | ✓ | — | — | — | ✓ |
| JSON-RPC write | `https_jsonrpc_write` | ✓ | — | — | — | — | ✓ |
| WSS subscription | `wss_subscribe` | ✓ | — | — | — | — | ✓ |
| Beacon REST API | `beacon_https` | — | ✓ | — | ✓ | ✓ | — |
| DiscV4 discovery (UDP:30303) | `discv4_ping` | — | — | ✓ | — | — | — |
| TCP:30303 transport | `p2p_tcp_connect` | — | — | ✓ | — | — | — |
| RLPx DPI detection | `rlpx_handshake` | — | — | ✓ | — | — | — |
| ETH protocol Status | — | — | — | ❌ gap | — | — | — |
| DiscV5 discovery (UDP:9000) | `beacon_discv5_ping` | — | — | — | ✓ | ✓ | — |
| TCP:9000 transport | `beacon_tcp_connect` | — | — | — | ✓ | ✓ | — |
| libp2p multistream | `libp2p_handshake` | — | — | — | ✓ | ✓ | — |
| Noise XX handshake | — | — | — | — | ❌ gap | ❌ gap | — |
| GossipSub participation | — | — | — | — | ❌ gap | ❌ gap | — |
| Portal Network | — | — | ❌ gap | — | — | — | — |
| MEV relay APIs | — | — | — | — | — | ❌ gap | ❌ gap |

**Legend**: ✓ = covered, — = not applicable to this role, ❌ gap = relevant but not implemented.

---

### 8.8 Censorship Interpretation Key

This subsection translates specific probe failure patterns into actionable conclusions. For each pattern, the table shows what it means and which participants are affected.

#### Execution layer (Section 2)

| Pattern observed | What it means | Participants affected |
|---|---|---|
| `dns_compare` mismatch for any provider | ISP DNS is returning different IPs. Active DNS manipulation. | All (wallet, node bootstrap, light client) |
| `p2p_tcp_connect` FAIL, `tcp_connect`(443) OK | TCP:30303 selectively blocked. Port-level filtering targeting Ethereum P2P. | Execution full node |
| `discv4_ping` FAIL, `p2p_tcp_connect` OK | UDP:30303 blocked while TCP:30303 open. Discovery impossible, but existing peer connections would work if already established. | Execution full node |
| Both `p2p_tcp_connect` and `discv4_ping` FAIL | Complete execution P2P isolation. No node participation possible. | Execution full node, archive node |
| `p2p_tcp_connect` OK, `rlpx_handshake` timeout (not RST) | DPI filtering of RLPx auth packets. The firewall is identifying and dropping RLPx traffic specifically while allowing other TCP:30303 traffic. **Most targeted form of execution censorship.** | Execution full node |
| `rlpx_handshake` RST (not timeout) | Normal boot node rejection of unknown identity. Not censorship. | — (expected) |

#### Consensus layer (Section 3)

| Pattern observed | What it means | Participants affected |
|---|---|---|
| `beacon_https` FAIL | Beacon REST API blocked. Cannot sync from checkpoint, light clients affected. | Light client, consensus node, validator |
| `beacon_discv5_ping` FAIL | UDP:9000 blocked. Peer discovery impossible. | Consensus node, validator |
| `beacon_discv5_ping` OK, `beacon_tcp_connect` FAIL | TCP:9000 blocked while UDP:9000 open. Can discover peers but cannot connect. | Consensus node, validator |
| Both `beacon_discv5_ping` and `beacon_tcp_connect` FAIL | Complete consensus P2P isolation. | Consensus node, validator |
| `beacon_tcp_connect` OK, `libp2p_handshake` timeout | DPI filtering of libp2p multistream bytes. **Most targeted form of consensus censorship.** | Consensus node, validator |
| `libp2p_handshake` RST immediately after TCP connect | Peer rejects our protocol. Could be running a different libp2p implementation (IPFS, etc.). Not ISP-level censorship. | — (non-censorship failure) |

#### RPC provider layer (Section 1)

| Pattern observed | What it means | Participants affected |
|---|---|---|
| `dns_resolve` FAIL for a provider | Provider domain blocked at DNS. Cannot even resolve the address. | All users of that provider |
| `tls_handshake` FAIL with certificate error | TLS MITM interception. Government or corporate proxy is terminating SSL. | All HTTPS users |
| `https_jsonrpc` OK, `https_jsonrpc_write` FAIL (403/auth error) | Provider is accepting reads but blocking write/tx submission. OFAC compliance, geographic restriction. | Wallet users sending transactions, searchers |
| `https_jsonrpc` FAIL (HTTP 503), DNS/TCP/TLS OK | Backend outage (CDN layer up, origin server down). Not censorship. | That specific provider's users |
| `wss_subscribe` FAIL, `https_jsonrpc` OK | WebSocket specifically blocked. Common in corporate/government proxies. | dApps using subscriptions, searchers monitoring mempool |
| All providers' write paths FAIL simultaneously | Systemic write censorship across the entire RPC provider ecosystem. Would be an unprecedented event. | All Ethereum users in this location |

#### Cross-section patterns

| Pattern observed | What it means |
|---|---|
| Section 1 OK, Section 2 all fail | Can use Ethereum as a wallet, but cannot run an execution node. Selective P2P port blocking. |
| Section 1 OK, Section 3 all fail | Can use Ethereum as a wallet, but cannot run a validator/consensus node. |
| Sections 1+2 OK, `beacon_tcp_connect` fails | Can use wallet + execution node, but cannot participate in consensus layer P2P. The most specific consensus censorship scenario. |
| All sections fail | No Ethereum connectivity at all. Complete network-level block. |
| Section 2 OK, Section 1 fails for some providers | P2P network accessible, but specific RPC providers are blocked. Node operators unaffected; wallet users for those providers are. |

---

### 8.9 Target Reference: What Is Probed and Why

This subsection is a complete, concrete reference of every configured target: what type of entity it is, who operates it, which probes are generated, and exactly what each probe verifies. Targets are grouped by category. Dynamic targets (live peers from Beacon API) are described separately since they vary per run.

---

#### Category 1 — RPC Provider Endpoints (Section 1)

These are centralized middleware services that expose Ethereum JSON-RPC over HTTPS/WSS. They are not raw Ethereum nodes — internally they run full nodes, but externally they behave as API services. All 8 providers generate the same set of probes. publicnode and drpc additionally expose WebSocket and get 2 extra probes each.

| Target | Operator | Endpoint |
|---|---|---|
| publicnode | Public Node, Inc. | ethereum-rpc.publicnode.com |
| cloudflare | Cloudflare | cloudflare-eth.com |
| llamarpc | LlamaNodes | eth.llamarpc.com |
| drpc | dRPC | eth.drpc.org |
| flashbots | Flashbots | rpc.flashbots.net |
| 1rpc | 1RPC (Automata) | 1rpc.io/eth |
| mevblocker | CoW Protocol / Gnosis | rpc.mevblocker.io |
| blast | Blast API (Bware Labs) | eth-mainnet.public.blastapi.io |

**Probes generated per provider and what they verify:**

| Probe | Verifies |
|---|---|
| `dns_resolve` | The provider's domain resolves from the user's local resolver. If this fails, the domain is NXDOMAIN-blocked or DNS is broken. |
| `tcp_connect` (port 443) | TCP:443 is reachable. Tests whether the CDN layer's IP is blocked at the network level. |
| `tls_handshake` | Full TLS 1.3 handshake completes with a valid certificate chain. Detects MITM interception: if an ISP proxy terminates TLS, it presents a different certificate that fails verification. |
| `https_jsonrpc` | `eth_chainId` returns a JSON-RPC `result`. Verifies read-path access at the application level. A CDN returning 503 (backend down) or 403 (geo-blocked) is caught here while lower layers (DNS, TCP, TLS) still pass. |
| `https_jsonrpc_write` | `eth_sendRawTransaction` with an intentionally invalid payload (`"0x"`) returns any JSON-RPC response (including an error). Verifies write-path access. If this fails while read succeeds, the provider is selectively blocking transaction submission — OFAC compliance or geographic restriction. |
| `wss_jsonrpc` *(publicnode, drpc only)* | WebSocket connection upgrades and `eth_chainId` returns a result over WSS. Verifies WebSocket is not blocked by proxies. |
| `wss_subscribe` *(publicnode, drpc only)* | `eth_subscribe newHeads` returns a subscription ID and at least one block event. Verifies real-time subscription works end-to-end. |

---

#### Category 2 — Execution Boot Nodes (Section 2)

These are Ethereum Foundation-operated **devp2p boot nodes** for the execution layer. Their purpose is DiscV4 peer discovery — they maintain a list of execution peers and respond to discovery queries. They are full nodes configured and optimized for discoverability, not regular user-facing nodes. All 4 generate 3 probes each (P2P TCP, DiscV4 UDP, and RLPx).

| Target | Operator | IP | Region |
|---|---|---|---|
| EF-ap-southeast | Ethereum Foundation | 18.138.108.67 | AWS ap-southeast-1 (Singapore) |
| EF-us-east | Ethereum Foundation | 3.209.45.79 | AWS us-east-1 (Virginia) |
| EF-hetzner-hel | Ethereum Foundation | 65.108.70.101 | Hetzner Helsinki |
| EF-hetzner-fsn | Ethereum Foundation | 157.90.35.166 | Hetzner Falkenstein |

**Probes generated per boot node and what they verify:**

| Probe | Verifies |
|---|---|
| `p2p_tcp_connect` (port 30303) | TCP:30303 is reachable. If TCP:443 works but this fails, it indicates **selective port blocking of Ethereum execution P2P** — the most fundamental form of censorship targeting node operators. |
| `discv4_ping` (UDP port 30303) | A DiscV4 PING datagram (secp256k1-signed) receives a PONG response. Verifies that UDP:30303 is reachable and the DiscV4 discovery protocol works end-to-end. UDP being blocked while TCP:30303 works indicates asymmetric filtering. |
| `rlpx_handshake` (TCP port 30303) | An EIP-8 ECIES-encrypted RLPx auth packet is sent and the remote responds (with data, FIN, or RST). Any response = the packet was not DPI-filtered. A **timeout** (while TCP connects) means a DPI firewall is specifically identifying and dropping RLPx auth packets — the most targeted execution-layer censorship. RST = normal rejection of unknown identity, not censorship. |

---

#### Category 3 — DNS Comparison Targets (Section 2)

These are domain names queried against two resolvers simultaneously: the system resolver and Cloudflare DoH (1.1.1.1). No connection is made beyond the DNS query.

| Target | Type |
|---|---|
| ethereum-rpc.publicnode.com | Execution RPC provider |
| cloudflare-eth.com | Execution RPC provider |
| eth.llamarpc.com | Execution RPC provider |
| eth.drpc.org | Execution RPC provider |
| ethereum-beacon-api.publicnode.com | Consensus API provider |
| lodestar-mainnet.chainsafe.io | Consensus API provider |

**Probe generated:**

| Probe | Verifies |
|---|---|
| `dns_compare` | The system resolver (ISP's resolver) returns the same set of IP addresses as Cloudflare DoH. If they differ, it means the ISP's resolver is returning different IPs for Ethereum-related domains — active DNS manipulation. The mismatch is the censorship signal, not the specific IPs returned. |

---

#### Category 4 — Consensus Boot Nodes (Section 3, static)

These are **libp2p/DiscV5 boot nodes** for the Ethereum consensus layer. Like execution boot nodes, their purpose is peer discovery. They are configured as discovery-only infrastructure and deliberately block inbound TCP:9000 (libp2p connections). Each generates only 1 probe.

| Target | Client | Operator | IP | tcp4 in ENR |
|---|---|---|---|---|
| teku-aws-ohio | Teku (Java) | Consensys | 3.147.37.0 | 9000 (firewalled) |
| teku-aws-sydney | Teku (Java) | Consensys | 3.107.124.68 | 9000 (firewalled) |
| nimbus-frankfurt | Nimbus (Nim) | Status | 3.120.104.18 | 9100 (Prometheus port) |

**Probe generated:**

| Probe | Verifies |
|---|---|
| `beacon_discv5_ping` | A DiscV5 PING datagram receives a PONG. Verifies that UDP:9000 is reachable and the DiscV5 consensus peer discovery protocol works end-to-end. Failing here means consensus peer discovery is blocked — the node cannot find peers to connect to. |

> **Why no TCP/libp2p to these?** These boot nodes deliberately block inbound TCP:9000 (by design, not misconfiguration). Probing them for TCP/libp2p always returns a known constant result (connection refused or wrong port). TCP and libp2p probes now target Category 6 (live peer nodes) instead.

---

#### Category 5 — Beacon API Nodes (Section 3)

These are consensus nodes that expose the standard Beacon REST API. They serve the same role as execution RPC providers but for the consensus layer. Additionally, they are the **source** of Category 6 live peer targets.

| Target | Client | Operator | URL |
|---|---|---|---|
| publicnode | Unknown | Public Node, Inc. | ethereum-beacon-api.publicnode.com |
| chainsafe-lodestar | Lodestar (TypeScript) | ChainSafe Systems | lodestar-mainnet.chainsafe.io |

**Probes generated:**

| Probe | Verifies |
|---|---|
| `beacon_https` | `GET /eth/v1/node/version` returns HTTP 200 with a JSON body containing the client name and version. Verifies that the Beacon REST API is accessible — needed by light clients, checkpoint sync, and monitoring tools. |
| *(indirect)* live peer fetch | Before running probes, `GET /eth/v1/node/peers?state=connected` is called to discover Category 6 targets. Each outbound peer generates 2 probes (see below). |

---

#### Category 6 — Consensus Peer Nodes (Section 3, dynamic)

These are **real consensus full nodes** discovered live at runtime from the Beacon API. Unlike boot nodes (Category 4), these nodes participate fully in block propagation and attestation gossip. They have been confirmed reachable by the beacon API nodes (the API node connected TO them — `direction=outbound`), so they have publicly routable IPs and open ports. The set changes every run depending on who is currently connected.

| Source | Typical count | Peer diversity |
|---|---|---|
| publicnode peers | ~130 unique outbound peers | Global; data center nodes and staking infrastructure |
| chainsafe-lodestar peers | ~3 unique outbound peers | Global; Lodestar has fewer stable outbound connections |

These are a mix of:
- **Validator nodes**: running a beacon client + validator keys; externally identical to sync nodes
- **Non-validating sync nodes**: archiving or providing data
- **Staking pool infrastructure**: Rocket Pool, Lido, etc.
- **Client team nodes**: Lighthouse, Prysm, Teku, Nimbus, Lodestar team-operated nodes

**Probes generated per live peer:**

| Probe | Target | Verifies |
|---|---|---|
| `beacon_tcp_connect` | `IP:PORT` from peer's multiaddr | TCP:9000 (or whatever port the peer advertises) is reachable from the prober's location. If the beacon node can reach the peer but the prober cannot, it indicates selective blocking of that IP:port from the prober's ISP. |
| `libp2p_handshake` | Same `IP:PORT` | The remote speaks the libp2p multistream-select protocol. Sends `/multistream/1.0.0` and `/noise`, checks for acknowledgement. Verifies the peer is a real libp2p node and that the multistream protocol bytes are not being filtered by DPI. A timeout here (TCP connects but no response) is the signature of **libp2p-specific DPI filtering** — the most targeted form of consensus censorship. |

---

#### Summary: probe count per run

| Category | Targets | Probes each | Total probes |
|---|---|---|---|
| 1 — RPC providers (base) | 8 | 5 (`dns_resolve`, `tcp_connect`, `tls_handshake`, `https_jsonrpc`, `https_jsonrpc_write`) | 40 |
| 1 — RPC providers (WSS) | 2 (publicnode, drpc) | 2 extra (`wss_jsonrpc`, `wss_subscribe`) | 4 |
| 2 — Execution boot nodes | 4 | 3 (`p2p_tcp_connect`, `discv4_ping`, `rlpx_handshake`) | 12 |
| 3 — DNS compare | 6 | 1 (`dns_compare`) | 6 |
| 4 — Consensus boot nodes | 3 | 1 (`beacon_discv5_ping`) | 3 |
| 5 — Beacon API nodes | 2 | 1 (`beacon_https`) | 2 |
| 6 — Live consensus peers | ~133 (varies) | 2 (`beacon_tcp_connect`, `libp2p_handshake`) | ~266 |
| **Total** | | | **~333** |

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

### Section 3 — Consensus Layer P2P (variable probes: ~5 static + N×2 dynamic)

#### 3.1 DiscV5 Beacon Ping (3 probes: all OK)

| Node | IP | avg RTT | Network model | Assessment |
|---|---|---|---|---|
| teku-aws-ohio | 3.147.37.0 (Ohio, US) | 241 ms | ~100 ms one-way → ~200 ms net → +41 ms processing | Server-side DiscV5 crypto adds ~41 ms |
| teku-aws-sydney | 3.107.124.68 (Sydney, AU) | 557 ms | ~170 ms one-way → ~340 ms net → +217 ms processing | High processing overhead — JVM-based Teku may have higher DiscV5 response latency |
| nimbus-frankfurt | 3.120.104.18 (Frankfurt, DE) | 100 ms | ~18 ms one-way → ~36 ms net → +64 ms processing | Significant processing relative to network; Frankfurt is close, most of the RTT is server-side crypto |

DiscV5 success confirms UDP:9000 is reachable to all 3 hosts. The large server-side processing component (especially for Teku Sydney at +217 ms) is consistent with JVM garbage collection or cold code paths in the DiscV5 implementation. Nimbus (Go/Nim native) shows less processing overhead per round trip despite similar infrastructure.

#### 3.2 Beacon TCP + libp2p — Dynamic Live Peers

Boot nodes are no longer probed for TCP or libp2p. Instead, `beacon_tcp_connect` and `libp2p_handshake` probes target real consensus peer nodes discovered dynamically from the Beacon REST API (`/eth/v1/node/peers?state=connected`, `direction=outbound`). The number of probes is variable per run.

**Why `direction=outbound`**: From the beacon node's perspective, "outbound" means the beacon node initiated the connection TO that peer — proving the peer has a publicly routable IP and open port. "Inbound" peers (those that connected TO the beacon node) include home validators behind NAT who can make outbound connections but cannot accept unsolicited inbound probes from a third party.

**Typical result pattern** (from Spain, 2026-05-15 run with PublicNode as source):
- ~70 unique outbound peers probed per run
- Beacon TCP: ~95% OK (latency consistent with geographic distance to peer)
- libp2p handshake: ~94% OK among TCP-successful peers
- libp2p RTT ≈ 3× TCP RTT — consistent with 2 request-response cycles in the multistream negotiation

**Remaining libp2p failures** (~6%): TCP connects but libp2p times out. Causes include: the peer changed state after the connection was established (disconnected between the API call and our probe), the peer is rate-limiting new connections, or the specific port is serving a different protocol.

**Historical note on boot node TCP/libp2p probing**: Teku boot nodes (3.147.37.0, 3.107.124.68) previously showed 2585–3398 ms "connection refused" when probed on TCP:9000. The anomalously high RTT (25× the geographic model) indicated a DROP-then-RST-after-retransmits firewall pattern: the SYN is initially dropped, the OS retransmits twice (~1 s each), and then the firewall sends RST. This information is preserved in §11bis and §11.4b. These probes were removed because they measure a boot node's deliberate firewall policy rather than network-level censorship.

#### 3.3 Beacon API (2 probes: both OK)

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
| TCP:9000 (live peers) | ✓ ~95% of peers reachable | Dynamic peers from Beacon API; data center nodes with public IPs |
| libp2p multistream | ✓ ~94% of TCP-reachable peers | Genuine protocol success; dynamic peers accept multistream negotiation |
| Beacon HTTPS API | ✓ 2/2 providers | Consensus data accessible |

**Outbound connectivity**: Essentially unconstrained from this network for Ethereum use. All critical layers (DNS, TCP:443, TCP:30303, UDP:30303, UDP:9000) are open. No evidence of ISP-level filtering of any Ethereum protocol.

**Gap not measured**: All probes above test *outbound* connectivity — whether the client can reach Ethereum infrastructure. The complementary question — whether Ethereum infrastructure (or peers) can reach the client — is not currently tested. This is the most common gap for residential users operating full or consensus nodes, where inbound connections to port 30303 or 9000 may be blocked by CGNAT or ISP policy regardless of outbound freedom. See §12 for the reverse connectivity design.

---

## 9. Technical Decisions and Justifications

### 9.1 Separate `[[probes.rlpx_targets]]` with enode:// URLs

**Why not extend `p2p_boot_nodes`?** The `p2p_boot_nodes` section contains `TcpTarget` entries with only `name`, `host`, and `port`. The RLPx handshake additionally requires the remote node's static public key for ECIES encryption. The enode:// URL is the standard format for carrying this information in the execution layer (analogous to ENR in the consensus layer). Adding an optional `pubkey` field to `TcpTarget` would violate the single-responsibility principle of each config type. A dedicated `RlpxTarget` with an `enode` string is self-contained and maps directly to the protocol's identity format.

### 9.2 Beacon TCP and libp2p Targets: From Static ENR to Dynamic Live Peers

**Original design**: `beacon_tcp_connect` and `libp2p_handshake` probes were auto-derived from `discv5_consensus` ENRs. One ENR entry generated three probes: DiscV5 ping, TCP connect, and libp2p multistream. The rationale was that ENRs are self-authenticating (signed by the node's private key) and act as a single source of truth for IP and port.

**Why this was changed**: Empirical testing showed that all available consensus boot nodes deliberately block inbound TCP:9000. Boot nodes serve only DiscV5 peer discovery (UDP) and explicitly reject inbound libp2p connections — this is by design, not a network issue. Probing boot nodes for TCP and libp2p produces a known constant result (connection refused or timeout) that provides no censorship signal. After exhaustive evaluation of 13 candidate ENRs across all major client teams (see §11.4b), zero nodes accepted inbound libp2p.

**Current design**: `beacon_tcp_connect` and `libp2p_handshake` targets are generated dynamically at runtime by querying the Beacon REST API (`/eth/v1/node/peers?state=connected`) on each configured `beacon_https` endpoint. Only peers with `direction=outbound` are used.

**Why `direction=outbound`?** The direction field is from the beacon node's perspective. "Outbound" means the beacon node (publicnode/chainsafe) *initiated* the connection TO that peer — this proves the peer has a publicly routable IP and an open listening port, because the beacon node successfully dialed it. "Inbound" peers (nodes that connected TO the beacon node) may be home validators behind CGNAT who can make outbound connections but cannot accept unsolicited inbound connections from a third-party prober. This distinction is geographically neutral: any node reachable by a well-connected infrastructure node on the public internet should also be reachable by the prober, regardless of geographic location.

**Results**: Dynamic live peers from PublicNode's Beacon API achieve ~95% success rate for both TCP and libp2p, versus ~0% for static boot node targets. Chainsafe/Lodestar has a lower rate because Lodestar attracts diverse peers including some that have changed state since the connection was established.

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

**How IPs are extracted in build_jobs for DiscV5**: At probe build time, `build_jobs()` parses each ENR with the `discv5` crate to extract the UDP address for the DiscV5 ping probe. ENRs are no longer used to derive TCP targets for `beacon_tcp_connect` or `libp2p_handshake` — those come from the live Beacon API peer fetch. The diagnostic test `cargo test -p prober_core --features discv5 -- enr_decode --nocapture` decodes ENRs and prints all ip/tcp/udp fields, retained as a developer diagnostic tool.

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

**Observed degradation (2026-05-15)**: A more severe failure mode has appeared — instead of a fast 503, the CDN itself becomes intermittent. HTTPS RPC shows avg 1714 ms (min 57 ms, max 5004 ms): one attempt gets a 503 quickly, the other two hit the 5000 ms timeout with no response at all. HTTPS Write shows avg 5006 ms with "network error" — all attempts time out, the CDN is no longer forwarding or responding. This indicates the CDN/load balancer layer itself is overloaded or failing, not just the origin. DNS, TCP, TLS still succeed (basic infrastructure up), but HTTPS is effectively unusable.

### Consensus Boot Nodes — DiscV5 Only (TCP/libp2p No Longer Probed)

The 3 remaining `discv5_consensus` boot nodes now generate **only DiscV5 ping probes**. TCP:9000 and libp2p probes are no longer derived from their ENRs. See §9.2 and §11.4b for the full rationale.

For reference, the historical boot node TCP/libp2p behaviour (documented here because it informed the architecture change):

- **Teku boot nodes** (teku-aws-ohio/sydney): TCP:9000 → connection refused with anomalous 2500–3400 ms latency (DROP-then-RST-after-SYN-retransmit firewall pattern). DiscV5 UDP:9000 succeeds. This confirms the port is actively blocked, not unreachable.
- **Nimbus Frankfurt**: TCP:9100 → connects (Prometheus metrics port), libp2p → timeout (wrong protocol on that port). DiscV5 UDP:9100 succeeds.

The conclusion from testing all 13 candidate boot node ENRs (see §11.4b): zero boot nodes accept inbound libp2p. TCP and libp2p probes now target real peer nodes discovered dynamically from the Beacon API (see §12.0).

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

### 12.0 Live Beacon Peer Probing (implemented 2026-05-15)

Instead of relying on hardcoded ENR entries for `beacon_tcp_connect` and `libp2p_handshake` probes — all of which are boot nodes that deliberately reject libp2p — the prober now dynamically discovers live peers by querying the Beacon REST API `/eth/v1/node/peers?state=connected` on each configured `beacon_https` endpoint before running the probe suite.

**Flow**: `run_plan()` spawns one tokio task per `beacon_https` entry, each calling the peers endpoint with the run's configured timeout. Results are filtered, deduplicated by `(host, port)` across sources, and converted into `BeaconTcpConnect` + `LibP2pHandshake` probe jobs. These are added to the static job list and run in the normal semaphore-limited parallel pool.

**Peer filter (`direction=outbound`)**: Only peers where the beacon node *initiated* the connection are used. From the API's perspective, `direction=outbound` means the beacon node connected TO that peer — proving it has a publicly routable IP and open port. `direction=inbound` peers (nodes that connected TO the beacon node) may be home validators behind NAT: they can make outbound connections but cannot accept unsolicited inbound probes from a third-party observer. This filter is geographically neutral.

**Additional filters**: Private, loopback, link-local, ULA, and RFC-6598 CGNAT (100.64.0.0/10) addresses are discarded as non-routable from the public internet.

**DiscV5 not added for live peers**: DiscV5 is the peer *discovery* protocol — its purpose is to find peers you don't yet know about. Live peers returned by the Beacon API are already discovered and connected. Adding a DiscV5 ping per peer would require parsing their ENR (not always present in the API response), add complexity, and provide no additional information beyond what TCP + libp2p already confirm. Only DiscV5 to static boot nodes is retained for measuring discovery-layer reachability.

**What changes vs static targets**: The number of Section 3 probes is variable. Each outbound peer is a full consensus node (validator or sync node) that accepted a TCP:9000 connection from the beacon infrastructure node. These nodes have realistic chances of accepting libp2p connections from the prober. Observed results: ~95% TCP success, ~94% libp2p success from PublicNode peers.

**Architectural impact**: `run_plan()` acquires one HTTP round trip per `beacon_https` URL before running probes (a few hundred ms, concurrent). The probe count grows by `N × 2` where N is the number of unique outbound peers across all beacon sources. In practice: PublicNode returns ~130 outbound peers, Chainsafe returns ~3 (Lodestar has a more conservative peering policy). Total Section 3 probes are typically ~273 (5 static + ~268 dynamic), running in ~22 seconds with parallelism=20.

**To remove**: delete `probes/beacon_peers.rs` and remove the two lines in `lib.rs::run_plan` that call `beacon_peers::fetch_all` and `build_live_peer_jobs`.

---

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
