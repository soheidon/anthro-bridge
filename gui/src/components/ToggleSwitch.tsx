interface ToggleSwitchProps {
    checked: boolean;
    onChange: (checked: boolean) => void;
    label: string;
    description?: string;
    disabled?: boolean;
    layout?: "row" | "inline";
    className?: string;
}

export function ToggleSwitch({
    checked,
    onChange,
    label,
    description,
    disabled = false,
    layout = "row",
    className = "",
}: ToggleSwitchProps) {
    const handleToggle = () => {
        if (!disabled) {
            onChange(!checked);
        }
    };

    if (layout === "inline") {
        return (
            <div className={`toggle-switch-inline-wrapper ${className}`.trim()}>
                {label && (
                    <span
                        className="toggle-switch-label toggle-switch-clickable"
                        onClick={handleToggle}
                    >
                        {label}
                    </span>
                )}
                <button
                    type="button"
                    className={`toggle-switch ${checked ? "toggle-switch-on" : ""}`}
                    role="switch"
                    aria-checked={checked}
                    aria-label={label || undefined}
                    disabled={disabled}
                    onClick={handleToggle}
                >
                    <span className="toggle-switch-knob" aria-hidden="true" />
                </button>
                {description && (
                    <span
                        className="toggle-switch-description toggle-switch-clickable"
                        onClick={handleToggle}
                    >
                        {description}
                    </span>
                )}
            </div>
        );
    }

    return (
        <div className={`toggle-switch-row ${className}`.trim()}>
            <div className="toggle-switch-text">
                <span
                    className="toggle-switch-label toggle-switch-clickable"
                    onClick={handleToggle}
                >
                    {label}
                </span>
                {description && <span className="toggle-switch-description">{description}</span>}
            </div>
            <button
                type="button"
                className={`toggle-switch ${checked ? "toggle-switch-on" : ""}`}
                role="switch"
                aria-checked={checked}
                aria-label={label || undefined}
                disabled={disabled}
                onClick={handleToggle}
            >
                <span className="toggle-switch-knob" aria-hidden="true" />
            </button>
        </div>
    );
}
