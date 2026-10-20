import { cn, Description, Input, InputGroup, Label, ListBox, Select, TextField } from "@heroui/react";
import { Eye, EyeOff } from "lucide-react";
import { useState, type ReactNode } from "react";

/** Labelled single-line text input. `hideLabel` keeps the label for screen readers only. */
export function TextInput({
  label,
  hideLabel,
  description,
  value,
  defaultValue,
  placeholder,
  onChange,
  onBlur,
  className,
  mono,
  autoFocus,
}: {
  label: string;
  hideLabel?: boolean;
  description?: ReactNode;
  value?: string;
  defaultValue?: string;
  placeholder?: string;
  onChange?: (v: string) => void;
  onBlur?: (v: string) => void;
  className?: string;
  mono?: boolean;
  autoFocus?: boolean;
}) {
  return (
    // min-w-0: an input's intrinsic width must not push its row (e.g. input + "Chọn…" button) wider than the dialog.
    <TextField className={cn("min-w-0", className)} value={value} defaultValue={defaultValue} onChange={onChange} autoFocus={autoFocus}>
      <Label className={hideLabel ? "sr-only" : undefined}>{label}</Label>
      <Input
        placeholder={placeholder}
        className={cn("w-full min-w-0", mono && "font-mono text-xs")}
        onBlur={onBlur ? (e) => onBlur(e.currentTarget.value) : undefined}
      />
      {description && <Description>{description}</Description>}
    </TextField>
  );
}

/** Password / PIN input with a show-hide toggle. */
export function SecretInput({
  label,
  value,
  onChange,
  showLabel,
  hideLabel,
  autoFocus,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  showLabel: string;
  hideLabel: string;
  autoFocus?: boolean;
}) {
  const [visible, setVisible] = useState(false);
  return (
    <TextField value={value} onChange={onChange} type={visible ? "text" : "password"} autoFocus={autoFocus}>
      <Label>{label}</Label>
      <InputGroup>
        <InputGroup.Input autoComplete="off" />
        <InputGroup.Suffix>
          <button
            type="button"
            aria-label={visible ? hideLabel : showLabel}
            className="grid size-7 cursor-pointer place-items-center rounded-md text-muted hover:text-foreground"
            onClick={() => setVisible((v) => !v)}
          >
            {visible ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
          </button>
        </InputGroup.Suffix>
      </InputGroup>
    </TextField>
  );
}

/** Single-choice dropdown. */
export function SelectField<T extends string | number>({
  label,
  hideLabel,
  value,
  onChange,
  options,
  className,
}: {
  label: string;
  hideLabel?: boolean;
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: string }[];
  className?: string;
}) {
  return (
    <Select className={className} selectedKey={value} onSelectionChange={(k) => k != null && onChange(k as T)} aria-label={hideLabel ? label : undefined}>
      {!hideLabel && <Label>{label}</Label>}
      <Select.Trigger>
        <Select.Value />
        <Select.Indicator />
      </Select.Trigger>
      <Select.Popover>
        <ListBox>
          {options.map((o) => (
            <ListBox.Item key={String(o.value)} id={o.value} textValue={o.label}>
              {o.label}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
    </Select>
  );
}
