---
title: Bağlantıdan bir sayfa açmak
---

# Bağlantıdan bir sayfa açmak

## Planlanan: NoteCraft'ı açan bağlantılar

NoteCraft'ın bir tarayıcıdan, e-postadan, sohbetten ya da başka bir uygulamadan belirli bir defter sayfasını
açan bağlantıları desteklemesi planlanıyor. İşletim sistemi NoteCraft'ı başlatmak ve ona açılacak sayfayı
vermek için `notecraft:` bağlantı türünü kullanacak. İşletim sisteminize ve tarayıcınıza bağlı olarak,
uygulama açılmadan önce onay istenebilir.

Bu özellik **henüz kullanılamaz**. NoteCraft şu anda `notecraft:` türünü işletim sisteminize kaydetmiyor ve
dış bir bağlantıyı sayfasına giderek işlemiyor. Bir NoteCraft sayfa bağlantısını kopyalamak, bugün bir
tarayıcıdan açılabileceği anlamına gelmez.

## Kaydın planlanan işleyişi

- **Windows kurulum programı:** kurulum sırasında NoteCraft'ı kaydeder ve NoteCraft kaldırıldığında bu kaydı siler.
- **Windows taşınabilir ZIP:** çalıştırmayı seçtiğiniz ayrı bir **NoteCraft bağlantılarını kaydet** eylemi ve
  bunu geri almanın bir yolu sağlanır. Yalnızca ZIP'i açmak bağlantıyı kaydetmez.
- **macOS:** bağlantı türü NoteCraft uygulama paketinde bildirilir.
- **Linux:** bağlantı türü için bir masaüstü uygulama ilişkisi kurulur.

Kayıt yalnızca işletim sistemine bir bağlantıyı hangi uygulamanın alabileceğini söyler. NoteCraft ayrıca
eşleşen defteri ve sayfayı bulmalı, uygulama zaten çalışırken bağlantıları işlemeli ve bir defter ya da sayfa
bulunamadığında bunu açıkça anlatmalıdır. Bu işleyiş de planlanan özelliğin bir parçasıdır.

*Bu sayfa bir yapay zekâ modeli tarafından çevrildi. Düzeltmeler memnuniyetle karşılanır.*
