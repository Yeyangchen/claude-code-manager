import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { ReleaseNotes } from './components';
import { createApi } from '../test/fixtures';
import { safeHttpsUrl } from '../services/manager';

describe('untrusted release notes', () => {
  it('renders no executable HTML, remote images, or non-HTTPS links', () => {
    const api = createApi();
    const { container } = render(<ReleaseNotes api={api} onError={vi.fn()} content={'<script>alert(1)</script>\n<img src="https://example.com/tracker.png" onerror="alert(1)">\n\n[bad](javascript:alert) [local](file:///c:/secret) ![remote](https://example.com/image.png)'} />);
    expect(container.querySelector('script')).toBeNull();
    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelector('a')).toBeNull();
    expect(screen.queryByRole('button')).toBeNull();
    expect(api.openUrl).not.toHaveBeenCalled();
  });

  it('opens a safe source through the backend only after clicking', async () => {
    const api = createApi();
    const user = userEvent.setup();
    render(<ReleaseNotes api={api} onError={vi.fn()} content="[更新日志](https://github.com/anthropics/claude-code)" />);
    expect(api.openUrl).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: '更新日志' }));
    expect(api.openUrl).toHaveBeenCalledWith('https://github.com/anthropics/claude-code');
  });

  it.each(['http://example.com', 'javascript:alert(1)', 'file:///C:/Windows', 'https://user:secret@example.com', '//example.com'])('rejects unsafe URL %s', (url) => {
    expect(safeHttpsUrl(url)).toBeNull();
  });
});
