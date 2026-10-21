import {
  Combobox,
  ComboboxDisclosure,
  ComboboxItem,
  ComboboxPopover,
  ComboboxProvider,
} from '@ariakit/react/combobox';
import { Check, LoaderCircle, Search, X } from 'lucide-react';
import { useId, useMemo, useRef, useState } from 'react';

export interface ComboBoxOption {
  id: string;
  label: string;
  description?: string;
  keywords?: string[];
}

interface SearchableComboBoxProps {
  label: string;
  options: ComboBoxOption[];
  selectedIds: string[];
  onSelectionChange: (selectedIds: string[]) => void;
  mode?: 'single' | 'multiple';
  placeholder?: string;
  emptyMessage?: string;
  disabled?: boolean;
  loading?: boolean;
  clearable?: boolean;
  onCreate?: (label: string) => ComboBoxOption | undefined;
}

const MAX_VISIBLE_OPTIONS = 100;

export function SearchableComboBox({
  label,
  options,
  selectedIds,
  onSelectionChange,
  mode = 'single',
  placeholder = 'Search options…',
  emptyMessage = 'No matching options',
  disabled = false,
  loading = false,
  clearable = true,
  onCreate,
}: SearchableComboBoxProps) {
  const inputId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState(false);
  const [createdOptions, setCreatedOptions] = useState<ComboBoxOption[]>([]);
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const allOptions = useMemo(
    () => [
      ...options,
      ...createdOptions.filter(
        (created) => !options.some((option) => option.id === created.id),
      ),
    ],
    [createdOptions, options],
  );
  const selectedSet = useMemo(() => new Set(selectedIds), [selectedIds]);
  const optionById = useMemo(
    () => new Map(allOptions.map((option) => [option.id, option])),
    [allOptions],
  );
  const matchingOptions = useMemo(() => {
    if (!normalizedQuery) return allOptions;
    return allOptions.filter((option) =>
      [option.label, option.description, ...(option.keywords ?? [])]
        .filter(Boolean)
        .join(' ')
        .toLocaleLowerCase()
        .includes(normalizedQuery),
    );
  }, [allOptions, normalizedQuery]);
  const visibleOptions = matchingOptions.slice(0, MAX_VISIBLE_OPTIONS);
  const exactLabelExists = allOptions.some(
    (option) => option.label.toLocaleLowerCase() === normalizedQuery,
  );
  const canCreate = Boolean(onCreate && normalizedQuery && !exactLabelExists);
  const isUnavailable = disabled || loading;

  const applySelection = (value: string | string[]) => {
    const next = Array.isArray(value) ? value : value ? [value] : [];
    onSelectionChange(mode === 'single' ? next.slice(-1) : next);
    setQuery('');
    if (mode === 'single') setOpen(false);
  };

  const removeSelection = (id: string) => {
    onSelectionChange(selectedIds.filter((selectedId) => selectedId !== id));
    inputRef.current?.focus();
  };

  const createOption = () => {
    const created = onCreate?.(query.trim());
    if (!created) return;
    setCreatedOptions((current) => [
      ...current.filter((option) => option.id !== created.id),
      created,
    ]);
    const next =
      mode === 'multiple'
        ? [...selectedIds.filter((id) => id !== created.id), created.id]
        : [created.id];
    onSelectionChange(next);
    setQuery('');
    if (mode === 'single') setOpen(false);
  };

  const selectedValue =
    mode === 'multiple' ? selectedIds : (selectedIds[0] ?? '');

  return (
    <div className="combo-field" data-disabled={isUnavailable || undefined}>
      <label className="combo-label" htmlFor={inputId}>
        {label}
      </label>
      <ComboboxProvider
        inputValue={query}
        setInputValue={setQuery}
        selectedValue={selectedValue}
        setSelectedValue={applySelection}
        open={open}
        setOpen={setOpen}
        resetValueOnHide
        virtualFocus={false}
      >
        <div
          className="combo-control"
          role="group"
          aria-label={`${label} control`}
          onMouseDown={(event) => {
            if (isUnavailable) return;
            if ((event.target as HTMLElement).closest('button, input')) return;
            event.preventDefault();
            inputRef.current?.focus();
            setOpen(true);
          }}
        >
          <Search aria-hidden="true" size={14} />
          {mode === 'multiple' &&
            selectedIds.map((id) => {
              const option = optionById.get(id);
              if (!option) return null;
              return (
                <span className="combo-chip" key={id}>
                  {option.label}
                  <button
                    type="button"
                    aria-label={`Remove ${option.label}`}
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => removeSelection(id)}
                  >
                    <X size={11} />
                  </button>
                </span>
              );
            })}
          {mode === 'single' && selectedIds[0] && !query && (
            <span className="combo-single-value">
              {optionById.get(selectedIds[0])?.label ?? selectedIds[0]}
            </span>
          )}
          <Combobox
            ref={inputRef}
            id={inputId}
            className="combo-input"
            placeholder={
              selectedIds.length
                ? mode === 'multiple'
                  ? 'Add another…'
                  : ''
                : placeholder
            }
            disabled={isUnavailable}
            aria-busy={loading || undefined}
          />
          {loading ? (
            <LoaderCircle className="combo-spinner" aria-label="Loading" />
          ) : (
            <>
              {clearable && selectedIds.length > 0 && (
                <button
                  type="button"
                  className="combo-clear"
                  aria-label={`Clear ${label}`}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => {
                    onSelectionChange([]);
                    setQuery('');
                    inputRef.current?.focus();
                  }}
                  disabled={isUnavailable}
                >
                  <X size={13} />
                </button>
              )}
              <ComboboxDisclosure
                className="combo-disclosure"
                disabled={isUnavailable}
                aria-label={`Toggle ${label}`}
              />
            </>
          )}
        </div>
        <ComboboxPopover
          className="combo-popover"
          portal
          modal={false}
          sameWidth
          gutter={6}
          unmountOnHide
        >
          {visibleOptions.map((option) => (
            <ComboboxItem
              className="combo-option"
              key={option.id}
              value={option.id}
              setValueOnClick={false}
              resetValueOnSelect
            >
              <span>
                <strong>{option.label}</strong>
                {option.description && <small>{option.description}</small>}
              </span>
              {selectedSet.has(option.id) && (
                <Check aria-hidden="true" size={14} />
              )}
            </ComboboxItem>
          ))}
          {canCreate && (
            <ComboboxItem
              className="combo-option combo-create"
              value={`__create__:${query.trim()}`}
              selectValueOnClick={false}
              setValueOnClick={false}
              hideOnClick={mode === 'single'}
              onClick={createOption}
            >
              Create “{query.trim()}”
            </ComboboxItem>
          )}
          {!loading && visibleOptions.length === 0 && !canCreate && (
            <p className="combo-empty" role="status">
              {emptyMessage}
            </p>
          )}
          {matchingOptions.length > MAX_VISIBLE_OPTIONS && (
            <p className="combo-limit" role="status">
              Showing the first {MAX_VISIBLE_OPTIONS} results. Keep typing to
              narrow the list.
            </p>
          )}
        </ComboboxPopover>
      </ComboboxProvider>
    </div>
  );
}
