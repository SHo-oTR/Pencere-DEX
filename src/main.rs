mod capture;
mod clipboard;
mod diff;
mod encoder;
mod input;
mod server;
mod tls;
mod win32;

use axum_server::tls_rustls::RustlsConfig;
use rand::distributions::Alphanumeric;
use rand::Rng;
use server::{create_router, AppState};
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicIsize};
use std::sync::{Arc, Mutex};

const PORT: u16 = 8765;

fn get_local_ip() -> IpAddr {
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(local_addr) = socket.local_addr() {
                return local_addr.ip();
            }
        }
    }
    "127.0.0.1".parse().unwrap()
}

fn generate_token(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Enable DPI awareness for accurate pixel coordinates
    win32::set_dpi_aware();

    let ip = get_local_ip();
    let key = generate_token(7);

    println!("============================================================");
    println!("        Pencere DeX (Rust Sürümü) - Windows PC");
    println!("============================================================");

    let state = AppState {
        key: key.clone(),
        cur_hwnd: Arc::new(AtomicIsize::new(0)),
        pw_mode: Arc::new(AtomicBool::new(false)),
        base_windows: Arc::new(Mutex::new(None)),
        base_fg: Arc::new(AtomicIsize::new(0)),
    };

    let app = create_router(state);
    let bind_addr = SocketAddr::from(([0, 0, 0, 0], PORT));

    // Attempt to set up HTTPS via self-signed TLS certificate
    match tls::get_or_generate_tls(ip) {
        Ok(tls_files) => {
            if let Ok(config) = RustlsConfig::from_pem(
                tls_files.cert_pem.as_bytes().to_vec(),
                tls_files.key_pem.as_bytes().to_vec(),
            )
            .await
            {
                let url = format!("https://{}:{}/?k={}", ip, PORT, key);
                println!("\nGörüntü Modu: Ultra Düşük Gecikmeli Bölgesel Karo Akışı");
                println!("Protokol: Güvenli HTTPS (TLS aktif)");
                println!("Tarayıcı ilk açılışta sertifika uyarısı verirse 'Gelişmiş > Yine de devam et' seçeneğine tıklayın.\n");
                println!("Telefonda aç: {}\n", url);
                println!("Sunucu dinleniyor: {} (Çıkış için Ctrl+C)", bind_addr);

                axum_server::bind_rustls(bind_addr, config)
                    .serve(app.into_make_service())
                    .await?;
                return Ok(());
            }
        }
        Err(e) => {
            eprintln!("TLS başlatılamadı ({}), standart HTTP moduna geçiliyor...", e);
        }
    }

    // Fallback to standard HTTP
    let url = format!("http://{}:{}/?k={}", ip, PORT, key);
    println!("\nGörüntü Modu: Ultra Düşük Gecikmeli Bölgesel Karo Akışı");
    println!("Protokol: Standart HTTP");
    println!("Telefonda aç: {}\n", url);
    println!("Sunucu dinleniyor: {} (Çıkış için Ctrl+C)", bind_addr);

    axum_server::bind(bind_addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
