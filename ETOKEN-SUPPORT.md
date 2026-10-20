# USB ký số nào dùng được với MacSigner?

Cập nhật ngày **20/10/2026**.

> [!IMPORTANT]
> **Câu trả lời ngắn:** hiện tại **chưa có USB ký số nào** được xác nhận ký được bằng MacSigner.
>
> MacSigner đang ở giai đoạn thử nghiệm (Alpha). Với USB của **CA2** và **E-CA**, MacSigner đã nhận được USB và đọc được chữ ký số bên trong, nhưng chưa ký thử. Các hãng khác còn chưa được thử với USB thật.
>
> Nếu bạn cần ký ngay, hãy dùng **chữ ký số dạng tệp** (tệp `.p12` hoặc `.pfx` do nhà cung cấp gửi). Cách này đã dùng được.

## Tra cứu theo nhà cung cấp

Tìm tên đơn vị đã bán USB ký số cho bạn (in trên USB hoặc trên hợp đồng). Danh sách gồm 26 nhà cung cấp chữ ký số công cộng tại Việt Nam, theo [Vietnam RootCA](https://rootca.gov.vn/).

**Cách đọc cột "Hiện trạng":**

- 🟢 **Đã nhận (không cần cài đặt gì):** cắm USB vào là MacSigner thấy được chữ ký số bên trong, không cần cài thêm phần mềm nào.
- 🟡 **Thông qua phần mềm của hãng:** bạn cần cài phần mềm USB token (driver) của hãng, MacSigner sẽ dùng USB qua phần mềm đó. Chưa thử với USB thật.
- ⚪ **Chưa có gì:** chưa có phần mềm của hãng, chưa thử được.

| Logo | Nhà cung cấp | Hiện trạng | Ghi chú |
| --- | --- | --- | --- |
| <img src="assets/ca-logos/bkavca.jpg" alt="BkavCA" width="96"> | BkavCA | 🟡 Thông qua phần mềm của hãng | Hãng có nhiều đời USB, cần thử từng đời |
| <img src="assets/ca-logos/ca2.jpg" alt="CA2" width="96"> | CA2 / Nacencomm | 🟢 Đã nhận (không cần cài đặt gì) | Đã đọc được 2 chữ ký số trên USB CA2 ACE |
| <img src="assets/ca-logos/cmc-ca.png" alt="CMC-CA" width="96"> | CMC-CA | 🟡 Thông qua phần mềm của hãng | |
| <img src="assets/ca-logos/e-ca.jpg" alt="E-CA" width="96"> | E-CA (Thái Sơn) | 🟢 Đã nhận (không cần cài đặt gì) | Đã đọc được 1 chữ ký số |
| <img src="assets/ca-logos/easyca.png" alt="EASYCA" width="96"> | EASYCA | 🟡 Thông qua phần mềm của hãng | |
| <img src="assets/ca-logos/efy-ca.jpg" alt="EFY-CA" width="96"> | EFY-CA | 🟡 Thông qua phần mềm của hãng | |
| <img src="assets/ca-logos/fastca.png" alt="FASTCA" width="96"> | FASTCA | 🟡 Thông qua phần mềm của hãng | Có cho USB đời V4, V6, V9 |
| <img src="assets/ca-logos/fpt-ca.png" alt="FPT-CA" width="96"> | FPT-CA | 🟡 Thông qua phần mềm của hãng | Chưa đủ cho mọi đời USB |
| <img src="assets/ca-logos/hilo-ca.png" alt="Hilo-CA" width="96"> | Hilo-CA | 🟡 Thông qua phần mềm của hãng | Chưa đủ cho mọi đời USB |
| <img src="assets/ca-logos/i-ca.png" alt="I-CA" width="96"> | I-CA | 🟡 Thông qua phần mềm của hãng | |
| <img src="assets/ca-logos/intrustca.jpg" alt="IntrustCA" width="96"> | IntrustCA | ⚪ Chưa có gì | Hãng đang dùng hình thức ký từ xa, không dùng USB |
| <img src="assets/ca-logos/lcs-ca.png" alt="LCS-CA" width="96"> | LCS-CA | 🟡 Thông qua phần mềm của hãng | Có cho USB đời V2 |
| <img src="assets/ca-logos/matbao-ca.jpg" alt="MATBAO-CA" width="96"> | MATBAO-CA | ⚪ Chưa có gì | |
| <img src="assets/ca-logos/misa-ca.jpg" alt="MISA-CA" width="96"> | MISA-CA | 🟡 Thông qua phần mềm của hãng | Phần mềm đã chạy được trên máy thử |
| <img src="assets/ca-logos/mk-ca.png" alt="MK CA" width="96"> | MK CA | ⚪ Chưa có gì | |
| <img src="assets/ca-logos/mobifone-ca.png" alt="MOBIFONE CA" width="96"> | MOBIFONE CA | ⚪ Chưa có gì | Mới có tài liệu hướng dẫn của hãng |
| <img src="assets/ca-logos/nc-ca.png" alt="NC-CA" width="96"> | NC-CA | 🟡 Thông qua phần mềm của hãng | |
| <img src="assets/ca-logos/newtel-ca.png" alt="Newtel-CA" width="96"> | Newtel-CA | 🟡 Thông qua phần mềm của hãng | Có cho USB đời V6 |
| <img src="assets/ca-logos/one-ca.jpg" alt="ONE-CA" width="96"> | ONE-CA | 🟡 Thông qua phần mềm của hãng | |
| <img src="assets/ca-logos/safe-ca.jpg" alt="Safe-CA" width="96"> | Safe-CA | 🟡 Thông qua phần mềm của hãng | Có cho USB đời V1, V2 |
| <img src="assets/ca-logos/smartsign.png" alt="SmartSign" width="96"> | SmartSign / Vina-CA | 🟡 Thông qua phần mềm của hãng | Có cho USB đời V6 |
| <img src="assets/ca-logos/trustca.png" alt="TrustCA" width="96"> | TrustCA | 🟡 Thông qua phần mềm của hãng | Chưa đủ cho mọi đời USB |
| <img src="assets/ca-logos/viettel-ca.jpg" alt="Viettel-CA" width="96"> | Viettel-CA | 🟡 Thông qua phần mềm của hãng | Phần mềm cho USB đời V0, V2–V5 đã chạy được trên máy thử; bản cho đời V1 còn lỗi |
| <img src="assets/ca-logos/vnpay-ca.png" alt="VNPAY-CA" width="96"> | VNPAY-CA | ⚪ Chưa có gì | Chưa rõ hãng dùng loại USB nào |
| <img src="assets/ca-logos/vnpt-ca.jpg" alt="VNPT-CA" width="96"> | VNPT-CA | 🟡 Thông qua phần mềm của hãng | **Hãng có phần mềm cho Mac** (cả Mac chip Intel và chip Apple) |
| <img src="assets/ca-logos/winca.png" alt="WINCA" width="96"> | WINCA | 🟡 Thông qua phần mềm của hãng | Lấy từ đại lý, chưa đối chiếu với hãng |

Bảng này chỉ nói về **USB ký số**. Thẻ ký số (smart card), SIM ký số và ký từ xa qua điện thoại là những loại khác, chưa được xét ở đây.

## USB của tôi không thuộc nhóm 🟢, tôi làm gì?

1. **Cần ký ngay:** hỏi nhà cung cấp chữ ký số xin tệp `.p12` hoặc `.pfx`, rồi nhập vào MacSigner ở mục **Chứng thư số → Nhập P12/PFX**.
2. **Muốn thử USB:** cài phần mềm USB token bản cho Mac của hãng (nếu hãng có), cắm USB và mở MacSigner. Biểu tượng USB ở góc trên bên phải sẽ cho biết MacSigner có nhận được USB hay không.
3. **Giúp dự án:** báo kết quả bạn thử được theo hướng dẫn ở [cuối trang](#bạn-có-usb-ký-số-và-muốn-giúp).

## MacSigner làm gì và không làm gì với USB của bạn

**Có làm:**

- Nhận biết USB khi bạn cắm vào.
- Đọc chữ ký số trong USB để bạn chọn. Bước này không cần PIN.
- Hỏi PIN khi bạn bấm ký. PIN do bạn nhập, và chỉ dùng cho lần ký đó.
- Báo khi chữ ký số sắp hết hạn.

**Không bao giờ làm:**

- **Tự thử PIN** hoặc dùng PIN mặc định. Nhập sai PIN nhiều lần có thể **khóa USB**, nên MacSigner không bao giờ tự nhập lại PIN.
- **Đổi PIN hoặc mở khóa USB.** Việc này bạn làm bằng phần mềm của hãng.
- **Gia hạn hoặc cấp lại chữ ký số.** Chỉ nhà cung cấp mới làm được.
- **Xóa hay ghi đè bất cứ thứ gì trên USB.**

## Câu hỏi thường gặp

**Có cần cài thêm phần mềm gì không?**
Tùy hãng. Với USB nhóm 🟢 thì không cần cài gì, cắm vào là dùng. Với USB nhóm 🟡, bạn cần cài phần mềm USB token (driver) của chính hãng đã bán USB cho bạn; MacSigner tự tìm phần mềm đó sau khi cài, bạn không cần chỉ đường dẫn. Trình duyệt web không tự đọc được USB; nó chỉ chuyển yêu cầu ký sang cho MacSigner.

**Tôi vừa gia hạn chữ ký số, MacSigner có tự dùng chữ ký mới không?**
Không tự đổi. MacSigner thấy được chữ ký mới trên USB, nhưng bạn là người chọn dùng chữ ký nào. Chữ ký cũ không bị xóa.

**Chữ ký số còn hạn thì chắc chắn còn dùng được?**
Chưa chắc. Chữ ký số còn hạn vẫn có thể đã bị nhà cung cấp thu hồi (ví dụ khi bạn báo mất USB). Hiện MacSigner **chưa kiểm tra** được chữ ký đã bị thu hồi hay chưa.

**MacSigner chạy trên Windows và Linux được không?**
Có, nhưng cũng giống trên Mac: USB ký số của từng hãng vẫn chưa được thử đủ trên từng hệ điều hành.

**Vì sao nhận được USB rồi mà vẫn chưa ghi là "đã hỗ trợ"?**
Vì nhận được USB và ký được là hai chuyện khác nhau. Một loại USB chỉ được ghi là đã hỗ trợ khi đã qua đủ 4 bước dưới đây trên USB thật, với **từng hãng, từng đời USB và từng hệ điều hành**:

1. Nhận đúng USB, kể cả khi rút ra cắm lại hay cắm nhiều USB cùng lúc.
2. Hiện đúng chữ ký số trong USB.
3. Nhập PIN, bấm hủy và báo lỗi đều chạy đúng, không tự thử lại PIN.
4. Ký thử một tài liệu, rồi dùng công cụ khác kiểm tra chữ ký đó hợp lệ.

## Bạn có USB ký số và muốn giúp?

Hãy [mở một Issue](../../issues) và ghi:

- Tên nhà cung cấp và đời USB (thường in trên vỏ USB).
- Máy bạn dùng (Mac chip Intel hay chip Apple, phiên bản macOS; hoặc Windows/Linux).
- Kết quả: MacSigner có nhận được USB không, có thấy chữ ký số không, ký có được không. Nếu có báo lỗi, chụp màn hình lỗi.

**Tuyệt đối không gửi PIN** hay bất kỳ tệp chữ ký số nào.

---

## Dành cho contributor

**21/26** nhà cung cấp đã được implement: MacSigner tự tìm và nạp thư viện PKCS#11 của hãng (ít nhất một phiên bản), danh sách nằm trong [`src/token/data/token-modules.json`](src/token/data/token-modules.json). Mỗi hãng có nhiều đời USB, nên con số này không phải tỷ lệ thiết bị đã chạy được. Trạng thái chỉ đúng cho những phiên bản phần mềm và thiết bị đã thử.

| Hạng mục | Trạng thái |
| --- | --- |
| Dò đầu đọc và token qua PKCS#11 | Đã có |
| Đọc chứng thư không cần đăng nhập | Đã thử với CA2 ACE và E-CA; một số token buộc đăng nhập trước |
| Đăng nhập PIN trong ứng dụng | Đã có, chưa thử với mọi CA |
| Hộp thoại PIN của Windows | Đã tích hợp; phụ thuộc middleware của hãng |
| Module PKCS#11 32-bit với ứng dụng 64-bit | Đã có tiến trình trung gian x86; đã đọc được CA2 ACE và E-CA, chưa ký bằng PIN |
| Ký RSA PKCS#1 v1.5 với SHA-1/256/384/512 | Đã có, có kiểm thử phần mềm (SoftHSM); chưa thử trên token thật |
| ECDSA, RSA-PSS | Chưa có |
| Phát hiện chứng thư thêm/bớt sau gia hạn | Đã có trong thư viện, chưa nối vào giao diện |
| Cảnh báo sắp hết hạn | Đã có; chưa có nhắc lịch chạy nền |
| Gói CA gốc/trung gian mới | Chỉ chấp nhận theo danh sách đã duyệt; chưa tự tải hay cài vào kho tin cậy |
| Kiểm tra chuỗi tin cậy và thu hồi (CRL/OCSP) | Chưa hoàn chỉnh |
| Gia hạn, cấp lại qua CA | Chưa có; cần kết nối chính thức và phê duyệt của từng CA |
| Đổi/mở khóa PIN, ghi chứng thư, xóa khóa | Không làm; không thay đổi dữ liệu trên token |
| Nhiều yêu cầu ký đồng thời | Chưa bảo đảm; cần điều phối phiên, cô lập lỗi và đo tải |

Máy thử nghiệm hiện đã cài middleware CA2, nên chưa thể kết luận một máy sạch cũng chạy được. Kiểm thử phần mềm đạt không thay thế kiểm thử trên thiết bị thật. Chỉ nâng trạng thái trong tài liệu này khi Pull Request có kèm ảnh chụp chứng minh đã thử.
