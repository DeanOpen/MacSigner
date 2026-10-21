# Bộ kết nối đi kèm ứng dụng

Mục tiêu là cài MacSigner một lần, sau đó cắm USB và nhập PIN trong ứng dụng, không yêu cầu cài trình quản lý token riêng. Chưa đạt mục tiêu này cho mọi USB hoặc cho E-CA/CA2 trên Mac M2.

## Trạng thái gói cục bộ

| Nhà cung cấp | Nền tảng / kiến trúc bộ kết nối | Mức bằng chứng | Được đưa vào gói hiện tại |
|---|---|---|---|
| E-CA | Windows x86, qua worker trong ứng dụng x64 | Đã đọc chứng thư trước đây trên máy Windows; chưa test full ký bằng PIN | Có |
| CA2 ACE | Windows x86, qua worker trong ứng dụng x64 | Đã đọc chứng thư trước đây trên máy Windows; chưa test full ký bằng PIN | Có |
| E-CA / CA2 | macOS ARM64 / universal | Chưa có bộ đọc độc lập hoàn chỉnh; đang phân tích giao thức | Không |
| Các biến thể / nhà cung cấp khác | Theo từng bộ kết nối | Có tên hàm, khởi tạo được hoặc có gói cài không đồng nghĩa đã đọc/ký được | Chưa tự động đưa vào |

Lần thử gói mới ngày 21/10/2026: cả hai bộ kết nối Windows khởi tạo thành công nhưng trả về không có token. Không dùng PIN và không ký. Đây không phải phép thử trên máy sạch hoặc trên Mac.

## UI dịch vụ VNPT

Mục Dịch vụ và thanh bên có VNPT-CA Plugin, logo, trạng thái, cổng cấu hình và danh sách cổng client gốc thử. Cổng được lưu cùng cấu hình hiện có; tệp cấu hình cũ không có mục VNPT vẫn dùng mặc định 4433. Kiểm tra xung đột với ESIGN, cổng dự phòng ESIGN và CTSIGN khi lưu. Trường cổng chỉ lưu khi rời ô hoặc nhấn Enter, không ghi từng chữ số đang nhập.

Nút kiểm tra cổng thực hiện kết nối TCP có thời hạn tới `127.0.0.1`, gồm cổng đã cấu hình và 4433/4434/4435/9201/9202, không gửi dữ liệu ứng dụng hoặc PIN. Kết nối thành công không xác định danh tính dịch vụ hoặc tính hợp lệ TLS; kết nối bị từ chối không bảo đảm cổng có thể bind. Bản xem trước trình duyệt luôn ghi không kiểm tra cổng thật, không giả kết quả máy tính.

Máy chủ WSS, backend ký và xác thực thử thách VNPT chưa hoàn chỉnh. UI ghi Chưa sẵn sàng, vô hiệu hóa bật/tự bật; backend cũng từ chối yêu cầu bật và lưu autostart, không chỉ chặn trên giao diện. Kiểm thử tích hợp hiển thị rõ kiểm tra frame mẫu và các bước bị bỏ qua, không trả kết quả ký thành công giả.

## Chuẩn bị và build

`token-connectors.local.json` chỉ định chính xác từng tệp, SHA-256, nền tảng, kiến trúc và mức bằng chứng. Các tệp nhị phân nguồn là dữ liệu cục bộ, không nằm trong Git. Manifest này phục vụ bản build thử nội bộ; chưa xác nhận điều kiện phân phối lại thành phần bên thứ ba cho bản phát hành công khai.

Từ thư mục gốc, dùng một thư mục đầu ra mới có thư mục cha tồn tại:

```powershell
node scripts/prepare-token-bundle.mjs packaging/token-connectors.local.json . .tools/token-build-new windows x86_64
scripts/node-env.ps1 pnpm exec tauri build --debug --bundles nsis --config ../.tools/token-build-new/tauri.tokens.conf.json
node --test scripts/prepare-token-bundle.test.mjs
```

Script không tải, chạy trình cài của hãng, sửa trust store hoặc ghi vào USB. Trước khi staging, script kiểm tra đường dẫn tương đối, từ chối liên kết tượng trưng, xác minh hash và header kiến trúc. Gói x64 chứa bộ kết nối x86 phải có worker x86 đúng hash. Giữ nguyên tên tệp và thành phần phụ thuộc của hãng. Không ghi đè thư mục staging đã tồn tại.

Tauri nhận bản cấu hình tài nguyên được tạo riêng; không làm thay đổi cấu hình build mặc định hoặc tự động phát hành. Bộ kết nối đặt tại `pkcs11/<nhà-cung-cấp>/<kiến-trúc>/`, worker Windows đặt cạnh tệp chạy chính. Trên macOS, tài nguyên tương ứng nằm trong `Contents/Resources/pkcs11`. Báo cáo `token-bundle.json` đi cùng gói, giữ nguyên mức bằng chứng; `static-only` không được nâng thành đã ký thành công.

Không có bộ kết nối phù hợp thì bước chuẩn bị thất bại thay vì tạo một gói rỗng rồi công bố có hỗ trợ. Hiện lệnh chọn macOS với manifest cục bộ này sẽ thất bại có chủ đích. Một thư viện universal có cả Intel và ARM64 cũng chưa chứng minh phụ thuộc, ký mã, giao thức hoặc thiết bị tương thích.

## Thêm nhà cung cấp

1. Kiểm tra nguồn, hash, phụ thuộc và kiến trúc của bộ kết nối, tách USB vật lý khỏi dịch vụ ký từ xa.
2. Nếu cần bộ đọc độc lập, khôi phục và xác minh giao thức đọc chứng thư trước; không suy đoán lệnh APDU hoặc tự gửi lệnh PIN/ghi dữ liệu.
3. Thử khởi tạo và đọc chứng thư không đăng nhập; lưu kết quả đúng biến thể thiết bị và hệ điều hành.
4. Thử ký tệp mẫu với PIN chỉ nhập trực tiếp trong ứng dụng, xác minh chữ ký bằng khóa công khai.
5. Thêm manifest khi có đủ thành phần và hash; xác nhận quyền phân phối trước khi đưa vào bản phát hành công khai. Gia hạn/cấp lại và cập nhật CA cần kiểm thử riêng.
