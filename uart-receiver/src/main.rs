use bytes::{Buf, BytesMut};
use color_eyre::eyre::Result;
use prost::Message;
use std::path::Path;
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;
use tracing::{debug, error, info};

const SOCKET_PATH: &str = "/tmp/orb-uart.sock";
const MAGIC_HEADER: [u8; 2] = [0x8E, 0xAD];

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize color-eyre for better error reporting
    color_eyre::install()?;

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("uart_receiver=debug".parse()?),
        )
        .init();

    info!("Starting UART receiver on socket: {}", SOCKET_PATH);

    // Remove existing socket if it exists
    if Path::new(SOCKET_PATH).exists() {
        std::fs::remove_file(SOCKET_PATH)?;
    }

    // Create Unix socket listener
    let listener = UnixListener::bind(SOCKET_PATH)?;
    info!("Listening for connections...");

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                info!("New connection established");

                // Spawn a task to handle this connection
                tokio::spawn(async move {
                    let mut stream = stream;
                    let mut buffer = BytesMut::with_capacity(4096);

                    loop {
                        // Read data from socket
                        let n = match stream.read_buf(&mut buffer).await {
                            Ok(0) => {
                                info!("Connection closed");
                                break;
                            }
                            Ok(n) => n,
                            Err(e) => {
                                error!("Failed to read from socket: {}", e);
                                break;
                            }
                        };

                        debug!("Received {} bytes", n);

                        // Process complete messages
                        while buffer.len() >= 4 {
                            // Check for magic header
                            if buffer[0..2] != MAGIC_HEADER {
                                continue;
                            }

                            // Read message size
                            let size =
                                u16::from_le_bytes([buffer[2], buffer[3]]) as usize;
                            let total_frame_size = 4 + size; // header + size + payload

                            // Extract and decode the protobuf message
                            let payload = &buffer[4..total_frame_size];

                            match orb_messages::McuMessage::decode_length_delimited(
                                payload,
                            ) {
                                Ok(message) => {
                                    info!("Received McuMessage:");
                                    info!("Version: {:?}", message.version);

                                    if let Some(msg) = message.message {
                                        // Log the message type and basic info
                                        match msg {
                                            orb_messages::mcu_message::Message::JMessage(jetson_msg) => {
                                                info!("Type: JetsonToMcu");
                                                info!("Message details: {:?}", jetson_msg);
                                            }
                                            orb_messages::mcu_message::Message::MMessage(_) => {
                                                info!("ype: McuToJetson");
                                            }
                                            orb_messages::mcu_message::Message::JetsonToSecMessage(sec_msg) => {
                                                info!("Type: JetsonToSec");
                                                info!("Message details: {:?}", sec_msg);
                                            }
                                            orb_messages::mcu_message::Message::SecToJetsonMessage(_) => {
                                                info!("Type: SecToJetson");
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to decode protobuf message: {}", e);
                                    debug!("Payload bytes: {:?}", payload);
                                }
                            }

                            // Remove processed message from buffer
                            buffer.advance(total_frame_size);
                        }
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
            }
        }
    }
}
