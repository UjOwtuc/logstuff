use log::{debug, error};
use native_tls::{Certificate, Identity, TlsConnector};
use rustls::{OwnedTrustAnchor, RootCertStore};
use serde_derive::{Deserialize, Serialize};
use std::{fmt, fs};

pub use rustls::ClientConfig;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Tls(native_tls::Error),
    Pem(pem::PemError),
}

impl std::error::Error for Error {}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct TlsSettings {
    pub client_cert_store: String,
    pub client_cert_password: String,
    pub ca_certs: Vec<String>,
    pub disable_system_trust: bool,
    pub accept_invalid_hostnames: bool,
}

impl Default for TlsSettings {
    fn default() -> Self {
        TlsSettings {
            client_cert_store: "".into(),
            client_cert_password: "".into(),
            ca_certs: Vec::new(),
            disable_system_trust: false,
            accept_invalid_hostnames: false,
        }
    }
}

impl TlsSettings {
    pub fn client_config(&self) -> Result<ClientConfig, Error> {
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

        self.ca_certs
            .iter()
            .try_for_each(|file| -> Result<(), Error> {
                debug!("Adding trust anchors from {}", file);
                let cert = fs::read(file)?;
                let data = pem::parse_many(cert)?;
                data.iter().for_each(|entry| {
                    root_store.add_parsable_certificates(&[entry.contents.to_vec()]);
                });
                Ok(())
            })?;

        let builder = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store);

        if self.client_cert_store.is_empty() {
            Ok(builder.with_no_client_auth())
        } else {
            error!("Client certificate authentication is not implemented yet");
            unimplemented!()
        }
    }

    pub fn connector(&self) -> Result<TlsConnector, Error> {
        let mut connector = TlsConnector::builder();
        if !self.client_cert_store.is_empty() {
            debug!("Loading client certificate from {}", self.client_cert_store);
            let der = fs::read(&self.client_cert_store)?;
            connector.identity(Identity::from_pkcs12(&der, &self.client_cert_password)?);
        }

        self.ca_certs
            .iter()
            .try_for_each(|file| -> Result<(), Error> {
                debug!("Loading trusted certificate {}", file);
                let cert = fs::read(file)?;
                let cert = Certificate::from_pem(&cert)?;
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

impl From<pem::PemError> for Error {
    fn from(error: pem::PemError) -> Self {
        Self::Pem(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
