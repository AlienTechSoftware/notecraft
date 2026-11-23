---
title: Sağdan sola
---

# Sağdan sola

Arapça, İbranice, Farsça ve Urduca sağdan okunur. NoteCraft bunları o şekilde çizer: harfler birleşir, bu
satırlar sağ kenardan başlar ve iki yönü birden içeren bir satır Unicode algoritmasıyla düzenlenir; böylece
bir fiyatın sayısı doğru yönde kalır:

السعر 123 ريال

Bunu mümkün kılan yazı tipleri uygulamanın içinde gelir; bu yüzden yukarıdaki satır, Arapça yazı tipi
kurulu olmayan bir bilgisayarda da aynı görünür. İbranice ve Hintçenin de kendi gömülü yazı tipleri vardır:

שלום עולם

नमस्ते दुनिया

## Üç yön, üç denetim

| Denetim | Nerede | Ne ayarlar |
|---|---|---|
| **Sayfa** | **Giriş**, sonra **Yön** | Tüm sayfanın hangi yönde okunacağı. Kendi harfleri olmayan satır buna uyar |
| **Paragraf** | **Temel Metin** içindeki hizalama menüsü | İmlecin olduğu paragrafın hangi yönde okunacağı |
| **Çalıştır** | Yön grubundaki **Daha fazla...** | Seçimin ya da bir sonraki yazacağınızın hangi yönde olacağı |

Sayfa yönü NoteCraft'ın kendi ayarıdır; insanların geldiği uygulamalarda böyle bir ayar yoktur ve Arapça
yazılmış bir defterin buna ihtiyacı vardır.

## Ayarın saklandığı yer

Metnin kendisinde. Bir sayfa yönü, bir Markdown sayfasında ön bilgideki `dir: rtl`, bir HTML sayfasında ise
bir meta etiketidir. Bir paragraf yönü, başındaki görünmez bir karakterdir: Unicode'un sağdan sola ya da soldan
sağa işareti. İkisi de başka her düzenleyicide sağlam kalır.

Bu defterdeki Arapça, Farsça, Urduca ve İbranice grupları baştan sona sağdan sola yazılmıştır.
[Başlangıç sayfasından](../../../00-start/welcome/) birini açıp karşılaştırın.

*Bu sayfa bir yapay zekâ modeli tarafından çevrildi. Düzeltmeler memnuniyetle karşılanır.*
