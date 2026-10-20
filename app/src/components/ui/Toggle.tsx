import { Switch } from "@heroui/react";

/** Bare on/off switch for the right side of a setting row. */
export function Toggle({
  label,
  isSelected,
  isDisabled,
  onChange,
}: {
  label: string;
  isSelected: boolean;
  isDisabled?: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <Switch isSelected={isSelected} isDisabled={isDisabled} onChange={onChange}>
      {/* Switch.Content is the clickable SwitchButton; Switch itself is only the field. */}
      <Switch.Content aria-label={label}>
        <Switch.Control>
          <Switch.Thumb />
        </Switch.Control>
      </Switch.Content>
    </Switch>
  );
}
