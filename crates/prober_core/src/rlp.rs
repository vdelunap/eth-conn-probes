// Minimal RLP encoder — covers only what DiscV4 and RLPx probes need.
// No external dependencies.

pub fn rlp_bytes(data: &[u8]) -> Vec<u8> {
    if data.len() == 1 && data[0] < 0x80 {
        vec![data[0]]
    } else if data.len() <= 55 {
        let mut out = vec![0x80 + data.len() as u8];
        out.extend_from_slice(data);
        out
    } else {
        let len_enc = be_bytes_minimal(data.len() as u64);
        let mut out = vec![0xb7 + len_enc.len() as u8];
        out.extend_from_slice(&len_enc);
        out.extend_from_slice(data);
        out
    }
}

pub fn rlp_uint(n: u64) -> Vec<u8> {
    if n == 0 {
        vec![0x80]
    } else {
        let b = be_bytes_minimal(n);
        rlp_bytes(&b)
    }
}

pub fn rlp_list(items: &[Vec<u8>]) -> Vec<u8> {
    let payload: Vec<u8> = items.iter().flat_map(|i| i.iter().copied()).collect();
    if payload.len() <= 55 {
        let mut out = vec![0xc0 + payload.len() as u8];
        out.extend_from_slice(&payload);
        out
    } else {
        let len_enc = be_bytes_minimal(payload.len() as u64);
        let mut out = vec![0xf7 + len_enc.len() as u8];
        out.extend_from_slice(&len_enc);
        out.extend_from_slice(&payload);
        out
    }
}

pub fn be_bytes_minimal(n: u64) -> Vec<u8> {
    let b = n.to_be_bytes();
    let start = b.iter().position(|&x| x != 0).unwrap_or(7);
    b[start..].to_vec()
}
