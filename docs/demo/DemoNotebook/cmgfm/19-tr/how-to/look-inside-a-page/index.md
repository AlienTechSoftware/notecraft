---
id: 39918e726a1b5dc225aa
title: "Sayfanın içine bakın"
order: 75
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

Birçok not uygulaması bütün bir defteri tek bir ikili dosyada tutar ve bir metin düzenleyicisi
gürültüden başka bir şey göstermez. Bir NoteCraft sayfası düz dosyalardan oluşan bir klasördür ve kod
görünümü size o klasörü, dosya dosya gösterir. Şimdi deneyin: bu sayfanın yanındaki `< >` düğmesine basın.

## Ne göreceksiniz

Solda **Bu sayfadaki dosyalar**: önce sayfanın kendi dosyası, `index.md`, sonra klasördeki diğer her şey.
Sayfada hiçbir şeyin kullanmadığı bir dosya *sayfa tarafından kullanılmıyor* olarak işaretlenir. Sağda
seçtiğiniz dosya bulunur. Bu, okuduğunuz sayfa olduğu için görüntüleyici kendi metnini gösterir.

Sayfa dosyası tamamen gösterilir: **başlığı** (sayfanın kimliği, başlığı, sırası, tarihleri ve ayarları) ve
altında **gövde**. Hepsini düzenleyebilirsiniz. Görünümü kapatmak için aynı düğmeye tekrar basın; sayfa,
yazdığınız şeyden yeniden çizilir.

| Dosya | Görüntüleyicinin gösterdiği | Ona yazabilir misiniz? |
|---|---|---|
| Sayfa dosyası, `index.md` ya da `index.html` | Başlık ve gövde, kaydedildiği gibi | Hepsine |
| Bir çizim, `ink.svg` | SVG metni, kalemin kendi okumalarıyla | Evet, dikkatli olarak |
| Bir resim | Resim | Hayır |
| Diğer metinler | Metni | Hayır |
| Diğer her şey | Adı, boyutu ve türü | Hayır |

256 KB'tan büyük bir dosya, ilk kısmını gösterir ve bunu söyler.

## Size neler gösterir

- Kutulara yerleştirilmiş bir sayfada her kutunun etrafında `class="nc-box"` olan bir `div` bulunur.
- Bir çizim bir dosyadır, `ink.svg`, ve sayfanın metni onu tek bir satırla adlandırır.
- Etiketler ve bağlantılar düz metindir.

Hiçbir şey gizli değildir: sayfa bir şey gösteriyorsa, bu klasördeki bir dosya onu tutar.

Sonraki: [Menüler ve gezinme](../menus-and-moving-around/).

*Bu sayfa bir yapay zekâ modeli tarafından çevrildi. Düzeltmeler memnuniyetle karşılanır.*
