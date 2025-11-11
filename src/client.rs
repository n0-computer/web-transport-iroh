use std::sync::Arc;
use std::time::Duration;

use iroh::endpoint::ConnectOptions;
use iroh::{EndpointAddr, EndpointId, SecretKey};
use quinn::TransportConfig;
use url::Url;

use crate::{ClientError, Session, ALPN};

// Copies the Web options, hiding the actual implementation.
/// Allows specifying a class of congestion control algorithm.
pub enum CongestionControl {
    Default,
    Throughput,
    LowLatency,
}

/// Construct a WebTransport [Client] using sane defaults.
///
/// This is optional; advanced users may use [Client::new] directly.
pub struct ClientBuilder {
    builder: iroh::endpoint::Builder,
    congestion_controller:
        Option<Arc<dyn quinn::congestion::ControllerFactory + Send + Sync + 'static>>,
}

impl ClientBuilder {
    /// Create a Client builder, which can be used to establish multiple [Session]s.
    pub fn new() -> Self {
        Self {
            builder: iroh::Endpoint::builder(),
            congestion_controller: None,
        }
    }

    /// For compatibility with WASM. Panics if `val` is false, but does nothing else.
    pub fn with_unreliable(self, val: bool) -> Self {
        if !val {
            panic!("with_unreliable must be true for quic transport");
        }

        self
    }

    /// Enable the specified congestion controller.
    pub fn with_congestion_control(mut self, algorithm: CongestionControl) -> Self {
        self.congestion_controller = match algorithm {
            CongestionControl::LowLatency => {
                Some(Arc::new(quinn::congestion::BbrConfig::default()))
            }
            // TODO BBR is also higher throughput in theory.
            CongestionControl::Throughput => {
                Some(Arc::new(quinn::congestion::CubicConfig::default()))
            }
            CongestionControl::Default => None,
        };

        self
    }

    /// Accept any certificate from the server if it uses a known root CA.
    pub async fn with_secret_key(mut self, secret_key: SecretKey) -> Result<Client, ClientError> {
        self.builder = self.builder.secret_key(secret_key);
        self.build().await
    }

    pub async fn build(self) -> Result<Client, ClientError> {
        let endpoint = self
            .builder
            .alpns(vec![crate::ALPN.as_bytes().to_vec()])
            .bind()
            .await
            .map_err(|err| ClientError::Bind(Arc::new(err)))?;

        let mut transport_config = quinn::TransportConfig::default();
        transport_config.keep_alive_interval(Some(Duration::from_secs(1)));
        if let Some(cc) = &self.congestion_controller {
            transport_config.congestion_controller_factory(cc.clone());
        }

        Ok(Client {
            endpoint,
            config: Arc::new(transport_config),
        })
    }
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A client for connecting to a WebTransport server.
pub struct Client {
    endpoint: iroh::Endpoint,
    config: Arc<TransportConfig>,
}

impl Client {
    /// Manually create a client via a Quinn endpoint and config.
    ///
    /// The ALPN MUST be set to [ALPN].
    pub fn new(endpoint: iroh::Endpoint, config: Arc<quinn::TransportConfig>) -> Self {
        Self { endpoint, config }
    }

    /// Connect to the server.
    pub async fn connect_url(&self, url: Url) -> Result<Session, ClientError> {
        let endpoint_id: EndpointId = url.host_str().unwrap().parse().unwrap();
        self.connect_endpoint_id(endpoint_id).await
    }

    pub async fn connect_endpoint_id(
        &self,
        addr: impl Into<EndpointAddr>,
    ) -> Result<Session, ClientError> {
        let addr = addr.into();
        let url: Url = format!("iroh://{}", addr.id).parse().unwrap();
        // Connect to the server using the addr we just resolved.
        let opts = ConnectOptions::new().with_transport_config(self.config.clone());
        let conn = self
            .endpoint
            .connect_with_opts(addr, ALPN.as_bytes(), opts)
            .await
            .map_err(Arc::new)?;
        let conn = conn.await.map_err(Arc::new)?;

        // Connect with the connection we established.
        Ok(Session::raw(conn, url))
    }
}
