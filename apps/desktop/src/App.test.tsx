// @vitest-environment jsdom

import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { App } from './App';

describe('library batch assignment', () => {
  it('assigns multiple selected fixture items to another project by stable ID', async () => {
    const user = userEvent.setup();
    const { container } = render(<App />);

    await user.click(
      screen.getByRole('group', { name: 'Assign projects control' }),
    );
    await user.click(screen.getByRole('option', { name: 'Workshop Stories' }));

    const cards = container.querySelectorAll('article');
    expect(within(cards[0]).getByText('Workshop Stories')).toBeVisible();
    expect(within(cards[1]).getByText('Workshop Stories')).toBeVisible();
    expect(within(cards[0]).getByText('Simple Vlog 055')).toBeVisible();
  });

  it('offers explicit tag creation without browser prompts', async () => {
    const user = userEvent.setup();
    const { container } = render(<App />);

    await user.type(
      screen.getByRole('combobox', { name: 'Add tags' }),
      'Night',
    );
    await user.click(screen.getByRole('option', { name: 'Create “Night”' }));

    const cards = container.querySelectorAll('article');
    expect(within(cards[0]).getByText('Night')).toBeVisible();
    expect(within(cards[1]).getByText('Night')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Remove Night' })).toBeVisible();
  });
});
