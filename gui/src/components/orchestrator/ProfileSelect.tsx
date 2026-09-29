import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";

export interface ProfileSelectOption {
  id: string;
  provider: string;
  model: string;
  displayName?: string;
  badges?: string[];
}

interface ProfileSelectProps {
  label: string;
  placeholder: string;
  options: ProfileSelectOption[];
  value: string;
  selectedFallback?: ProfileSelectOption;
  disabled?: boolean;
  onChange: (profileId: string) => void;
}

export function ProfileSelect({
  label,
  placeholder,
  options,
  value,
  selectedFallback,
  disabled = false,
  onChange,
}: ProfileSelectProps) {
  const id = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const selectedIndex = options.findIndex((option) => option.id === value);
  const [activeIndex, setActiveIndex] = useState(selectedIndex >= 0 ? selectedIndex : 0);
  const selected = selectedIndex >= 0 ? options[selectedIndex] : selectedFallback;
  const accessibleValue = selected
    ? [selected.displayName ?? `${selected.provider} ${selected.model}`, ...(selected.badges ?? [])].join(", ")
    : placeholder;

  const groups = useMemo(() => {
    const grouped = new Map<string, Array<{ option: ProfileSelectOption; index: number }>>();
    options.forEach((option, index) => {
      const group = grouped.get(option.provider) ?? [];
      group.push({ option, index });
      grouped.set(option.provider, group);
    });
    return [...grouped.entries()];
  }, [options]);

  useEffect(() => {
    if (!open) return;
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (event.target instanceof Node && !rootRef.current?.contains(event.target)) {
        setOpen(false);
      }
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [open]);

  useEffect(() => {
    if (open && selectedIndex >= 0) setActiveIndex(selectedIndex);
  }, [open, selectedIndex]);

  const close = () => setOpen(false);
  const selectIndex = (index: number) => {
    const option = options[index];
    if (!option) return;
    onChange(option.id);
    close();
    triggerRef.current?.focus();
  };

  const openAt = (index: number) => {
    if (disabled || options.length === 0) return;
    setActiveIndex(Math.max(0, Math.min(options.length - 1, index)));
    setOpen(true);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key === "Tab") {
      close();
      return;
    }
    if (event.key === "Escape") {
      if (open) {
        event.preventDefault();
        close();
      }
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const direction = event.key === "ArrowDown" ? 1 : -1;
      if (!open) {
        openAt(selectedIndex >= 0 ? selectedIndex : direction > 0 ? 0 : options.length - 1);
      } else if (options.length > 0) {
        setActiveIndex((current) => (current + direction + options.length) % options.length);
      }
      return;
    }
    if (event.key === "Home" || event.key === "End") {
      if (open && options.length > 0) {
        event.preventDefault();
        setActiveIndex(event.key === "Home" ? 0 : options.length - 1);
      }
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (!open) {
        openAt(selectedIndex >= 0 ? selectedIndex : 0);
      } else {
        selectIndex(activeIndex);
      }
    }
  };

  return (
    <div className="orchestrator-profile-select" ref={rootRef}>
      <button
        ref={triggerRef}
        type="button"
        role="combobox"
        className="orchestrator-profile-select-trigger"
        aria-label={`${label}: ${accessibleValue}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={`${id}-listbox`}
        aria-activedescendant={open && options[activeIndex] ? `${id}-option-${activeIndex}` : undefined}
        disabled={disabled || options.length === 0}
        onClick={() => {
          if (open) close();
          else openAt(selectedIndex >= 0 ? selectedIndex : 0);
        }}
        onKeyDown={handleKeyDown}
      >
        <span className="orchestrator-profile-select-copy">
          {selected ? (
            <>
              <span className="orchestrator-profile-select-heading">
                {selected.displayName ? (
                  <span className="orchestrator-profile-model">{selected.displayName}</span>
                ) : (
                  <>
                    <span className="orchestrator-profile-provider">{selected.provider}</span>
                    <span className="orchestrator-profile-separator" aria-hidden="true">·</span>
                    <span className="orchestrator-profile-model">{selected.model}</span>
                  </>
                )}
              </span>
              {selected.badges && selected.badges.length > 0 && (
                <span className="orchestrator-profile-select-meta">
                  {selected.badges.map((badge) => <span className="orchestrator-profile-badge" key={badge}>{badge}</span>)}
                </span>
              )}
            </>
          ) : <span className="orchestrator-profile-select-placeholder">{placeholder}</span>}
        </span>
        <svg className="orchestrator-profile-select-chevron" viewBox="0 0 16 16" aria-hidden="true">
          <path d="m3.5 6 4.5 4 4.5-4" />
        </svg>
      </button>

      <div
        id={`${id}-listbox`}
        className="orchestrator-profile-select-menu"
        role="listbox"
        aria-label={label}
        hidden={!open}
      >
          {groups.map(([provider, entries]) => (
            <div className="orchestrator-profile-select-group" role="group" aria-label={provider} key={provider}>
              <div className="orchestrator-profile-select-group-label" aria-hidden="true">{provider}</div>
              {entries.map(({ option, index }) => (
                <div
                  id={`${id}-option-${index}`}
                  className={`orchestrator-profile-select-option ${index === activeIndex ? "active" : ""} ${option.id === value ? "selected" : ""}`}
                  role="option"
                  aria-selected={option.id === value}
                  key={option.id}
                  onMouseDown={(event) => event.preventDefault()}
                  onMouseMove={() => setActiveIndex(index)}
                  onClick={() => selectIndex(index)}
                >
                  <span className="orchestrator-profile-select-option-model">{option.displayName ?? option.model}</span>
                  {option.badges && option.badges.length > 0 && (
                    <span className="orchestrator-profile-select-meta">
                      {option.badges.map((badge) => <span className="orchestrator-profile-badge" key={badge}>{badge}</span>)}
                    </span>
                  )}
                  {option.id === value && <span className="orchestrator-profile-select-check" aria-hidden="true">✓</span>}
                </div>
              ))}
            </div>
          ))}
      </div>
    </div>
  );
}
