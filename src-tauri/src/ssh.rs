use russh::client;
use russh_keys::key;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub static PREFERRED_HOST_KEY_ALGOS: &[russh_keys::key::Name] = &[
    russh_keys::key::ED25519,
    russh_keys::key::ECDSA_SHA2_NISTP256,
    russh_keys::key::ECDSA_SHA2_NISTP521,
    russh_keys::key::RSA_SHA2_256,
    russh_keys::key::RSA_SHA2_512,
    russh_keys::key::SSH_RSA,
];

pub struct Client {
    expected_host_key: Option<String>,
    observed_host_key: Option<Arc<Mutex<Option<String>>>>,
}

impl Client {
    pub fn require(expected_host_key: Option<String>) -> Self {
        Self { expected_host_key, observed_host_key: None }
    }

    fn observe(observed_host_key: Arc<Mutex<Option<String>>>) -> Self {
        Self { expected_host_key: None, observed_host_key: Some(observed_host_key) }
    }
}

pub async fn probe_host_key(host: &str, port: u16) -> Result<String, String> {
    let observed = Arc::new(Mutex::new(None));
    let configuration = client::Config {
        preferred: russh::Preferred {
            key: std::borrow::Cow::Borrowed(PREFERRED_HOST_KEY_ALGOS),
            ..russh::Preferred::DEFAULT
        },
        ..client::Config::default()
    };
    let _ = tokio::time::timeout(
        Duration::from_secs(10),
        client::connect(Arc::new(configuration), (host, port), Client::observe(observed.clone())),
    )
    .await;
    let fingerprint = observed.lock().map_err(|_| "Host key probe failed.".to_string())?.clone();
    fingerprint
        .ok_or_else(|| "Could not read the server host key. Check the server address and port.".to_string())
}

#[async_trait::async_trait]
impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(&mut self, server_public_key: &key::PublicKey) -> Result<bool, Self::Error> {
        let fingerprint = server_public_key.fingerprint();
        if let Some(observed) = &self.observed_host_key {
            if let Ok(mut value) = observed.lock() {
                *value = Some(fingerprint);
            }
            return Ok(false);
        }
        Ok(self.expected_host_key.as_deref() == Some(fingerprint.as_str()))
    }
}
