use std::{
    cmp::Ordering,
    collections::HashMap,
    fmt::Display,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use hyper::{body, http::request, Body, Request, Response, Uri};
use rustls::{Certificate, ClientConfig, RootCertStore};
use rustls_native_certs::load_native_certs;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

#[derive(Debug)]
pub(crate) struct Timer {
    results: Vec<Result>,
    headers: Vec<(String, String)>,
    host: String,
    https: bool,
    port: u16,
    uri: Uri,
    variables: HashMap<String, Value>,
}

impl Timer {
    pub(crate) fn new(
        uri: &str,
        headers: Vec<String>,
        variables: Option<String>,
    ) -> anyhow::Result<Self> {
        let uri = Uri::from_str(uri)?;
        let https = uri.scheme_str() != Some("http");

        Ok(Self {
            results: Vec::new(),
            headers: headers
                .into_iter()
                .map(|header| {
                    let (k, v) = header.split_once(':').unwrap();
                    (k.trim().to_string(), v.trim().to_string())
                })
                .collect(),
            host: match uri.host() {
                Some(host) => host,
                None => anyhow::bail!("no host in the URI; cannot proceed"),
            }
            .to_string(),
            https,
            port: uri.port_u16().unwrap_or(if https { 443 } else { 80 }),
            uri,
            variables: serde_json::from_str(
                variables.unwrap_or_else(|| String::from("{}")).as_str(),
            )?,
        })
    }

    pub(crate) fn results(mut self) -> Vec<Result> {
        self.results.sort_by(|a, b| {
            if a.status == b.status {
                a.duration.cmp(&b.duration)
            } else if a.status == Status::Failure {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        });

        self.results
    }

    pub(crate) async fn send_query(&mut self, query: &str) -> anyhow::Result<()> {
        let request = self.create_request(GraphQLRequest {
            query,
            variables: &self.variables,
        })?;

        let (mut response, duration) = self.send_request(request).await?;
        let body = body::to_bytes(response.body_mut()).await?;
        let response: GraphQLResponse = match serde_json::from_slice(&body) {
            Ok(response) => response,
            Err(e) => {
                anyhow::bail!(
                    "error parsing response: {:?}; body {:?}; error {:?}",
                    response,
                    body,
                    e
                );
            }
        };
        let status = if response.data.is_some() {
            Status::Success
        } else if response.errors.is_some() {
            Status::Failure
        } else {
            anyhow::bail!("unknown response: {:?} {:?}", response, body);
        };

        self.results.push(Result {
            duration,
            query: query.to_string(),
            response,
            status,
        });

        Ok(())
    }

    fn add_headers_to_builder(&self, mut builder: request::Builder) -> request::Builder {
        for (k, v) in self.headers.iter() {
            builder = builder.header(k, v);
        }
        builder
    }

    fn create_builder(&self) -> request::Builder {
        let builder = Request::builder()
            .method("POST")
            .uri(&self.uri)
            .header("Host", self.host.as_str())
            .header("Content-Type", "application/json; charset=utf-8");
        self.add_headers_to_builder(builder)
    }

    fn create_request(&self, body: GraphQLRequest) -> anyhow::Result<Request<Body>> {
        Ok(self
            .create_builder()
            .body(Body::from(serde_json::to_string_pretty(&body)?))?)
    }

    async fn send_request(
        &self,
        request: Request<Body>,
    ) -> anyhow::Result<(Response<Body>, Duration)> {
        if self.https {
            self.send_request_https(request).await
        } else {
            self.send_request_http(request).await
        }
    }

    async fn send_request_http(
        &self,
        request: Request<Body>,
    ) -> anyhow::Result<(Response<Body>, Duration)> {
        let stream = TcpStream::connect((self.host.as_str(), self.port)).await?;
        let (mut sender, conn) = hyper::client::conn::handshake(stream).await?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                eprintln!("Error in connection: {}", e);
            }
        });

        let before = Instant::now();
        let response = sender.send_request(request).await?;
        let duration = Instant::now() - before;

        Ok((response, duration))
    }

    async fn send_request_https(
        &self,
        request: Request<Body>,
    ) -> anyhow::Result<(Response<Body>, Duration)> {
        let tls = TlsConnector::from(CLIENT_CONFIG.clone());

        let tcp = TcpStream::connect((self.host.as_str(), self.port)).await?;
        let stream = tls
            .connect(rustls::ServerName::try_from(self.host.as_str())?, tcp)
            .await?;
        let (mut sender, conn) = hyper::client::conn::handshake(stream).await?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                eprintln!("Error in connection: {}", e);
            }
        });

        let before = Instant::now();
        let response = sender.send_request(request).await?;
        let duration = Instant::now() - before;

        Ok((response, duration))
    }
}

#[derive(Debug)]
pub(crate) struct Result {
    pub(crate) duration: Duration,
    pub(crate) query: String,
    response: GraphQLResponse,
    pub(crate) status: Status,
}

impl Result {
    pub(crate) fn dump_response(&self) -> String {
        format!("{:?}", self.response)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum Status {
    Success,
    Failure,
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Success => write!(f, "OK"),
            Status::Failure => write!(f, "ERR"),
        }
    }
}

#[derive(Serialize, Debug)]
struct GraphQLRequest<'a> {
    query: &'a str,
    variables: &'a HashMap<String, Value>,
}

#[derive(Deserialize, Debug)]
struct GraphQLResponse {
    data: Option<Value>,
    errors: Option<Value>,
}

lazy_static::lazy_static! {
    static ref CLIENT_CONFIG: Arc<ClientConfig> = {
        let mut roots = RootCertStore::empty();
        for cert in load_native_certs().unwrap() {
            roots.add(&Certificate(cert.0)).unwrap();
        }

        let config = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(roots)
            .with_no_client_auth();

        Arc::new(config)
    };
}
