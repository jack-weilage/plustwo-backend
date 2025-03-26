use eyre::Result;
use twitch_api::{
    client::ClientDefault as _,
    twitch_oauth2::{TwitchToken as _, UserToken},
};

pub struct TwitchClient {
    client: twitch_api::HelixClient<'static, reqwest::Client>,
    token: UserToken,
}
impl TwitchClient {
    /// Constructs a new client from a refresh token and client ID
    pub async fn new(refresh_token: &str, client_id: &str) -> Result<Self> {
        let client = twitch_api::HelixClient::with_client(
            reqwest::Client::default_client_with_name(Some("plustwo.live watcher".parse()?))?,
        );
        let token =
            UserToken::from_refresh_token(&client, refresh_token.into(), client_id.into(), None)
                .await?;

        Ok(Self { client, token })
    }
    /// Refreshes the token if the token is outdated.
    pub async fn refresh_token(&mut self) -> Result<()> {
        if self.token.is_elapsed() {
            self.token.refresh_token(&self.client).await?;
        }

        Ok(())
    }

    /// Subscribes to an EventSub event.
    pub async fn subscribe<S: twitch_api::eventsub::EventSubscription + Send>(
        &self,
        transport: twitch_api::eventsub::Transport,
        subscription: S,
    ) -> Result<()> {
        self.client
            .create_eventsub_subscription(subscription, transport, &self.token)
            .await?;

        Ok(())
    }
}
