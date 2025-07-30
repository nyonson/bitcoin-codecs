//! Example: Synchronous TCP Bitcoin client

use bitcoin::network::message::NetworkMessage;
use bitcoin::Network;
use bitcoin_codecs::v1_frame_decoder;
use push_decode::decode_sync_with;
use std::io::{BufReader, Write};
use std::net::TcpStream;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to a Bitcoin node
    let stream = TcpStream::connect("127.0.0.1:8333")?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;

    // Send version message
    let version_msg = create_version_message();
    writer.write_all(&version_msg)?;
    writer.flush()?;

    // Read messages
    loop {
        let decoder = v1_frame_decoder(Network::Bitcoin);
        let message = decode_sync_with(&mut reader, decoder)?;

        println!("Received: {:?}", message.cmd());

        match message {
            NetworkMessage::Version(version) => {
                println!("  Version: {}", version.version);
                println!("  User Agent: {}", version.user_agent);
                println!("  Services: {:?}", version.services);

                // Send verack
                let verack = create_verack_message();
                writer.write_all(&verack)?;
                writer.flush()?;
            }
            NetworkMessage::Ping(nonce) => {
                println!("  Ping nonce: {}", nonce);

                // Send pong
                let pong = create_pong_message(nonce);
                writer.write_all(&pong)?;
                writer.flush()?;
            }
            NetworkMessage::Inv(inventory) => {
                println!("  Inventory: {} items", inventory.len());
                for item in &inventory {
                    println!("    {:?}: {}", item.inv_type, item.hash);
                }
            }
            NetworkMessage::Addr(addresses) => {
                println!("  Addresses: {} peers", addresses.len());
                for (addr, _) in addresses.iter().take(5) {
                    println!("    {}", addr);
                }
            }
            _ => {}
        }
    }
}

fn create_version_message() -> Vec<u8> {
    // This would use bitcoin crate's serialization
    // For now, just a placeholder
    vec![]
}

fn create_verack_message() -> Vec<u8> {
    // Placeholder
    vec![]
}

fn create_pong_message(nonce: u64) -> Vec<u8> {
    // Placeholder
    vec![]
}
