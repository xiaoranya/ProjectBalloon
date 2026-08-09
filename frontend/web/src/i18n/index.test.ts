import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { currentLocale, setLocale, translate, useI18n } from '.';
import { contestStatusLabel, submissionStatusLabel } from '../utils/format';

describe('application i18n', () => {
  beforeAll(() => {
    const values = new Map<string, string>();
    Object.defineProperty(window, 'localStorage', {
      configurable: true,
      value: {
        getItem: (key: string) => values.get(key) ?? null,
        setItem: (key: string, value: string) => values.set(key, value),
        removeItem: (key: string) => values.delete(key),
        clear: () => values.clear(),
      },
    });
  });
  afterEach(() => setLocale('zh-CN'));

  it('defaults to Chinese and interpolates translated English messages', () => {
    setLocale('zh-CN');
    expect(translate('欢迎回来，{name}', { name: 'Alice' })).toBe('欢迎回来，Alice');

    setLocale('en');
    expect(translate('欢迎回来，{name}', { name: 'Alice' })).toBe('Welcome back, Alice');
    expect(contestStatusLabel('RUNNING')).toBe('Running');
    expect(submissionStatusLabel('ACCEPTED')).toBe('Accepted');
  });

  it('persists the locale and exposes the matching Element Plus locale', () => {
    setLocale('en');
    expect(currentLocale()).toBe('en');
    expect(document.documentElement.lang).toBe('en');
    expect(localStorage.getItem('project-balloon-locale')).toBe('en');
    expect(useI18n().elementLocale.value.name).toBe('en');
  });

  it('provides English resources for every literal Chinese translation call', () => {
    const sources = import.meta.glob('../**/*.{ts,vue}', {
      eager: true,
      query: '?raw',
      import: 'default',
    }) as Record<string, string>;
    const keys = new Set<string>();
    for (const source of Object.values(sources)) {
      for (const match of source.matchAll(/\bt\(\s*'((?:\\.|[^'])*)'/g)) {
        keys.add(match[1].replaceAll("\\'", "'"));
      }
    }

    setLocale('en');
    const missing = [...keys].filter(
      (key) => /[\u3400-\u9fff]/.test(key) && translate(key) === key,
    );
    expect(missing).toEqual([]);
  });
});
