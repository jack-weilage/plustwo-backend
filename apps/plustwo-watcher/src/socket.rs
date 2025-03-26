use eyre::{Context as _, Result, bail};
use tokio_stream::StreamExt as _;
use tokio_tungstenite::tungstenite;

pub struct EventSubSocket {
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    url: String,
}
impl EventSubSocket {
    /// Connects to the specified URL via WebSocket.
    pub async fn connect(url: &str) -> Result<Self> {
        let config = tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(64 << 20))
            .max_frame_size(Some(16 << 20))
            .accept_unmasked_frames(false);

        let (socket, _) = tokio_tungstenite::connect_async_with_config(url, Some(config), false)
            .await
            .wrap_err("Failed to connect to eventsub websocket")?;

        Ok(Self {
            socket,
            url: url.to_string(),
        })
    }

    /// Waits for the next message, handling disconnects.
    pub async fn next_message(&mut self) -> Result<tungstenite::Utf8Bytes> {
        while let Some(msg) = self.socket.next().await {
            match msg {
                Ok(tungstenite::Message::Text(msg)) => return Ok(msg),
                Ok(_) => continue,
                Err(tungstenite::Error::Protocol(
                    tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
                )) => {
                    println!("Connection reset, attempting to reconnect...");

                    self.socket.close(None).await?;
                    *self = Self::connect(&self.url).await?;
                }
                Err(e) => return Err(e.into()),
            }
        }

        bail!("next_message somehow didn't recieve a text message before terminating?");
    }
}
