---
title: Eşitleme ve yedekleme
---

# Eşitleme ve yedekleme

NoteCraft'ın kendi eşitlemesi ve hesabı yoktur. Bir defter, küçük dosyalardan oluşan bir klasördür; bu
yüzden defter klasörünüzü eşitleme aracınızın zaten baktığı yere koyun: OneDrive, Dropbox, Google Drive,
iCloud Drive, Syncthing, bir ağ paylaşımı ya da bir git deposu. Hiçbirinin NoteCraft'ı anlaması gerekmez.
Dosyaları kopyalarlar.

## Neden çalışır

- **Sayfa başına bir dosya.** Bir sayfayı düzenlemek küçük bir dosyayı yeniden yazar; bu yüzden iki
  bilgisayar nadiren aynı baytlara dokunur.
- **Bölünmez yazmalar.** Bir sayfa geçici bir dosyaya yazılır ve eskisinin üzerine adlandırılır; böylece bir
  eşitleme istemcisi yarım yazılmış bir sayfa görmez.
- **Kısa ve güvenli adlar.** Klasör adları en çok 64, yollar en çok 255 karakter olur ve Windows'un
  kabul etmediği hiçbir şey yazılmaz.
- **Çakışma kopyaları fark edilir.** Bir eşitleme aracı bir sayfanın yanında çakışmalı bir kopya bırakırsa,
  denetleyici bunu bildirir: **Dosya**, sonra **Denetle**, ya da bir terminalden `notecraft-cli check`.

## Yedekleme

Yedek, klasörün bir kopyasıdır. Dışa aktarılacak başka bir şey yoktur.

## Sayfa geçmişi

Bu sayfanın kaydedilmiş kopyalarını gözden geçirmek ve geri yüklemek için **Geçmiş**, sonra **Sayfa
Sürümleri**'ni açın. Silinmiş bir sayfayı, bölümü ya da bölüm grubunu kurtarmak için **Geçmiş**, sonra
**Defter Geri Dönüşüm Kutusu**'nu açın. Bu araçlar bu aygıtta çalışır ve defterin ayrı bir yedeğinin yerini
tutmaz.

## Ele alınmayan şeyler

NoteCraft bir sayfanın iki sürümünü birleştirmez. Eşitleme aracınız bir çakışma kopyası oluşturursa, hangi
sürümü tutacağınıza siz karar verirsiniz.

Sonraki: [Size göre yapın](../make-it-yours/).

*Bu sayfa bir yapay zekâ modeli tarafından çevrildi. Düzeltmeler memnuniyetle karşılanır.*
