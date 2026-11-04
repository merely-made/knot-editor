// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

fn serve_http_cookie_probe(body: &'static str) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind HTTP fixture");
    let address = listener.local_addr().expect("read HTTP fixture address");
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for seed_session in [true, false] {
            let (mut stream, _) = listener.accept().expect("accept HTTP fixture");
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).expect("read HTTP request");
            let set_cookie = if seed_session {
                "Set-Cookie: knot-browser-authority=secret; Path=/\r\n"
            } else {
                ""
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/markdown\r\n{set_cookie}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body,
            )
            .expect("write HTTP response");
            requests.push(String::from_utf8_lossy(&request[..read]).into_owned());
        }
        requests
    });
    (format!("http://{address}/note"), handle)
}

fn serve_gopher_once(body: &'static str) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind Gopher fixture");
    let address = listener.local_addr().expect("read Gopher fixture address");
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept Gopher fixture");
        let mut request = [0_u8; 256];
        let read = stream.read(&mut request).expect("read Gopher request");
        stream
            .write_all(body.as_bytes())
            .expect("write Gopher response");
        String::from_utf8_lossy(&request[..read]).into_owned()
    });
    (format!("gopher://{address}/0note"), handle)
}

#[test]
fn network_effect_fetcher_reads_anonymous_http() {
    let (url, server) = serve_http_cookie_probe("# from HTTP\n");
    let mut fetcher = NetworkEffectFetcher::new(1024).expect("start fetcher");
    fetcher
        .runtime
        .block_on(fetch::fetch_page_capped(&url, 1024))
        .expect("seed browser session cookie");
    let fetched =
        knot_editor::KnotEffectFetcher::fetch(&mut fetcher, &url).expect("fetch HTTP effect");

    assert_eq!(fetched.content_type.as_deref(), Some("text/markdown"));
    assert_eq!(fetched.body, "# from HTTP\n");
    let requests = server.join().expect("join HTTP fixture");
    assert_eq!(requests.len(), 2);
    assert!(requests[1].starts_with("GET /note HTTP/"));
    assert!(
        !requests[1].to_ascii_lowercase().contains("\r\ncookie:"),
        "effect fetch must not borrow browser cookies: {}",
        requests[1],
    );
}

#[test]
fn network_effect_fetcher_reads_gopher_as_plain_text() {
    let (url, server) = serve_gopher_once("from Gopher\r\n");
    let mut fetcher = NetworkEffectFetcher::new(1024).expect("start fetcher");
    let fetched =
        knot_editor::KnotEffectFetcher::fetch(&mut fetcher, &url).expect("fetch Gopher effect");

    assert_eq!(fetched.content_type.as_deref(), Some("text/plain"));
    assert_eq!(fetched.body, "from Gopher\r\n");
    assert_eq!(server.join().expect("join Gopher fixture"), "note\r\n");
}

#[test]
fn effect_cache_identity_binds_the_fetch_byte_cap() {
    let small = RoutedEffectFetcher {
        file: None,
        network: Some(NetworkEffectFetcher::new(1024).expect("start small fetcher")),
    };
    let large = RoutedEffectFetcher {
        file: None,
        network: Some(NetworkEffectFetcher::new(2048).expect("start large fetcher")),
    };

    assert_ne!(
        knot_editor::KnotEffectFetcher::cache_version(&small),
        knot_editor::KnotEffectFetcher::cache_version(&large)
    );
}

#[test]
#[should_panic(expected = "Titan is an upload protocol")]
fn effect_authority_rejects_titan_read_grants() {
    effect_authority(
        OsStr::new("ask"),
        OsStr::new("never"),
        OsStr::new("titan"),
        OsStr::new(""),
        OsStr::new("1"),
        OsStr::new("1"),
        OsStr::new("1024"),
        None,
    );
}
