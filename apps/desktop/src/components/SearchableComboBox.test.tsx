// @vitest-environment jsdom

import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { type ComboBoxOption, SearchableComboBox } from './SearchableComboBox';

const options: ComboBoxOption[] = [
  { id: 'project-1', label: 'Simple Vlog 055' },
  { id: 'project-2', label: 'Workshop Stories' },
  { id: 'project-3', label: 'Halloween Special' },
];

function Harness({
  mode = 'single',
  initial = [],
  comboOptions = options,
  disabled = false,
  loading = false,
  onCreate,
}: {
  mode?: 'single' | 'multiple';
  initial?: string[];
  comboOptions?: ComboBoxOption[];
  disabled?: boolean;
  loading?: boolean;
  onCreate?: (label: string) => ComboBoxOption | undefined;
}) {
  const [selectedIds, setSelectedIds] = useState(initial);
  return (
    <>
      <SearchableComboBox
        label="Projects"
        mode={mode}
        options={comboOptions}
        selectedIds={selectedIds}
        onSelectionChange={setSelectedIds}
        disabled={disabled}
        loading={loading}
        onCreate={onCreate}
      />
      <output data-testid="selection">{selectedIds.join(',')}</output>
      <button type="button">Outside</button>
    </>
  );
}

describe('SearchableComboBox', () => {
  it('opens from the full control and filters case-insensitively', async () => {
    const user = userEvent.setup();
    render(<Harness />);

    await user.click(screen.getByRole('group', { name: 'Projects control' }));
    expect(
      screen.getByRole('option', { name: 'Simple Vlog 055' }),
    ).toBeVisible();

    await user.type(screen.getByRole('combobox', { name: 'Projects' }), 'WORK');
    expect(
      screen.getByRole('option', { name: 'Workshop Stories' }),
    ).toBeVisible();
    expect(
      screen.queryByRole('option', { name: 'Simple Vlog 055' }),
    ).toBeNull();
  });

  it('selects and clears a value', async () => {
    const user = userEvent.setup();
    render(<Harness />);

    await user.click(screen.getByRole('combobox', { name: 'Projects' }));
    await user.click(screen.getByRole('option', { name: 'Workshop Stories' }));
    expect(screen.getByTestId('selection')).toHaveTextContent('project-2');
    expect(
      screen
        .getByRole('group', { name: 'Projects control' })
        .querySelector('.combo-single-value'),
    ).toHaveTextContent('Workshop Stories');

    await user.click(screen.getByRole('button', { name: 'Clear Projects' }));
    expect(screen.getByTestId('selection')).toBeEmptyDOMElement();
    expect(screen.getByRole('combobox', { name: 'Projects' })).toHaveFocus();
  });

  it('supports keyboard-only selection and Escape focus restoration', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const input = screen.getByRole('combobox', { name: 'Projects' });

    await user.click(input);
    await user.keyboard('{ArrowDown}{Enter}');
    expect(screen.getByTestId('selection')).toHaveTextContent('project-1');

    await user.click(input);
    await user.keyboard('{Escape}');
    await waitFor(() =>
      expect(input).toHaveAttribute('aria-expanded', 'false'),
    );
    expect(input).toHaveFocus();
  });

  it('closes when focus moves to an outside click target', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const input = screen.getByRole('combobox', { name: 'Projects' });

    await user.click(input);
    expect(input).toHaveAttribute('aria-expanded', 'true');
    await user.click(screen.getByRole('button', { name: 'Outside' }));
    await waitFor(() =>
      expect(input).toHaveAttribute('aria-expanded', 'false'),
    );
  });

  it('reports empty, disabled, and loading states', async () => {
    const user = userEvent.setup();
    const { rerender } = render(<Harness comboOptions={[]} />);

    await user.click(screen.getByRole('combobox', { name: 'Projects' }));
    expect(screen.getByText('No matching options')).toBeVisible();

    rerender(<Harness disabled />);
    expect(screen.getByRole('combobox', { name: 'Projects' })).toBeDisabled();

    rerender(<Harness loading />);
    expect(screen.getByRole('combobox', { name: 'Projects' })).toHaveAttribute(
      'aria-busy',
      'true',
    );
    expect(screen.getByLabelText('Loading')).toBeInTheDocument();
  });

  it('keeps multiple selections and distinguishes duplicate labels by ID', async () => {
    const user = userEvent.setup();
    render(
      <Harness
        mode="multiple"
        comboOptions={[
          { id: 'camera-a', label: 'Camera', description: 'Sony A7' },
          { id: 'camera-b', label: 'Camera', description: 'DJI Pocket 3' },
        ]}
      />,
    );
    const input = screen.getByRole('combobox', { name: 'Projects' });

    await user.click(input);
    const optionA = screen.getByText('Sony A7').closest('[role="option"]');
    expect(optionA).not.toBeNull();
    await user.click(optionA!);
    const optionB = screen.getByText('DJI Pocket 3').closest('[role="option"]');
    expect(optionB).not.toBeNull();
    await user.click(optionB!);

    expect(screen.getByTestId('selection')).toHaveTextContent(
      'camera-a,camera-b',
    );
    expect(
      screen.getAllByRole('button', { name: 'Remove Camera' }),
    ).toHaveLength(2);
  });

  it('creates only through the explicit create option', async () => {
    const user = userEvent.setup();
    const onCreate = vi.fn((label: string) => ({
      id: `created:${label}`,
      label,
    }));
    render(<Harness mode="multiple" onCreate={onCreate} />);
    const input = screen.getByRole('combobox', { name: 'Projects' });

    await user.type(input, 'New Project');
    expect(screen.getByTestId('selection')).toBeEmptyDOMElement();
    await user.click(
      screen.getByRole('option', { name: 'Create “New Project”' }),
    );

    expect(onCreate).toHaveBeenCalledWith('New Project');
    expect(screen.getByTestId('selection')).toHaveTextContent(
      'created:New Project',
    );
  });

  it('exposes combobox and listbox semantics', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const input = screen.getByRole('combobox', { name: 'Projects' });
    expect(input).toHaveAttribute('aria-expanded', 'false');

    await user.click(input);
    expect(input).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByRole('listbox')).toBeVisible();
    expect(
      within(screen.getByRole('listbox')).getAllByRole('option'),
    ).toHaveLength(3);
  });

  it('caps large rendered result sets until the query narrows them', async () => {
    const user = userEvent.setup();
    const manyOptions = Array.from({ length: 150 }, (_, index) => ({
      id: `option-${index}`,
      label: `Option ${index}`,
    }));
    render(<Harness comboOptions={manyOptions} />);

    await user.click(screen.getByRole('combobox', { name: 'Projects' }));
    expect(screen.getAllByRole('option')).toHaveLength(100);
    expect(screen.getByText(/Showing the first 100 results/)).toBeVisible();
  });
});
