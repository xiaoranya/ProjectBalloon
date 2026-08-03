import type { ScreenGroupPlayback, ScreenViewTarget } from '../api/screen';

export interface ScreenPlaybackResolution {
  targetView: ScreenViewTarget | null;
  nextTargetView: ScreenViewTarget | null;
  msUntilNext: number | null;
}

export function resolveScreenPlayback(
  playback: ScreenGroupPlayback | null,
  serverNowMs: number,
): ScreenPlaybackResolution {
  if (!playback) return { targetView: null, nextTargetView: null, msUntilNext: null };
  if (playback.lockedView)
    return { targetView: playback.lockedView, nextTargetView: null, msUntilNext: null };
  if (playback.status === 'STOPPED' || !playback.items.length)
    return { targetView: null, nextTargetView: null, msUntilNext: null };
  const durations = playback.items.map((item) => item.durationSeconds * 1000);
  const total = durations.reduce((sum, value) => sum + value, 0);
  const elapsed =
    playback.status === 'PAUSED'
      ? playback.pausedElapsedSeconds * 1000
      : Math.max(0, serverNowMs - new Date(playback.startedAt ?? serverNowMs).getTime());
  const completed = !playback.loopEnabled && elapsed >= total;
  const position = completed ? Math.max(0, total - 1) : elapsed % total;
  let boundary = 0;
  let active = 0;
  for (let index = 0; index < durations.length; index += 1) {
    boundary += durations[index];
    if (position < boundary) {
      active = index;
      break;
    }
  }
  const targetView = playback.items[active].targetView;
  if (playback.status === 'PAUSED' || completed)
    return { targetView, nextTargetView: null, msUntilNext: null };
  const next = active + 1 < playback.items.length ? active + 1 : playback.loopEnabled ? 0 : -1;
  return {
    targetView,
    nextTargetView: next >= 0 ? playback.items[next].targetView : null,
    msUntilNext: Math.max(50, boundary - position),
  };
}
