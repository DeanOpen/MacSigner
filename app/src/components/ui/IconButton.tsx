import { Button, Tooltip } from "@heroui/react";
import type { ComponentProps, ReactNode } from "react";

type Placement = ComponentProps<typeof Tooltip.Content>["placement"];

/** Icon-only button with its label as tooltip and accessible name. */
export function IconButton({
  label,
  children,
  variant = "ghost",
  size = "sm",
  placement = "bottom",
  ...props
}: Omit<ComponentProps<typeof Button>, "children" | "isIconOnly"> & { label: string; children: ReactNode; placement?: Placement }) {
  return (
    <Tooltip delay={400} closeDelay={0}>
      <Button isIconOnly aria-label={label} variant={variant} size={size} {...props}>
        {children}
      </Button>
      <Tooltip.Content placement={placement}>{label}</Tooltip.Content>
    </Tooltip>
  );
}
