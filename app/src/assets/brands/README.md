# Logo nhà cung cấp CA và hãng token

Logo của 26 CA công cộng được chép nguyên từ [`assets/ca-logos`](../../../../assets/ca-logos/README.md)
(nguồn NEAC, không chỉnh sửa), tên file theo `provider_id` trong `manifest.json`. `BrandMark` tự
nhận file khi build; chưa có file thì hiện monogram theo màu thương hiệu (`src/lib/brands.ts`).

Mức hỗ trợ của từng CA (`support` trong `caBrands`) lấy theo [ETOKEN-SUPPORT.md](../../../../ETOKEN-SUPPORT.md):
`verified` = đã đọc chứng thư trên USB thật, `driver` = đã có bộ kết nối nhưng chưa kiểm chứng,
`none` = chưa có bộ kết nối. Khi kiểm chứng thêm CA, cập nhật cả hai nơi.

Logo thuộc các chủ sở hữu tương ứng; cần rà soát quyền sử dụng trước khi đóng gói phát hành.

Hãng token chưa có logo — thả file (SVG ưu tiên, hoặc PNG/WebP/JPG, vuông) với đúng tên:

| File | Thương hiệu |
|---|---|
| `securemetric.svg` | SecureMetric (token) |
| `feitian.svg` | Feitian / FT ePass (token) |
| `safenet.svg` | Thales SafeNet / Gemalto (token) |
| `watchdata.svg` | Watchdata (token) |
| `longmai.svg` | Longmai mToken (token) |
| `bit4id.svg` | Bit4id (token) |

Thêm thương hiệu mới: thêm một dòng vào `caBrands` hoặc `tokenBrands` trong `src/lib/brands.ts`.
