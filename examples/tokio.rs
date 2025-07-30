//! Async usage with Tokio

use bitcoin::p2p::message::NetworkMessage;
use bitcoin::Network;
use bitcoin_codecs::V1MessageDecoder;
use push_decode::decode_tokio_with;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stream = TcpStream::connect("127.0.0.1:8333").await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    let version_msg = create_version_message();
    writer.write_all(&version_msg).await?;
    writer.flush().await?;

    loop {
        let decoder = V1MessageDecoder::new(Network::Bitcoin);

        match decode_tokio_with(&mut reader, decoder).await {
            Ok(message) => {
                println!("Received: {:?}", message.cmd());

                match message {
                    NetworkMessage::Version(version) => {
                        println!("  Version: {}", version.version);
                        println!("  User Agent: {}", version.user_agent);
                        let verack = create_verack_message();
                        writer.write_all(&verack).await?;
                        writer.flush().await?;
                    }
                    NetworkMessage::Ping(nonce) => {
                        println!("  Ping nonce: {nonce}");
                        let pong = create_pong_message(nonce);
                        writer.write_all(&pong).await?;
                        writer.flush().await?;
                    }
                    _ => {}
                }
            }
            Err(e) => {
                eprintln!("Error: {e:?}");
                break;
            }
        }
    }

    Ok(())
}

fn create_version_message() -> Vec<u8> {
    use bitcoin::p2p::message_network::VersionMessage;
    use bitcoin::p2p::{Address, ServiceFlags};
    use bitcoin::{consensus::encode, p2p::message::RawNetworkMessage};
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let version = VersionMessage {
        version: 70015,
        services: ServiceFlags::NONE,
        timestamp,
        receiver: Address::new(&"127.0.0.1:8333".parse().unwrap(), ServiceFlags::NONE),
        sender: Address::new(&"0.0.0.0:0".parse().unwrap(), ServiceFlags::NONE),
        nonce: 0x1234567890abcdef, // Hardcoded nonce
        user_agent: "/bitcoin-codecs:0.1.0/".to_string(),
        start_height: 0,
        relay: false,
    };

    let msg = RawNetworkMessage::new(Network::Bitcoin.magic(), NetworkMessage::Version(version));

    encode::serialize(&msg)
}

fn create_verack_message() -> Vec<u8> {
    use bitcoin::{consensus::encode, p2p::message::RawNetworkMessage};

    let msg = RawNetworkMessage::new(Network::Bitcoin.magic(), NetworkMessage::Verack);

    encode::serialize(&msg)
}

fn create_pong_message(nonce: u64) -> Vec<u8> {
    use bitcoin::{consensus::encode, p2p::message::RawNetworkMessage};

    let msg = RawNetworkMessage::new(Network::Bitcoin.magic(), NetworkMessage::Pong(nonce));

    encode::serialize(&msg)
}
