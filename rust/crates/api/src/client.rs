use crate::error::ApiError;
use crate::providers::kla_provider::{self, AuthSource, KlaApiClient};
use crate::providers::openai_compat::{self, OpenAiCompatClient, OpenAiCompatConfig};
use crate::providers::{self, Provider, ProviderKind};
use crate::types::{MessageRequest, MessageResponse, StreamEvent};

async fn send_via_provider<P: Provider>(
    provider: &P,
    request: &MessageRequest,
) -> Result<MessageResponse, ApiError> {
    provider.send_message(request).await
}

async fn stream_via_provider<P: Provider>(
    provider: &P,
    request: &MessageRequest,
) -> Result<P::Stream, ApiError> {
    provider.stream_message(request).await
}

#[derive(Debug, Clone)]
pub enum ProviderClient {
    KlaApi(KlaApiClient),
    Xai(OpenAiCompatClient),
    OpenAi(OpenAiCompatClient),
}

impl ProviderClient {
    pub fn from_model(model: &str) -> Result<Self, ApiError> {
        Self::from_model_with_default_auth(model, None)
    }

    pub fn from_model_with_default_auth(
        model: &str,
        default_auth: Option<AuthSource>,
    ) -> Result<Self, ApiError> {
        let resolved_model = providers::resolve_model_alias(model);
        let kind = providers::detect_provider_kind(&resolved_model);
        
        match kind {
            ProviderKind::KlaApi => Ok(Self::KlaApi(match default_auth {
                Some(auth) => KlaApiClient::from_auth(auth),
                None => KlaApiClient::from_env()?,
            })),
            ProviderKind::Xai => {
                if let Some(AuthSource::ApiKey(key)) = default_auth {
                    Ok(Self::Xai(OpenAiCompatClient::new(key, OpenAiCompatConfig::xai())))
                } else {
                    Ok(Self::Xai(OpenAiCompatClient::from_env(
                        OpenAiCompatConfig::xai(),
                    )?))
                }
            },
            ProviderKind::OpenAi => {
                let metadata = providers::metadata_for_model(&resolved_model);
                let config = if let Some(ref m) = metadata {
                    OpenAiCompatConfig {
                        provider_name: if m.auth_env.contains("GEMINI") { "Gemini" } else { "OpenAI" },
                        api_key_env: m.auth_env,
                        base_url_env: m.base_url_env,
                        default_base_url: m.default_base_url,
                    }
                } else {
                    OpenAiCompatConfig::openai()
                };

                if let Some(auth) = default_auth {
                    match auth {
                        AuthSource::ApiKey(key) | AuthSource::BearerToken(key) => {
                            Ok(Self::OpenAi(OpenAiCompatClient::new(key, config)))
                        },
                        AuthSource::ApiKeyAndBearer { api_key, .. } => {
                            Ok(Self::OpenAi(OpenAiCompatClient::new(api_key, config)))
                        },
                        AuthSource::None => {
                            // If we have a topology, we might not need this client to be valid.
                            // But for now, let's try to load from env as fallback if possible.
                            Ok(Self::OpenAi(OpenAiCompatClient::from_env(config).unwrap_or_else(|_| {
                                OpenAiCompatClient::new("dummy-key-for-topology-fallback", config)
                            })))
                        }
                    }
                } else {
                    Ok(Self::OpenAi(OpenAiCompatClient::from_env(config)?))
                }
            }
        }
    }

    #[must_use]
    pub const fn provider_kind(&self) -> ProviderKind {
        match self {
            Self::KlaApi(_) => ProviderKind::KlaApi,
            Self::Xai(_) => ProviderKind::Xai,
            Self::OpenAi(_) => ProviderKind::OpenAi,
        }
    }

    #[must_use]
    pub fn with_base_url(self, base_url: impl Into<String>) -> Self {
        let url = base_url.into();
        match self {
            Self::KlaApi(client) => Self::KlaApi(client.with_base_url(url)),
            Self::Xai(client) => Self::Xai(client.with_base_url(url)),
            Self::OpenAi(client) => Self::OpenAi(client.with_base_url(url)),
        }
    }
}

impl crate::router::InferenceProvider for ProviderClient {
    fn stream_inference<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<StreamEvent>, ApiError>> + Send + 'a>> {
        Box::pin(async move {
            let mut stream = self.stream_message(request).await?;
            let mut events = Vec::new();
            while let Some(event) = stream.next_event().await? {
                events.push(event);
            }
            Ok(events)
        })
    }

    fn provider_label(&self) -> &str {
        match self {
            Self::KlaApi(_) => "kla_api",
            Self::Xai(_) => "xai",
            Self::OpenAi(_) => "openai",
        }
    }
}

impl ProviderClient {
    pub async fn send_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageResponse, ApiError> {
        match self {
            Self::KlaApi(client) => send_via_provider(client, request).await,
            Self::Xai(client) | Self::OpenAi(client) => send_via_provider(client, request).await,
        }
    }

    pub async fn stream_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageStream, ApiError> {
        match self {
            Self::KlaApi(client) => stream_via_provider(client, request)
                .await
                .map(MessageStream::KlaApi),
            Self::Xai(client) | Self::OpenAi(client) => stream_via_provider(client, request)
                .await
                .map(MessageStream::OpenAiCompat),
        }
    }
}

#[derive(Debug)]
pub enum MessageStream {
    KlaApi(kla_provider::MessageStream),
    OpenAiCompat(openai_compat::MessageStream),
}

impl MessageStream {
    #[must_use]
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::KlaApi(stream) => stream.request_id(),
            Self::OpenAiCompat(stream) => stream.request_id(),
        }
    }

    pub async fn next_event(&mut self) -> Result<Option<StreamEvent>, ApiError> {
        match self {
            Self::KlaApi(stream) => stream.next_event().await,
            Self::OpenAiCompat(stream) => stream.next_event().await,
        }
    }
}

pub use kla_provider::{
    oauth_token_is_expired, resolve_saved_oauth_token, resolve_startup_auth_source, OAuthTokenSet,
};
#[must_use]
pub fn read_base_url() -> String {
    kla_provider::read_base_url()
}

#[must_use]
pub fn read_xai_base_url() -> String {
    openai_compat::read_base_url(OpenAiCompatConfig::xai())
}

#[cfg(test)]
mod tests {
    use crate::providers::{detect_provider_kind, resolve_model_alias, ProviderKind};

    #[test]
    fn resolves_existing_and_grok_aliases() {
        assert_eq!(resolve_model_alias("opus"), "claude-opus-4-6");
        assert_eq!(resolve_model_alias("grok"), "grok-3");
        assert_eq!(resolve_model_alias("grok-mini"), "grok-3-mini");
    }

    #[test]
    fn provider_detection_prefers_model_family() {
        assert_eq!(detect_provider_kind("grok-3"), ProviderKind::Xai);
        assert_eq!(
            detect_provider_kind("claude-sonnet-4-6"),
            ProviderKind::KlaApi
        );
    }
}
