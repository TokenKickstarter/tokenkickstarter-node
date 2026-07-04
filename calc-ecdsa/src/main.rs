use sp_core::{ecdsa, Pair};
use keccak_hash::keccak;
use sha3::{Digest, Keccak256};
use parity_scale_codec::{Encode, Compact};

fn sha3_keccak256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

fn main() {
    println!("=== Full Extrinsic Submission Test ===\n");

    // 1. Use //Alice dev key
    let pair = ecdsa::Pair::from_string("//Alice", None).expect("Valid seed");
    let pubkey = pair.public();
    let pkey = libsecp256k1::PublicKey::parse_compressed(&pubkey.0)
        .expect("Valid public key");
    let uncompressed = pkey.serialize();
    let hash = keccak(&uncompressed[1..65]);
    let address: [u8; 20] = hash.0[12..32].try_into().unwrap();
    let secret_raw = pair.seed();
    println!("Address: 0x{}", hex::encode(&address));

    // 2. Real chain parameters
    let genesis_hash = hex::decode("49397761c3e00070b4aef07df523f658b19ad9da0112584de30e0e9dd78d59e4").unwrap();
    let spec_version: u32 = 200;
    let tx_version: u32 = 1;

    // 3. Build call data: NameRegistry::register("alice_test")
    let pallet_index: u8 = 20; // NameRegistry pallet index
    let call_index: u8 = 0;    // register
    let name_bytes = b"alice_test".to_vec();
    let mut call_data = Vec::new();
    call_data.push(pallet_index);
    call_data.push(call_index);
    Compact(name_bytes.len() as u32).encode_to(&mut call_data);
    call_data.extend_from_slice(&name_bytes);
    println!("Call data: 0x{}", hex::encode(&call_data));

    // 4. Build extension (goes in extrinsic body)
    let mut extra = Vec::new();
    extra.push(0x00u8);  // Era: immortal
    Compact(0u32).encode_to(&mut extra);  // Nonce: 0
    Compact(0u128).encode_to(&mut extra);  // Tip: 0
    println!("Extension: 0x{}", hex::encode(&extra));

    // 5. Build signing payload: call + extra + implicit
    let mut payload = Vec::new();
    payload.extend_from_slice(&call_data);
    payload.extend_from_slice(&extra);
    spec_version.encode_to(&mut payload); // u32 LE
    tx_version.encode_to(&mut payload);   // u32 LE
    payload.extend_from_slice(&genesis_hash); // 32 bytes
    payload.extend_from_slice(&genesis_hash); // checkpoint = genesis for immortal era
    println!("Signing payload ({} bytes): 0x{}", payload.len(), hex::encode(&payload));

    // 6. Hash and sign
    let msg_hash = sha3_keccak256(&payload);
    println!("Keccak256(payload): 0x{}", hex::encode(&msg_hash));

    let secp = secp256k1::Secp256k1::new();
    let sk = secp256k1::SecretKey::from_slice(&secret_raw).expect("Valid SK");
    let msg = secp256k1::Message::from_digest(msg_hash);
    let sig_recoverable = secp.sign_ecdsa_recoverable(&msg, &sk);
    let (rec_id, sig_bytes_64) = sig_recoverable.serialize_compact();
    let mut sig_65 = [0u8; 65];
    sig_65[..64].copy_from_slice(&sig_bytes_64);
    sig_65[64] = rec_id.to_i32() as u8;
    println!("Signature (65): 0x{}", hex::encode(&sig_65));
    println!("Recovery ID: {}", rec_id.to_i32());

    // Quick verification
    let lib_msg = libsecp256k1::Message::parse(&msg_hash);
    let recover_sig = libsecp256k1::Signature::parse_standard_slice(&sig_65[..64]).unwrap();
    let recover_rec = libsecp256k1::RecoveryId::parse(sig_65[64]).unwrap();
    let pub_recovered = libsecp256k1::recover(&lib_msg, &recover_sig, &recover_rec).unwrap();
    let uncompressed2 = pub_recovered.serialize();
    let hash2 = keccak(&uncompressed2[1..65]);
    let recovered_addr: [u8; 20] = hash2.0[12..32].try_into().unwrap();
    println!("Recovered address: 0x{}", hex::encode(&recovered_addr));
    println!("Matches: {}", recovered_addr == address);

    // 7. Build the full extrinsic
    let mut extrinsic_body = Vec::new();
    extrinsic_body.push(0x84u8); // Version: signed (0x80) | v4 (0x04)
    extrinsic_body.extend_from_slice(&address); // 20 bytes
    extrinsic_body.extend_from_slice(&sig_65);  // 65 bytes
    extrinsic_body.extend_from_slice(&extra);   // extension
    extrinsic_body.extend_from_slice(&call_data); // call

    let mut extrinsic = Vec::new();
    Compact(extrinsic_body.len() as u32).encode_to(&mut extrinsic);
    extrinsic.extend_from_slice(&extrinsic_body);

    let ext_hex = format!("0x{}", hex::encode(&extrinsic));
    println!("\nFull extrinsic: {}", ext_hex);
    println!("Extrinsic length: {} bytes", extrinsic.len());

    // 8. Submit to local node
    println!("\n=== Submitting to node... ===");
    let body = format!(
        r#"{{"id":1,"jsonrpc":"2.0","method":"author_submitExtrinsic","params":["{}"]}}"#,
        ext_hex
    );

    let response = std::process::Command::new("curl")
        .args(&["-s", "-H", "Content-Type: application/json", "-d", &body, "http://127.0.0.1:9944"])
        .output()
        .expect("curl failed");
    let resp_str = String::from_utf8_lossy(&response.stdout);
    println!("Response: {}", resp_str);

    // If it fails, also try with the nonce from the RPC
    if resp_str.contains("error") {
        println!("\n=== Checking nonce... ===");
        // Get account nonce
        let nonce_body = format!(
            r#"{{"id":1,"jsonrpc":"2.0","method":"system_accountNextIndex","params":["0x{}"]}}"#,
            hex::encode(&address)
        );
        let nonce_resp = std::process::Command::new("curl")
            .args(&["-s", "-H", "Content-Type: application/json", "-d", &nonce_body, "http://127.0.0.1:9944"])
            .output()
            .expect("curl failed");
        println!("Nonce response: {}", String::from_utf8_lossy(&nonce_resp.stdout));
    }
}
