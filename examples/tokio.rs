//! Example: Async usage with Tokio

use bitcoin::network::message::NetworkMessage;
use bitcoin::Network;
use bitcoin_codecs::v1_frame_decoder;
use push_decode::decode_tokio_with;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to a Bitcoin node
    let stream = TcpStream::connect("127.0.0.1:8333").await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Send version message
    let version_msg = create_version_message();
    writer.write_all(&version_msg).await?;
    writer.flush().await?;

    // Read messages in a loop
    loop {
        let decoder = v1_frame_decoder(Network::Bitcoin);

        match decode_tokio_with(&mut reader, decoder).await {
            Ok(message) => {
                println!("Received: {:?}", message.cmd());

                match message {
                    NetworkMessage::Version(version) => {
                        println!("  Version: {}", version.version);
                        println!("  User Agent: {}", version.user_agent);

                        // Send verack
                        let verack = create_verack_message();
                        writer.write_all(&verack).await?;
                        writer.flush().await?;
                    }
                    NetworkMessage::Ping(nonce) => {
                        println!("  Ping nonce: {}", nonce);

                        // Send pong
                        let pong = create_pong_message(nonce);
                        writer.write_all(&pong).await?;
                        writer.flush().await?;
                    }
                    NetworkMessage::Inv(inventory) => {
                        println!("  Inventory: {} items", inventory.len());
                    }
                    _ => {}
                }
            }
            Err(e) => {
                eprintln!("Error: {:?}", e);
                break;
            }
        }
    }

    Ok(())
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

fn create_pong_message(_nonce: u64) -> Vec<u8> {
    // Placeholder
    vec![]
}
