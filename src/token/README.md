# Cấu trúc lớp token và mật mã

Lớp dùng chung phục vụ ESIGN, CTSIGN, CLI và giao diện. Mã chạy trong `token/` và `crypto/` không phụ thuộc các mô-đun giao thức hoặc giao diện đó.

```text
src/crypto/
  mod.rs                 API mật mã dùng chung
  algorithm.rs           Kiểu thuật toán RSA/hash
  certificate.rs         Đọc X.509, khóa công khai, PkiPath

src/token/
  mod.rs                 API phần cứng dùng chung
  context.rs             Đăng ký context PKCS#11 theo đường dẫn module
  loader.rs              Nạp DLL và phụ thuộc trên Windows
  pkcs11.rs              Thao tác token/chứng thư/ký qua PKCS#11
  reader.rs              Kiểm kê đầu đọc qua PC/SC
  windows_store.rs       Tích hợp Windows CNG/CryptoAPI
  lifecycle.rs           Theo dõi phiên bản chứng thư và kiểm tra gói CA cập nhật
  data/token-modules.json Danh mục tên middleware
  discovery/
    mod.rs               Tổng hợp token, chứng thư và chẩn đoán
    catalog.rs           Nhận diện ứng viên; đọc catalog một lần
    paths.rs             Đường dẫn theo OS, portable, p11-kit
    probe.rs             Tiến trình con, thời hạn, kiểm tra PE
    selection.rs         Chọn chứng thư trong terminal, giữ API cũ
```

## Hướng phụ thuộc

```text
ESIGN / CTSIGN / desktop / CLI
                ↓
       token → crypto
                ↓
     thư viện mật mã / OS / vendor SDK
```

`crypto` không biết thiết bị, dịch vụ HTTP hay hộp thoại PIN. `token` không biết payload ESIGN/CTSIGN hoặc bố cục PDF. Việc ký tài liệu và chính sách cho phép API tiếp tục do lớp ứng dụng hiện có điều phối.

Các đường dẫn `esign::certificate`, `esign::online::Algorithm`, `esign::{discovery, pkcs11, reader}` và `certstore` được giữ bằng re-export để mã gọi hiện tại tiếp tục biên dịch. Chỉ có một triển khai cho mỗi chức năng; khi viết tích hợp mới, dùng `crypto` và `token` trực tiếp.

## Thêm nhà cung cấp

- Nếu thiết bị có PKCS#11 chuẩn: cấu hình hoặc đóng gói module portable phù hợp; thêm tên middleware vào `data/token-modules.json` khi cần dò thư mục hệ thống. Không thêm nhánh theo tên CA vào luồng ký.
- Nếu có giao thức/SDK riêng: tạo adapter riêng trong `token/`, giữ lệnh vendor và quản lý phiên ở adapter; không đưa mã đó vào ESIGN, CTSIGN hoặc `crypto`.
- Mã riêng nền tảng dùng `cfg`; chỉ mở rộng `paths.rs` khi thay đổi nguồn tìm driver, `probe.rs` khi thay đổi cơ chế cô lập/thời hạn.
- Trên Windows, nguồn tìm kiếm gồm cả `System32` và `SysWOW64`, ngoài thư mục chương trình và gói portable, để ứng dụng 64-bit không bỏ sót thành phần 32-bit. Việc tìm thấy thành phần không chứng minh USB đã ký được.
- Đường dẫn bộ kết nối VNPT trong ứng dụng macOS được đưa vào catalog theo vị trí cài đặt quan sát từ gói của hãng. Đã xác nhận tĩnh thư viện chứa cả Intel và ARM64; chưa xác nhận nạp thư viện, nhập PIN hoặc ký trên máy Mac thật.
- Trên macOS, dò thêm các ứng dụng quản lý token trong `/Applications` và `~/Applications`, cùng cả `Contents/MacOS/pkcs11` và `Contents/Resources/pkcs11` của MacSigner. Đây là tìm bộ kết nối đã có, không tải hoặc cài phần mềm, không làm thư viện Windows chạy được trên Mac và không biến thư viện Intel thành ARM64. Kiểm thử đường dẫn bằng cây thư mục giả không xác nhận đọc hoặc ký trên Mac M2 thật.
- Trước khi nạp trên macOS, đọc kiến trúc từ header Mach-O đơn hoặc universal (32/64-bit, hai thứ tự byte). Bộ kết nối phải chứa kiến trúc của tiến trình MacSigner đang chạy; ARM64 không nạp được bộ kết nối chỉ có Intel. Lỗi sai kiến trúc đi vào `module_errors`, không bị gộp thành lỗi probe chung. Đây là kiểm tra header với giới hạn số mục, không xác minh chữ ký mã, phụ thuộc hoặc tính toàn vẹn toàn bộ tệp; ba kiểm thử parser dùng dữ liệu tổng hợp trên Windows, không thay thế chạy trên Mac.
- Kiểm chứng thiết bị mới bằng đọc chứng thư không đăng nhập, rồi ký tài liệu thử do người vận hành nhập PIN trực tiếp. Catalog tên driver không chứng minh tương thích phần cứng.

Khi quét một bộ kết nối, `pkcs11::tokens` tiếp tục sau các lỗi `C_GetTokenInfo` chỉ ra khe đã biến mất, token bị rút hoặc không được bộ kết nối nhận diện. Nhờ đó USB của hãng khác không làm bỏ qua các khe còn lại. Chỉ bỏ qua bốn mã `CKR_SLOT_ID_INVALID`, `CKR_DEVICE_REMOVED`, `CKR_TOKEN_NOT_PRESENT`, `CKR_TOKEN_NOT_RECOGNIZED` tại đúng bước đọc thông tin token; các lỗi khác vẫn làm probe báo lỗi. Quét xong với danh sách rỗng không chứng minh đã nhận USB tương ứng.

## Vòng đời chứng thư

Ba luồng riêng biệt, không dùng chung quyền hoặc coi là một thao tác gia hạn:

- **Nhận chứng thư mới trên USB:** `lifecycle::CertificateInventory` giữ một snapshot cho từng thiết bị; `refresh_pkcs11` đọc lại chứng thư không yêu cầu PIN. So sánh SHA-256 toàn bộ DER, không dùng serial hay nhãn khóa làm định danh phiên bản. Snapshot chỉ thay sau khi đọc và phân tích thành công toàn bộ; lỗi không xóa dữ liệu cũ. `removed` chỉ nghĩa là không còn thấy trong snapshot, không khẳng định bị thu hồi. Lớp gọi phải kiểm tra đúng thiết bị trước khi tái sử dụng inventory, bỏ lựa chọn ký cũ khi chứng thư đó biến mất, và yêu cầu người dùng chọn chứng thư mới; không tự chuyển khóa.
- **CA gốc/trung gian:** `validate_ca_update` kiểm tra DER, giới hạn kích thước, Basic Constraints, Key Usage, thời hạn và toàn bộ tập SHA-256 được phê duyệt. Tập fingerprint phải đến từ cấu hình đáng tin hoặc manifest đã xác thực độc lập, tuyệt đối không lấy cùng nguồn tải chưa xác thực. Đây chỉ là cổng kiểm tra gói để staging, chưa tải mạng, chưa kiểm tra đường dẫn/chữ ký chuỗi X.509 hay CRL/OCSP, chưa sửa trust store hệ điều hành. Một CA trong USB hoặc tải qua HTTPS không mặc nhiên được tin cậy. Không xóa CA cũ còn cần để xác minh chữ ký lịch sử.
- **Gia hạn/cấp lại:** cần adapter theo giao thức chính thức của từng CA, tài khoản/hồ sơ và sự phê duyệt của nhà cung cấp. Chưa có adapter thực hiện trong lớp này. Không suy luận API gia hạn từ API ký PKCS#11; không tự sinh/xóa khóa, ghi đè chứng thư hoặc gửi yêu cầu có phí. Theo hướng dẫn VNPT, chứng thư mới chỉ được cập nhật vào token sau khi thủ tục gia hạn đã hoàn tất và được duyệt: https://www.vnpt-ca.vn/documents/download?fileNameDownload=documents%2F05082019220923.pdf. Khi có SDK/API được xác nhận, đặt adapter trong `token/`, giữ xác thực, retry/idempotency và bước xác nhận thao tác ghi riêng với luồng ký.

`CertificateVersion::validity` hỗ trợ kiểm tra hết hạn/cảnh báo với thời điểm do lớp gọi cung cấp; đây không phải kiểm tra thu hồi hoặc tính tin cậy. API vòng đời chưa được nối vào giao diện hoặc bộ lịch nền; không coi thay đổi core này là tính năng tự gia hạn hoàn chỉnh.

## Kiểm tra kết quả ký

Adapter CTSIGN thay thế backend đã nạp chỉ khi toàn bộ DER chứng thư giống nhau, không coi serial là định danh toàn cục giữa các CA. Hai chứng thư khác nhau có cùng serial vẫn được giữ riêng. Hợp đồng yêu cầu ký hiện chỉ nhận serial, nên khi có nhiều chứng thư đã nạp cùng khớp serial chuẩn hóa, adapter trả lỗi trước khi gọi backend hoặc hộp thoại PIN; không tự chọn chứng thư đầu tiên. Khi chưa nạp chứng thư khớp và tra cứu kho Windows, tất cả kết quả cùng serial được xem xét, loại bản sao DER trước khi kiểm tra mơ hồ. Việc này không tự xác minh chuỗi tin cậy, không giải quyết định danh qua API mới và không chứng minh đã ký trên hai USB thật.

`pkcs11::sign_many` xác minh từng chữ ký RSA trước khi trả về toàn bộ lô. Nếu CKA_ID có chứng thư, chỉ chấp nhận đúng một chứng thư RSA còn hạn; nếu token phần mềm hoặc thiết bị chỉ có khóa, lấy khóa công khai cùng CKA_ID hoặc tham số công khai của khóa bí mật. Thiếu tham số xác minh, trùng đối tượng hoặc chữ ký sai làm hỏng toàn bộ kết quả trả về; thao tác ký đã thực hiện trong thiết bị không thể được hoàn tác.

Sau đăng nhập thành công, cả luồng ký digest và ký hàng loạt đều thử đăng xuất khi thao tác trả lỗi, không tự đăng nhập hoặc ký lại. Không trả chữ ký nếu đăng xuất báo lỗi. Token báo PIN bị khóa bị từ chối trước bước đăng nhập; đây không phải bảo đảm trạng thái thiết bị không thay đổi đồng thời.

Kiểm thử SoftHSM trên Windows dùng kho token phần mềm riêng và PIN thử nghiệm, không nạp bộ kết nối CA hoặc ghi khóa lên USB thật. Bốn thuật toán RSA/hash đi qua API PKCS#11 và được xác minh bằng khóa công khai, gồm cả trường hợp token chưa có chứng thư. Kết quả này không xác nhận mã PIN, firmware, bộ kết nối hoặc chính sách của từng CA.

## Phạm vi kiểm chứng

Đây là thay đổi cấu trúc và hướng phụ thuộc, giữ nguyên hợp đồng hiện tại. Không đặt chỉ tiêu thông lượng khi chưa đo tải: dò driver vẫn tuần tự, mỗi lần probe có thời hạn riêng; lớp `SigningState` hiện tại còn giữ khóa chung khi ký. Cô lập tiến trình hiện chỉ áp dụng cho dò thiết bị, không áp dụng cho toàn bộ quá trình ký. Mở rộng số nhà cung cấp và tăng số yêu cầu ký đồng thời là hai công việc khác nhau.

Kiểm tra bằng `scripts/cargo-windows.ps1 test --locked`. Một số test phần cứng phụ thuộc biến môi trường và có thể trả về sớm khi thiếu thiết bị/module; kết quả test không thay thế kiểm thử thiết bị thật.
