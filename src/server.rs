use std::{net::SocketAddr, sync::Arc, time::Duration};

use crate::{CongestionControl, ServerError, Session};

use iroh::{endpoint, Endpoint, EndpointId};
use n0_future::{boxed::BoxFuture, StreamExt};
use quinn::TransportConfig;
use url::Url;

/// Construct a WebTransport [Server] using sane defaults.
///
/// This is optional; advanced users may use [Server::new] directly.
pub struct ServerBuilder {
    builder: endpoint::Builder,
    transport_config: TransportConfig,
}

impl Default for ServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerBuilder {
    /// Create a server builder with sane defaults.
    pub fn new() -> Self {
        let mut transport_config = iroh::endpoint::TransportConfig::default();
        transport_config.keep_alive_interval(Some(Duration::from_secs(1)));
        Self {
            builder: Endpoint::builder(),
            transport_config,
        }
    }

    /// Listen on the specified address.
    pub fn with_addr(mut self, addr: SocketAddr) -> Self {
        self.builder = match addr {
            SocketAddr::V4(addr) => self.builder.bind_addr_v4(addr),
            SocketAddr::V6(addr) => self.builder.bind_addr_v6(addr),
        };
        self
    }

    /// Enable the specified congestion controller.
    pub fn with_congestion_control(mut self, algorithm: CongestionControl) -> Self {
        match algorithm {
            CongestionControl::LowLatency => {
                let cc = Arc::new(quinn::congestion::BbrConfig::default());
                self.transport_config.congestion_controller_factory(cc);
            }
            // TODO BBR is also higher throughput in theory.
            CongestionControl::Throughput => {
                let cc = Arc::new(quinn::congestion::CubicConfig::default());
                self.transport_config.congestion_controller_factory(cc);
            }
            CongestionControl::Default => {}
        };
        self
    }

    pub async fn build(self, secret_key: iroh::SecretKey) -> Result<Server, ServerError> {
        let endpoint = self
            .builder
            .alpns(vec![crate::ALPN.as_bytes().to_vec()])
            .secret_key(secret_key)
            .bind()
            .await
            .map_err(Arc::new)?;

        Ok(Server::new(endpoint))
    }
}

/// A WebTransport server that accepts new sessions.
pub struct Server {
    endpoint: iroh::Endpoint,
    accept: n0_future::FuturesUnordered<BoxFuture<Result<Request, ServerError>>>,
}

impl Server {
    /// Creates a new server with a manually constructed [`Endpoint`].
    pub fn new(endpoint: iroh::Endpoint) -> Self {
        Self {
            endpoint,
            accept: Default::default(),
        }
    }

    pub fn endpoint_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    /// Accept a new WebTransport session Request from a client.
    pub async fn accept(&mut self) -> Option<Request> {
        loop {
            tokio::select! {
                res = self.endpoint.accept() => {
                    let conn = res?;
                    self.accept.push(Box::pin(async move {
                        let conn = conn.await.map_err(Arc::new)?;
                        Request::accept(conn).await
                    }));
                }
                Some(res) = self.accept.next() => {
                    if let Ok(session) = res {
                        return Some(session)
                    }
                }
            }
        }
    }
}

/// A mostly complete WebTransport handshake, just awaiting the server's decision on whether to accept or reject the session based on the URL.
pub struct Request {
    conn: iroh::endpoint::Connection,
    url: Url,
}

impl Request {
    /// Accept a new WebTransport session from a client.
    pub async fn accept(conn: iroh::endpoint::Connection) -> Result<Self, ServerError> {
        let url: Url = format!("iroh://{}", conn.remote_id()).parse().unwrap();
        // Return the resulting request with a reference to the settings/connect streams.
        Ok(Self { url, conn })
    }

    /// Returns the URL provided by the client.
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Accept the session, returning a 200 OK.
    pub async fn ok(self) -> Result<Session, quinn::WriteError> {
        Ok(Session::raw(self.conn, self.url))
    }

    /// Reject the session, returing your favorite HTTP status code.
    pub async fn close(self, status: http::StatusCode) -> Result<(), quinn::WriteError> {
        self.conn
            .close(status.as_u16().into(), status.as_str().as_bytes());
        Ok(())
    }
}
