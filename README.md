# Pencere DeX (Rust Sürümü)

Telefonda DeX benzeri tek ve çoklu pencere kontrolü (Windows PC, yerel ağ) — Yüksek performanslı ve düşük bellek tüketimli Rust sürümü.

---

## 🚀 Proje Yapısı

```
Pencere/
├── Cargo.toml                  # Rust bağımlılıkları ve derleme ayarları
├── .gitignore                  # Git yoksayma listesi
├── .github/
│   └── workflows/
│       └── build.yml           # GitHub Actions otomatik derleme iş akışı
├── static/
│   └── index.html              # WebGL ve dokunmatik istemci arayüzü
└── src/
    ├── main.rs                 # Giriş noktası ve CLI banner
    ├── win32.rs                # Win32 pencere listesi, DPI, ikon ve odak yönetimi
    ├── capture.rs              # GDI / BitBlt / PrintWindow ekran yakalama
    ├── diff.rs                 # 64x64 karo tabanlı bölgesel fark algılama
    ├── encoder.rs              # JPEG / WebP / PNG görüntü kodlama
    ├── input.rs                # Fare, kaydırma, dokunma ve Unicode klavye simülasyonu
    ├── clipboard.rs            # Windows pano metin ve dosya aktarımı
    ├── tls.rs                  # Kendinden imzalı HTTPS / TLS sertifikası oluşturucu
    └── server.rs               # Axum HTTP & WebSocket sunucusu
```

---

## 🛠️ GitHub Üzerinde Derleme (GitHub Actions)

Projede hazır bir GitHub Actions yapılandırması (`.github/workflows/build.yml`) bulunmaktadır:

1. Bu klasördeki dosyaları GitHub deponuza yükleyin (push edin):
   ```bash
   git init
   git add .
   git commit -m "Rust sürümü hazırlandı"
   git branch -M main
   git remote add origin https://github.com/KULLANICI_ADINIZ/Pencere.git
   git push -u origin main
   ```
2. GitHub deponuzdaki **Actions** sekmesine gidin.
3. Otomatik başlayan **"Rust Windows Derleme"** iş akışını göreceksiniz.
4. İş akışı tamamlandığında sayfanın altındaki **Artifacts** bölümünden `pencere_dex-windows-x86_64` zip dosyasını indirip içindeki `pencere_dex.exe` dosyasını doğrudan çalıştırabilirsiniz.

---

## 💻 Yerel Bilgisayarda Derleme (Opsiyonel)

Eğer bilgisayarınızda Rust kuruluysa:
```bash
# Debug modunda çalıştırma
cargo run

# Optimize edilmiş Release sürümü oluşturma
cargo build --release
# Çıktı: target/release/pencere_dex.exe
```
