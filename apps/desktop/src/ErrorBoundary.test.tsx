// @vitest-environment jsdom
import { render, screen } from '@testing-library/react';
import { vi, expect, it } from 'vitest';

const { logUiError } = vi.hoisted(() => ({ logUiError: vi.fn() }));
vi.mock('./bridge', () => ({
  nativeAvailable: () => true,
  desktopBridge: { logUiError },
}));

import { ErrorBoundary } from './ErrorBoundary';

function BrokenView(): never {
  throw new Error('simulated render failure');
}

it('renders a recoverable interface and records render failures', () => {
  vi.spyOn(console, 'error').mockImplementation(() => undefined);
  render(
    <ErrorBoundary>
      <BrokenView />
    </ErrorBoundary>,
  );
  expect(
    screen.getByRole('heading', {
      name: 'PIMP could not display the interface',
    }),
  ).toBeVisible();
  expect(
    screen.getByRole('button', { name: 'Reload interface' }),
  ).toBeVisible();
  expect(logUiError).toHaveBeenCalledWith(
    expect.stringContaining('simulated render failure'),
  );
});
