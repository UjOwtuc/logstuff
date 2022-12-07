use log::{debug, error, warn};
use native_tls::{Identity, TlsConnector};
use rustls::{Certificate, OwnedTrustAnchor, RootCertStore};
use rustls_pemfile::{read_one, Item};
use serde_derive::{Deserialize, Serialize};
use std::{fmt, fs, io, iter};

pub use rustls::{ClientConfig, ServerConfig};

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Tls(native_tls::Error),
    Rustls(rustls::Error),
}

impl std::error::Error for Error {}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct TlsSettings {
    pub private_cert: String,
    pub private_key: String,
    pub ca_certs: Vec<String>,
    pub disable_system_trust: bool,
    pub accept_invalid_hostnames: bool,
}

impl Default for TlsSettings {
    fn default() -> Self {
        TlsSettings {
            private_cert: "".into(),
            private_key: "".into(),
            ca_certs: Vec::new(),
            disable_system_trust: false,
            accept_invalid_hostnames: false,
        }
    }
}

impl TlsSettings {
    pub fn root_trust_store(&self) -> Result<RootCertStore, Error> {
        let mut root_store = RootCertStore::empty();

        if !self.disable_system_trust {
            debug!("Adding webpki trust anchors");
            root_store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(
                |ta| {
                    OwnedTrustAnchor::from_subject_spki_name_constraints(
                        ta.subject,
                        ta.spki,
                        ta.name_constraints,
                    )
                },
            ));
        }

        self.load_trusted_certs()?.into_iter().for_each(|cert| {
            root_store.add_parsable_certificates(&[cert.0]);
        });
        Ok(root_store)
    }

    pub fn load_trusted_certs(&self) -> Result<Vec<Certificate>, Error> {
        let mut result = Vec::new();
        self.ca_certs
            .iter()
            .try_for_each(|file| -> Result<(), Error> {
                debug!("Adding trust anchors from {}", file);
                let cert = fs::File::open(file)?;
                let mut reader = io::BufReader::new(cert);
                for item in iter::from_fn(|| read_one(&mut reader).transpose()) {
                    match item? {
                        Item::X509Certificate(cert) => {
                            result.push(Certificate(cert));
                        }
                        _ => {
                            warn!("Ignoring private key in trusted certificates");
                        }
                    };
                }
                Ok(())
            })?;
        Ok(result)
    }

    pub fn client_config(&self) -> Result<ClientConfig, Error> {
        let builder = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(self.root_trust_store()?);

        if self.private_cert.is_empty() {
            Ok(builder.with_no_client_auth())
        } else {
            error!("Client certificate authentication is not implemented yet");
            unimplemented!()
        }
    }

    pub fn connector(&self) -> Result<TlsConnector, Error> {
        let mut connector = TlsConnector::builder();
        if !self.private_cert.is_empty() {
            debug!(
                "Loading client certificate and key from {} and {}",
                self.private_cert, self.private_key
            );
            // TODO load PEMs
            let der = fs::read(&self.private_cert)?;
            connector.identity(Identity::from_pkcs12(&der, "")?);
        }

        self.ca_certs
            .iter()
            .try_for_each(|file| -> Result<(), Error> {
                debug!("Loading trusted certificate {}", file);
                let cert = fs::read(file)?;
                let cert = native_tls::Certificate::from_pem(&cert)?;
                connector.add_root_certificate(cert);
                Ok(())
            })?;

        connector.disable_built_in_roots(self.disable_system_trust);
        let connector = connector.build()?;
        debug!("TLS connector settings: {:?}", connector);
        Ok(connector)
    }
}

impl From<native_tls::Error> for Error {
    fn from(error: native_tls::Error) -> Self {
        Self::Tls(error)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rustls::Error> for Error {
    fn from(error: rustls::Error) -> Self {
        Self::Rustls(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
