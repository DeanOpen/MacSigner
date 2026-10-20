# Worker PKCS#11 x86

Executable console riêng, tái sử dụng trực tiếp `token/context.rs`, `token/loader.rs`, `token/pkcs11.rs` và `crypto/`; không có bản sao thuật toán hoặc bộ đọc chứng thư.

## Trạng thái

- Đã build và chạy Windows x86 trên máy Windows x64.
- Đã dùng module CA2 ACE x86 được lấy từ gói chính thức để đọc một token và hai chứng thư, không nhập PIN. Kết quả chỉ áp dụng thiết bị và phiên bản đã thử.
- Đã dùng module E-CA x86 từ gói chính thức để đọc một token ECA và một chứng thư do E-CA/THAISONSOFT phát hành, không nhập PIN và không cài Token Manager E-CA. Windows nhận đầu đọc FT ePass2003Auto bằng USB CCID; ứng dụng x64 chưa nối luồng này.
- Có `version`, `probe-token`, `probe-cert` và `sign-digest`. Lệnh ký chỉ đọc yêu cầu có framing qua stdin, không nhận PIN trên command line. Chưa kiểm chứng ký bằng PIN trên thiết bị thật.
- Discovery và adapter `ctsign::signing::open_token` chọn worker khi ứng dụng Windows x64 gặp module x86. Đặt worker dưới tên `macsigner-token-x86.exe` cạnh executable đang chạy; bộ portable đi theo thư mục `pkcs11/`. Chưa tự đóng gói vào bộ phát hành.
- Không có lệnh đổi PIN, gia hạn, ghi chứng thư hoặc xóa khóa. Không coi kết quả đọc là hỗ trợ x86 end-to-end đã kiểm chứng.
- Worker không phải sandbox. Chỉ nạp module có nguồn đáng tin; tiến trình gọi phải đặt timeout và giới hạn stdout.

## Build và test

Yêu cầu Rust toolchain `stable-x86_64-pc-windows-gnu`, target `i686-pc-windows-gnu`, compiler host x64 và compiler MinGW i686 POSIX/DWARF/MSVCRT đầy đủ. Một alias `gcc -m32` thiếu thư viện runtime không đủ.

```powershell
rustup target add i686-pc-windows-gnu --toolchain stable-x86_64-pc-windows-gnu
& src/token/worker/build-windows.ps1 -ToolBin '<host-x64-bin>' -X86ToolBin '<i686-bin>'
& src/token/worker/build-windows.ps1 -ToolBin '<host-x64-bin>' -X86ToolBin '<i686-bin>' -Test
```

Có thể cấu hình `W64DEVKIT_BIN` và `MACSIGNER_X86_TOOLCHAIN_BIN`. Script có đường dẫn dự phòng tới toolchain portable của workspace nghiên cứu hiện tại; máy khác cần tự cung cấp compiler, không tự tải/cài từ script. Lockfile của worker được quản lý riêng; target output bị bỏ qua bởi Git.

Compiler wrapper native chỉ thêm `link-self-contained=yes` cho các lệnh rustc host không có `--target`. Build target i686 dùng `link-self-contained=no`, lấy CRT và thư viện từ cùng compiler i686. Không trộn `crt2.o` đóng gói trong Rust với MinGW runtime khác phiên bản: cấu hình trộn đã gây lặp `atexit`/`onexit` và tràn stack trước khi nạp vendor DLL. Wrapper dùng Win32 trực tiếp để giữ nguyên command line dài, tránh giới hạn 8191 ký tự của batch shell.

## Giao thức đọc

```text
macsigner-token-worker.exe version
macsigner-token-worker.exe probe-token C:\absolute\vendor.dll
macsigner-token-worker.exe probe-cert C:\absolute\vendor.dll 1
```

Các lệnh đọc trả JSON trên stdout, tối đa 4 MiB; lỗi dùng stderr và exit code 1. Module phải là đường dẫn tuyệt đối tới file tồn tại. Đối với gói có DLL x86/x64 trùng tên, giữ thư mục riêng theo kiến trúc, khôi phục đúng tên DLL/phụ thuộc/chữ ký theo bộ cài trước khi thử. Không sửa DLL hệ thống và không chạy installer chỉ để probe.

## Cầu ký digest

`token::remote::WorkerClient` cung cấp inventory và ký digest RSA qua worker. Lệnh `sign-digest ABSOLUTE_MODULE` nhận đúng một frame: độ dài little-endian 4 byte, tiếp theo là JSON tối đa 64 KiB và EOF. Đầu ra ký dùng framing tương tự. Trường giao thức được định nghĩa trong `worker_protocol.rs`; từ chối trường lạ. Không dùng giao thức này làm API mạng hoặc cho trình duyệt gửi đường dẫn module.

PIN chỉ truyền qua anonymous pipe, không ghi ra file, argv, môi trường hoặc log. Buffer yêu cầu và trường PIN được xóa khi giải phóng; worker chuyển PIN vào kiểu bí mật của thư viện trước đăng nhập. Adapter ứng dụng giữ PIN trong bộ nhớ có xóa khi giải phóng trong suốt đời identity để phục vụ các lần ký; không lưu PIN xuống đĩa. `open_token` cho backend worker chuẩn bị identity nhưng chưa xác thực PIN; xác thực chỉ xảy ra ở lần ký thật.

Mỗi yêu cầu ký có thời hạn 30 giây và không tự retry PIN. Trước đăng nhập, worker kiểm tra digest, fingerprint và thời hạn chứng thư, trạng thái khóa PIN và cơ chế ký. Cả worker lẫn phía gọi xác minh chữ ký bằng đúng chứng thư đã chọn trước khi chấp nhận kết quả. Bước xác minh này không thay thế kiểm tra chuỗi tin cậy/thu hồi. Ký digest cần cơ chế RSA PKCS#1 v1.5 thô của token; token chỉ có cơ chế hash-and-sign chưa được backend này hỗ trợ.

Mỗi yêu cầu hiện dùng một tiến trình và một phiên đăng nhập riêng, không phải phiên xác thực liên tục. Chưa benchmark, chưa tăng giới hạn đồng thời và chưa kiểm chứng ký USB thật. Nếu bị timeout hoặc tiến trình lỗi, không được suy ra thao tác trên token chắc chắn chưa xảy ra và không tự gửi lại yêu cầu ký.
