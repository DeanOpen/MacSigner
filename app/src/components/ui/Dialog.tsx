import { Modal } from "@heroui/react";
import type { ComponentProps, ReactNode } from "react";

/** Controlled modal: heading, optional description, body and footer actions. */
export function Dialog({
  isOpen,
  onClose,
  title,
  description,
  children,
  footer,
  size = "lg",
  className,
  isDismissable = true,
}: {
  isOpen: boolean;
  onClose: () => void;
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
  /** HeroUI sizes are in rem; with the 14px root, "lg" is about 450px. */
  size?: ComponentProps<typeof Modal.Container>["size"];
  /** Extra classes on the dialog, e.g. a wider max-width for result lists. */
  className?: string;
  isDismissable?: boolean;
}) {
  // Controlled without a trigger: the open state goes on the backdrop (a Modal root expects a pressable trigger).
  return (
    <Modal.Backdrop
      isOpen={isOpen}
      onOpenChange={(open) => !open && isDismissable && onClose()}
      isDismissable={isDismissable}
      isKeyboardDismissDisabled={!isDismissable}
    >
      <Modal.Container size={size}>
        <Modal.Dialog className={className}>
          {isDismissable && <Modal.CloseTrigger aria-label="Đóng" />}
          <Modal.Header>
            <Modal.Heading>{title}</Modal.Heading>
            {description && <p className="text-[13px] text-muted">{description}</p>}
          </Modal.Header>
          {children && <Modal.Body className="text-[13px]">{children}</Modal.Body>}
          {footer && <Modal.Footer>{footer}</Modal.Footer>}
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  );
}
