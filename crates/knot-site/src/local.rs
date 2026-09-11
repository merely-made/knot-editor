//! Process ownership around the shared wire servers. A private runtime owns
//! every accepted task, so closing the handle also cancels in-flight clients.
use crate::{Publication, SiteFormat, server::ScrollServer};
use std::{
    net::SocketAddr,
    sync::{Arc, RwLock},
    thread,
};

pub struct LocalServer {
    backend: Backend,
    pub certificate_pem: String,
}

enum Backend {
    Scroll(ScrollServer),
    Native {
        address: SocketAddr,
        format: SiteFormat,
        publication: Arc<RwLock<Publication>>,
        shutdown: Option<tokio::sync::oneshot::Sender<()>>,
        worker: Option<thread::JoinHandle<()>>,
    },
}

impl LocalServer {
    pub fn start(publication: Publication, port: u16) -> Result<Self, String> {
        let format = publication.format;
        if format == SiteFormat::Scroll {
            let server = ScrollServer::start(publication, port)?;
            return Ok(Self {
                certificate_pem: server.certificate_pem.clone(),
                backend: Backend::Scroll(server),
            });
        }
        if format == SiteFormat::Micron {
            return Err(
                "Micron source editing is available; a NomadNet serving adapter is not installed"
                    .into(),
            );
        }
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
            .map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let (acceptor, certificate_pem) = if format == SiteFormat::Gemini {
            let cert =
                rcgen::generate_simple_self_signed(vec!["localhost".into(), "127.0.0.1".into()])
                    .map_err(|e| e.to_string())?;
            let pem = cert.cert.pem();
            let key = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
            (
                Some(
                    gemini_protocol::server::acceptor(vec![cert.cert.der().clone()], key.into())
                        .map_err(|e| e.to_string())?,
                ),
                pem,
            )
        } else {
            (None, String::new())
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let publication = Arc::new(RwLock::new(publication));
        let shared = Arc::clone(&publication);
        let (shutdown, stopped) = tokio::sync::oneshot::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            runtime.block_on(async move {
                let listener = match tokio::net::TcpListener::from_std(listener) {
                    Ok(listener) => listener,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    },
                };
                let _ = ready_tx.send(Ok(()));
                if let Some(acceptor) = acceptor {
                    let handler = move |request: gemini_protocol::server::Request| {
                        let reply = shared
                            .read()
                            .unwrap()
                            .gemini_reply(&request.url, address.port());
                        async move { reply }
                    };
                    let config = gemini_protocol::server::ServerConfig {
                        timeout: std::time::Duration::from_secs(2),
                        ..Default::default()
                    };
                    let _ = gemini_protocol::server::serve(
                        listener,
                        acceptor,
                        handler,
                        config,
                        async {
                            let _ = stopped.await;
                        },
                    )
                    .await;
                } else {
                    let handler = move |request: spartan_protocol::Request| {
                        let reply = shared.read().unwrap().spartan_reply(&request);
                        async move { reply }
                    };
                    let config = spartan_protocol::ServerConfig {
                        timeout: std::time::Duration::from_secs(2),
                        ..Default::default()
                    };
                    let _ = spartan_protocol::serve(listener, handler, config, async {
                        let _ = stopped.await;
                    })
                    .await;
                }
            });
            // Dropping this dedicated runtime cancels all accepted connections.
        });
        ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self {
            certificate_pem,
            backend: Backend::Native {
                address,
                format,
                publication,
                shutdown: Some(shutdown),
                worker: Some(worker),
            },
        })
    }

    pub fn address(&self) -> SocketAddr {
        match &self.backend {
            Backend::Scroll(server) => server.address(),
            Backend::Native { address, .. } => *address,
        }
    }

    pub fn url(&self) -> String {
        match &self.backend {
            Backend::Scroll(server) => server.url(),
            Backend::Native {
                address, format, ..
            } => format!(
                "{}://localhost:{}/",
                if *format == SiteFormat::Gemini {
                    "gemini"
                } else {
                    "spartan"
                },
                address.port()
            ),
        }
    }

    pub fn replace(&self, next: Publication) -> Result<(), String> {
        match &self.backend {
            Backend::Scroll(server) if next.format == SiteFormat::Scroll => {
                server.replace(next);
                Ok(())
            },
            Backend::Native {
                format,
                publication,
                ..
            } if next.format == *format => {
                *publication.write().unwrap() = next;
                Ok(())
            },
            _ => Err("Stop serving before changing the site protocol".into()),
        }
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        if let Backend::Native {
            shutdown, worker, ..
        } = &mut self.backend
        {
            if let Some(sender) = shutdown.take() {
                let _ = sender.send(());
            }
            if let Some(worker) = worker.take() {
                let _ = worker.join();
            }
        }
    }
}
