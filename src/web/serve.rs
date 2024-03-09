use axum::Router;
use std::{
    net::{Ipv4Addr, SocketAddr},
    path::Path,
};
use tokio::net::TcpListener;

#[cfg(unix)]
use std::os::unix::fs::FileTypeExt;

#[cfg(unix)]
pub(crate) async fn serve_unix_listener(app: Router, socket_path_string: &str) {
    let socket_path = Path::new(socket_path_string);
    let socket_directory_path = socket_path.parent().unwrap();

    if socket_directory_path.is_dir() {
        if let Ok(metadata) = std::fs::metadata(socket_path) {
            if metadata.file_type().is_socket() {
                std::fs::remove_file(socket_path).ok();
            } else {
                panic!("A file already exists at the UNIX socket path and is not a socket.");
            }
        }
    } else {
        std::fs::create_dir_all(socket_directory_path).unwrap();
    }

    let listener = tokio::net::UnixListener::bind(socket_path).unwrap();
    println!("Running server on UNIX socket {socket_path_string}...");

    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

pub(crate) async fn serve_tcp_listener(app: Router, port_string: &str) {
    let port: u16 = port_string.parse().expect("PORT is not a valid number.");
    let addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port);
    let listener = TcpListener::bind(&addr).await.unwrap();

    println!("Running server on TCP port {port}...");

    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}
