import { describe, expect, it } from 'vitest';
import { resolveScreenPlayback } from './screenPlayback';
import type { ScreenGroupPlayback } from '../api/screen';
const playback: ScreenGroupPlayback = {
  groupId: 1,
  groupName: 'Hall',
  playlistId: 2,
  loopEnabled: true,
  status: 'PLAYING',
  startedAt: '2026-07-22T00:00:00Z',
  pausedElapsedSeconds: 0,
  lockedView: null,
  version: 1,
  items: [
    { id: 1, targetView: 'SCOREBOARD', durationSeconds: 10, displayOrder: 1 },
    { id: 2, targetView: 'AWARDS', durationSeconds: 5, displayOrder: 2 },
  ],
};
describe('screen playback timeline', () => {
  it('uses the shared server timeline and loops', () => {
    expect(resolveScreenPlayback(playback, Date.parse('2026-07-22T00:00:12Z')).targetView).toBe(
      'AWARDS',
    );
    expect(resolveScreenPlayback(playback, Date.parse('2026-07-22T00:00:16Z')).targetView).toBe(
      'SCOREBOARD',
    );
  });
  it('lets lock override playback and freezes paused playback', () => {
    expect(
      resolveScreenPlayback({ ...playback, lockedView: 'RESOLVER' }, Date.now()).targetView,
    ).toBe('RESOLVER');
    expect(
      resolveScreenPlayback(
        { ...playback, status: 'PAUSED', pausedElapsedSeconds: 12 },
        Date.now(),
      ),
    ).toMatchObject({ targetView: 'AWARDS', nextTargetView: null });
  });
});
