---
id: 8d0235e1b74901763052
title: "Mermaid diyagramları"
order: 82
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

Bir diyagramı, çitli bir blok içinde Mermaid metni olarak yazın. NoteCraft bloğu geçerli temada sayfada
çizer; metni düzenlemek için **< >** ile sayfa kaynağını açın.

## Akış şeması

```mermaid
flowchart TD
  Start[Bir not yazın] --> Save{Kaydedildi mi?}
  Save -->|evet| Ready[Devam edin]
  Save -->|henüz değil| Wait[Kısa bir süre durun]
  Wait --> Save
```

## Sıra

```mermaid
sequenceDiagram
  Writer->>Page: Bir satır yazın
  Page->>Files: Sayfayı kaydedin
  Files-->>Page: Kaydedildi
  Page-->>Writer: Yazmaya devam edin
```

## Zihin haritası

```mermaid
mindmap
  root((Bir defter))
    Sayfalar
      Sözcükler
      Denklemler
    Çizimler
      Şekiller
      Mürekkep
    Dosyalar
      Resimler
      Ekler
```

## Durum diyagramı

```mermaid
stateDiagram-v2
  [*] --> Hazir
  Hazir --> Duzenleniyor: yaz
  Duzenleniyor --> Kaydedildi: dur
  Kaydedildi --> Duzenleniyor: yeniden yaz
  Kaydedildi --> [*]
```

## Pasta diyagramı

```mermaid
pie title Bu turdaki sayfalar
  "Yazma" : 4
  "Çizim" : 2
  "Dosyalar" : 2
```

Her blok kendi diyagramıdır. Metin sayfa dosyasında kalır; bu yüzden bir resim düzenleyicisi olmadan
değiştirilebilir ya da kopyalanabilir.

Sonraki: [Grafikler, Vega ve D3](../charts-vega-d3/).

*Bu sayfa bir yapay zekâ modeli tarafından çevrildi. Düzeltmeler memnuniyetle karşılanır.*
