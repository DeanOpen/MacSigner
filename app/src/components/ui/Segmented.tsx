import { cn, Tabs } from "@heroui/react";

/** Segmented control built on HeroUI tabs (no panels). */
export function Segmented<T extends string>({
  label,
  value,
  onChange,
  items,
  fullWidth,
}: {
  label: string;
  value: T;
  onChange: (v: T) => void;
  items: { id: T; label: string }[];
  fullWidth?: boolean;
}) {
  return (
    <Tabs selectedKey={value} onSelectionChange={(k) => onChange(k as T)} className={fullWidth ? "w-full" : "w-auto"}>
      <Tabs.ListContainer>
        <Tabs.List aria-label={label} className={fullWidth ? "w-full" : undefined}>
          {items.map((it) => (
            <Tabs.Tab key={it.id} id={it.id} className={cn("px-3 text-[13px] whitespace-nowrap", fullWidth && "flex-1")}>
              {it.label}
              <Tabs.Indicator />
            </Tabs.Tab>
          ))}
        </Tabs.List>
      </Tabs.ListContainer>
    </Tabs>
  );
}
