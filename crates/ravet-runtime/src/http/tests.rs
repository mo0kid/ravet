// Synthetic localhost TLS identity, unrelated to any product or activation server.
use super::*;
use rustls::{
    ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    thread,
};
const CERT: &[u8] = include_bytes!("test-cert.der");
const KEY: &[u8] = include_bytes!("test-key.der");
fn agent(options: &HttpOptions) -> Agent {
    Agent::config_builder()
        .https_only(true)
        .proxy(None)
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(options.timeout))
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::new_with_certs(&[
                    ureq::tls::Certificate::from_der(CERT),
                ]))
                .build(),
        )
        .build()
        .into()
}
fn server(response: &'static [u8], delay: Duration) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "https://localhost:{}/test?query=1",
        listener.local_addr().unwrap().port()
    );
    let worker = thread::spawn(move || {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(CERT.to_vec())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(KEY.to_vec())),
            )
            .unwrap();
        let (socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut stream = StreamOwned::new(ServerConnection::new(Arc::new(config)).unwrap(), socket);
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            if stream.read_exact(&mut byte).is_err() {
                return;
            }
            request.push(byte[0]);
        }
        assert!(request.starts_with(b"POST /test?query=1 HTTP/1.1\r\n"));
        let mut body = [0; 2];
        if stream.read_exact(&mut body).is_err() {
            return;
        }
        assert_eq!(&body, b"{}");
        thread::sleep(delay);
        let _ = stream.write_all(response);
        let _ = stream.flush();
    });
    (url, worker)
}
#[test]
fn real_tls_returns_error_status_and_does_not_follow_redirect() {
    let options = HttpOptions::default();
    for (response, expected) in [
        (b"HTTP/1.1 403 Forbidden\r\nContent-Length: 2\r\nConnection: close\r\n\r\nno".as_slice(), 403),
        (b"HTTP/1.1 302 Found\r\nLocation: https://localhost:1/never\r\nContent-Length: 2\r\nConnection: close\r\n\r\nno".as_slice(), 302),
    ] {
        let (url, worker) = server(response, Duration::ZERO);
        let response = send(&agent(&options), &url, b"{}", &options).unwrap();
        assert_eq!(response.status, expected);
        assert_eq!(response.body, b"no");
        worker.join().unwrap();
    }
}
#[test]
fn streamed_response_limit_and_whole_request_timeout() {
    let options = HttpOptions {
        maximum_response_bytes: 4,
        timeout: Duration::from_secs(2),
        ..HttpOptions::default()
    };
    let (url, worker) = server(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\nabc\r\n3\r\ndef\r\n0\r\n\r\n", Duration::ZERO);
    assert_eq!(
        send(&agent(&options), &url, b"{}", &options).unwrap_err(),
        HttpError::TooLarge
    );
    worker.join().unwrap();
    let options = HttpOptions {
        timeout: Duration::from_millis(100),
        ..HttpOptions::default()
    };
    let (url, worker) = server(
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n",
        Duration::from_millis(300),
    );
    assert_eq!(
        send(&agent(&options), &url, b"{}", &options).unwrap_err(),
        HttpError::Timeout
    );
    worker.join().unwrap();
}
#[test]
fn normal_verifier_rejects_untrusted_local_certificate() {
    let (url, worker) = server(b"", Duration::ZERO);
    assert_eq!(
        post_json(&url, b"{}", &HttpOptions::default()).unwrap_err(),
        HttpError::Transport
    );
    worker.join().unwrap();
}
#[test]
fn invalid_requests_fail_before_network_activity() {
    for url in [
        "http://localhost/",
        "file:///etc/passwd",
        "https://user:password@localhost/",
        "https://",
        "https://localhost/#fragment",
    ] {
        assert_eq!(
            post_json(url, b"{}", &HttpOptions::default()).unwrap_err(),
            HttpError::InvalidRequest
        );
    }
    let options = HttpOptions {
        user_agent: "RAVET\r\nInjected: true".into(),
        ..HttpOptions::default()
    };
    assert_eq!(
        post_json("https://localhost/", b"{}", &options).unwrap_err(),
        HttpError::InvalidRequest
    );
}
