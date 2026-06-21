use std::fs;
use std::io::{self, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::core::auth::basic_auth_matches;
use crate::core::config::EffectiveConfig;
use crate::core::dav;
use crate::core::events::{EventBus, ServerEvent};
use crate::core::fs_backend::FileSystemBackend;
use crate::core::http::{Request, Response, read_request, write_response};
use crate::core::network::display_host;

const WRITE_METHODS: &[&str] = &["PUT", "DELETE", "MKCOL", "COPY", "MOVE"];

#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub folder: String,
    pub bind_host: String,
    pub display_host: String,
    pub port: u16,
    pub name: String,
    pub read_only: bool,
    pub auth_enabled: bool,
    pub username: String,
    pub password: Option<String>,
    pub tui_enabled: bool,
}

pub struct DavServer {
    config: Arc<EffectiveConfig>,
    backend: Arc<FileSystemBackend>,
    listener: Option<TcpListener>,
    addr: Option<SocketAddr>,
    shutdown: Arc<AtomicBool>,
    events: Arc<Mutex<EventBus>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl DavServer {
    pub fn new(config: EffectiveConfig) -> io::Result<Self> {
        let backend = FileSystemBackend::new(config.folder.clone(), &config.server);
        backend.assert_root()?;
        Ok(Self {
            config: Arc::new(config),
            backend: Arc::new(backend),
            listener: None,
            addr: None,
            shutdown: Arc::new(AtomicBool::new(false)),
            events: Arc::new(Mutex::new(EventBus::new())),
            handle: None,
        })
    }

    pub fn subscribe(&mut self) -> Receiver<ServerEvent> {
        self.events.lock().expect("event bus poisoned").subscribe()
    }

    pub fn start(&mut self) -> io::Result<()> {
        let listener =
            TcpListener::bind((self.config.server.host.as_str(), self.config.server.port))?;
        listener.set_nonblocking(true)?;
        self.addr = Some(listener.local_addr()?);
        self.listener = Some(listener.try_clone()?);

        let config = Arc::clone(&self.config);
        let backend = Arc::clone(&self.backend);
        let shutdown = Arc::clone(&self.shutdown);
        let events = Arc::clone(&self.events);
        self.handle = Some(thread::spawn(move || {
            while !shutdown.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, peer)) => {
                        let config = Arc::clone(&config);
                        let backend = Arc::clone(&backend);
                        let events = Arc::clone(&events);
                        emit(
                            &events,
                            ServerEvent::ClientConnected {
                                peer: peer.to_string(),
                            },
                        );
                        thread::spawn(move || {
                            let _ = handle_connection(stream, &config, &backend, &events);
                        });
                    }
                    Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20))
                    }
                    Err(_) => break,
                }
            }
        }));
        Ok(())
    }

    pub fn stop(&mut self) -> io::Result<()> {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(addr) = self.addr {
            let _ = TcpStream::connect(addr);
        }
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .map_err(|_| io::Error::other("Server thread panicked"))?;
        }
        emit(&self.events, ServerEvent::ServerStopped);
        Ok(())
    }

    pub fn info(&self) -> ServerInfo {
        let addr = self
            .addr
            .expect("server must be started before info is requested");
        ServerInfo {
            folder: self.config.folder.display().to_string(),
            bind_host: self.config.server.host.clone(),
            display_host: display_host(&self.config.server.host),
            port: addr.port(),
            name: self.config.server.name.clone(),
            read_only: self.config.server.read_only,
            auth_enabled: self.config.auth.enabled,
            username: self.config.auth.username.clone(),
            password: self.config.auth.password.clone(),
            tui_enabled: self.config.tui.enabled,
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    config: &EffectiveConfig,
    backend: &FileSystemBackend,
    events: &Arc<Mutex<EventBus>>,
) -> io::Result<()> {
    let Some(request) = read_request(&mut stream)? else {
        return Ok(());
    };
    let started = Instant::now();
    let bytes_in = request.body.len() as u64;
    let method = request.method.clone();
    let path = request.path.clone();
    let response = route(config, backend, &request).unwrap_or_else(error_response);
    let status = response.status;
    let bytes_out = response.body.len() as u64;
    let result = write_response(&mut stream, &method, response);
    emit(
        events,
        ServerEvent::RequestCompleted {
            method,
            path,
            status,
            bytes_in,
            bytes_out,
            duration: started.elapsed(),
        },
    );
    result
}

fn route(
    config: &EffectiveConfig,
    backend: &FileSystemBackend,
    request: &Request,
) -> io::Result<Response> {
    if !basic_auth_matches(
        request.headers.get("authorization").map(String::as_str),
        &config.auth,
    ) {
        return Ok(
            Response::text(401, "Unauthorized", "Authentication required")
                .with_header("WWW-Authenticate", "Basic realm=\"davbox\""),
        );
    }
    if config.server.read_only && WRITE_METHODS.contains(&request.method.as_str()) {
        return Ok(Response::text(403, "Forbidden", "Read-only share"));
    }

    match request.method.as_str() {
        "OPTIONS" => Ok(dav::options()),
        "PROPFIND" => dav::propfind(backend, request),
        "GET" | "HEAD" => dav::get_or_head(backend, request),
        "PUT" => put(backend, request),
        "MKCOL" => mkcol(backend, request),
        "DELETE" => delete(backend, request),
        "COPY" => copy_or_move(backend, request, false),
        "MOVE" => copy_or_move(backend, request, true),
        _ => Ok(Response::text(
            405,
            "Method Not Allowed",
            "Method not allowed",
        )),
    }
}

fn emit(events: &Arc<Mutex<EventBus>>, event: ServerEvent) {
    if let Ok(mut events) = events.lock() {
        events.emit(event);
    }
}

fn put(backend: &FileSystemBackend, request: &Request) -> io::Result<Response> {
    let path = backend.resolve(&request.path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::File::create(path)?.write_all(&request.body)?;
    Ok(Response::new(201, "Created"))
}

fn mkcol(backend: &FileSystemBackend, request: &Request) -> io::Result<Response> {
    fs::create_dir(backend.resolve(&request.path)?)?;
    Ok(Response::new(201, "Created"))
}

fn delete(backend: &FileSystemBackend, request: &Request) -> io::Result<Response> {
    backend.remove(&request.path)?;
    Ok(Response::new(204, "No Content"))
}

fn copy_or_move(
    backend: &FileSystemBackend,
    request: &Request,
    move_file: bool,
) -> io::Result<Response> {
    let destination = request
        .headers
        .get("destination")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Missing Destination header"))?;
    let destination_path = destination
        .split_once("://")
        .and_then(|(_, rest)| rest.split_once('/').map(|(_, path)| format!("/{path}")))
        .unwrap_or_else(|| destination.to_string());
    let from = backend.resolve(&request.path)?;
    let to = backend.resolve(&destination_path)?;
    if move_file {
        fs::rename(from, to)?;
        Ok(Response::new(204, "No Content"))
    } else {
        copy_recursive(&from, &to)?;
        Ok(Response::new(201, "Created"))
    }
}

fn copy_recursive(from: &std::path::Path, to: &std::path::Path) -> io::Result<()> {
    let metadata = fs::metadata(from)?;
    if metadata.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)?;
    }
    Ok(())
}

fn error_response(err: io::Error) -> Response {
    match err.kind() {
        io::ErrorKind::NotFound => Response::text(404, "Not Found", "Not found"),
        io::ErrorKind::PermissionDenied => Response::text(403, "Forbidden", err.to_string()),
        io::ErrorKind::InvalidInput => Response::text(400, "Bad Request", err.to_string()),
        io::ErrorKind::AlreadyExists => Response::text(409, "Conflict", err.to_string()),
        _ => Response::text(500, "Internal Server Error", "Internal server error"),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    use crate::cli::ServeArgs;
    use crate::core::config::{Config, EffectiveConfig};
    use crate::core::events::ServerEvent;

    use super::DavServer;

    #[test]
    fn serves_file_over_http() {
        let root = std::env::temp_dir().join(format!("davbox-http-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("hello.txt"), "hello world").unwrap();

        let args = ServeArgs {
            target: root.display().to_string(),
            port: Some(0),
            no_auth: true,
            tui: Some(false),
            ..ServeArgs::default()
        };
        let config = EffectiveConfig::from_inputs(Config::default(), args, &[]).unwrap();
        let mut server = DavServer::new(config).unwrap();
        let events = server.subscribe();
        server.start().unwrap();
        let info = server.info();

        let mut stream = TcpStream::connect(("127.0.0.1", info.port)).unwrap();
        stream
            .write_all(b"GET /hello.txt HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with("hello world"));

        let mut saw_completed = false;
        for _ in 0..3 {
            if let Ok(ServerEvent::RequestCompleted {
                method,
                path,
                status,
                bytes_out,
                ..
            }) = events.recv_timeout(Duration::from_secs(1))
            {
                assert_eq!(method, "GET");
                assert_eq!(path, "/hello.txt");
                assert_eq!(status, 200);
                assert_eq!(bytes_out, 11);
                saw_completed = true;
                break;
            }
        }
        assert!(saw_completed);

        server.stop().unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_only_rejects_put() {
        let root =
            std::env::temp_dir().join(format!("davbox-readonly-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();

        let args = ServeArgs {
            target: root.display().to_string(),
            port: Some(0),
            no_auth: true,
            read_only: Some(true),
            tui: Some(false),
            ..ServeArgs::default()
        };
        let config = EffectiveConfig::from_inputs(Config::default(), args, &[]).unwrap();
        let mut server = DavServer::new(config).unwrap();
        server.start().unwrap();
        let info = server.info();

        let mut stream = TcpStream::connect(("127.0.0.1", info.port)).unwrap();
        stream
            .write_all(b"PUT /new.txt HTTP/1.1\r\nHost: localhost\r\nContent-Length: 3\r\n\r\nnew")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 403 Forbidden"));

        server.stop().unwrap();
        let _ = fs::remove_dir_all(root);
    }
}
