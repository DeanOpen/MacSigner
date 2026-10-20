import type { ComponentType } from "react";
import type { Brand } from "../lib/brands";

// Official logos dropped into src/assets/brands/<key>.svg|png|jpg are picked up here.
const files = import.meta.glob<string>("../assets/brands/*.{svg,png,webp,jpg,jpeg}", {
  eager: true,
  query: "?url",
  import: "default",
});
export const logo = (key: string) =>
  Object.entries(files).find(([path]) => path.replace(/^.*\/|\.\w+$/g, "") === key)?.[1];

const iconSize = { xs: 10, sm: 12, md: 16, lg: 18 };

/** A brand's logo, its monogram, or a fallback icon when the brand is unknown. */
export function BrandMark({
  brand,
  fallback: Fallback,
  size = "md",
}: {
  brand?: Brand;
  fallback: ComponentType<{ size?: number }>;
  size?: "xs" | "sm" | "md" | "lg";
}) {
  const cls = `brand brand--${size}`;
  if (!brand)
    return (
      <span className={`${cls} brand--icon`}>
        <Fallback size={iconSize[size]} />
      </span>
    );
  const src = logo(brand.key);
  if (src)
    return (
      <span className={`${cls} brand--logo`} title={brand.name}>
        <img src={src} alt={brand.name} />
      </span>
    );
  return (
    <span
      className={`${cls} brand--mono${brand.short.length > 2 ? " brand--long" : ""}`}
      style={{ "--brand": brand.color } as React.CSSProperties}
      title={brand.name}
      aria-label={brand.name}
    >
      {brand.short}
    </span>
  );
}

/** Windows logo (four panes) for the Windows certificate store. */
export function WindowsIcon({ size = 16 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" aria-hidden="true" fill="currentColor">
      <path d="M1 2.6 6.7 1.8v5.6H1zM7.4 1.7 15 .6v6.8H7.4zM1 8.1h5.7v5.6L1 12.9zM7.4 8.1H15v6.8l-7.6-1.1z" />
    </svg>
  );
}
