interface ToggleSwitchProps {
    checked: boolean;
    onChange: (checked: boolean) => void;
    label: string;
    description?: string;
    disabled?: boolean;
}

export function ToggleSwitch({ checked, onChange, label, description, disabled = false }: ToggleSwitchProps) {
    return (
        <div className="toggle-switch-row">
            <div className="toggle-switch-text">
                <span className="toggle-switch-label">{label}</span>
                {description && <span className="toggle-switch-description">{description}</span>}
            </div>
            <button
                type="button"
                className={`toggle-switch ${checked ? "toggle-switch-on" : ""}`}
                role="switch"
                aria-checked={checked}
                aria-label={label}
                disabled={disabled}
                onClick={() => onChange(!checked)}
            >
                <span className="toggle-switch-knob" aria-hidden="true" />
            </button>
        </div>
    );
}
