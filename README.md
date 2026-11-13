# Kurye Takip — Railway / Rust

Bağımsız yeniden geliştirme. Orijinal `ozgursolakoglu/kurye-takip` ve Supabase verileri değiştirilmez.

## Mimari
Rust (Axum) API + PostgreSQL (Railway) + mevcut HTML/CSS/JavaScript arayüzü.

## İş kuralları
- Çoklu yönetici, ayrı hesaplar.
- Onaylayan yönetici kimliği ve zaman damgası.
- Onaylı fişin gerekçeli iptali (yalnızca yönetici).
- İptal edilmiş fişin yönetici tarafından yeniden etkinleştirilmesi; yeniden onay gerektirir.
- Değiştirilemez olay geçmişi; kayıtlar fiziksel olarak silinmez.

Bu depo başlangıç iskeletidir; henüz üretim kullanımına hazır değildir.

## Giriş yöntemi
Kurye ve yöneticiler aynı parola ile kayıtlı e-posta adresi **veya** E.164 biçimindeki telefon numarası (ör. +905xxxxxxxxx) üzerinden giriş yapabilecek. Telefonla SMS doğrulaması bu aşamada kapsam dışıdır. Kimlik doğrulama API'si henüz uygulanmamıştır.
