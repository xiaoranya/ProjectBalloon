import { beforeEach, describe, expect, it, vi } from 'vitest';
import type * as monaco from 'monaco-editor/editor/editor.api';

const monacoStubs = vi.hoisted(() => ({
  register: vi.fn(() => ({ dispose: () => undefined })),
  Snippet: 15,
  Keyword: 14,
  Variable: 6,
  InsertAsSnippet: 4,
}));

vi.mock('monaco-editor/editor/editor.api', () => ({
  Range: class {
    constructor(
      public startLineNumber: number,
      public startColumn: number,
      public endLineNumber: number,
      public endColumn: number,
    ) {}
  },
  languages: {
    registerCompletionItemProvider: monacoStubs.register,
    CompletionItemKind: {
      Snippet: monacoStubs.Snippet,
      Keyword: monacoStubs.Keyword,
      Variable: monacoStubs.Variable,
    },
    CompletionItemInsertTextRule: { InsertAsSnippet: monacoStubs.InsertAsSnippet },
  },
}));

import { registerCompletionProviders } from './monacoCompletion';

type CapturedProvider = {
  language: string;
  triggerCharacters: string[];
  provideCompletionItems: (
    model: monaco.editor.ITextModel,
    position: monaco.Position,
  ) => { suggestions: Array<{ label: string; kind: number; insertText: string }> };
};

function providersByLanguage(): Map<string, CapturedProvider> {
  const byLanguage = new Map<string, CapturedProvider>();
  const calls = monacoStubs.register.mock.calls as unknown as Array<[string, CapturedProvider]>;
  for (const [language, provider] of calls) {
    byLanguage.set(language, provider);
  }
  return byLanguage;
}

function fakeModel(
  text: string,
  word: { startColumn: number; endColumn: number },
  typed: string,
): monaco.editor.ITextModel {
  return {
    getValue: () => text,
    getWordUntilPosition: () => word,
    getValueInRange: () => typed,
  } as unknown as monaco.editor.ITextModel;
}

const cursorAt = { lineNumber: 3, column: 6 } as monaco.Position;

describe('registerCompletionProviders', () => {
  it('registers exactly one provider per judge language and is idempotent', () => {
    registerCompletionProviders();
    registerCompletionProviders();
    registerCompletionProviders();

    expect(monacoStubs.register).toHaveBeenCalledTimes(6);
    const languages = providersByLanguage();
    expect([...languages.keys()].sort()).toEqual(['c', 'cpp', 'go', 'java', 'python', 'rust']);
    for (const provider of languages.values()) {
      expect(provider.triggerCharacters).toEqual(['.', ':', '#', '<']);
    }
  });

  it('c and cpp share the C++ snippet set while java and python differ', () => {
    registerCompletionProviders();
    const providers = providersByLanguage();

    const cpp = providers
      .get('cpp')!
      .provideCompletionItems(fakeModel('', { startColumn: 1, endColumn: 1 }, ''), cursorAt);
    const c = providers
      .get('c')!
      .provideCompletionItems(fakeModel('', { startColumn: 1, endColumn: 1 }, ''), cursorAt);
    const java = providers
      .get('java')!
      .provideCompletionItems(fakeModel('', { startColumn: 1, endColumn: 1 }, ''), cursorAt);
    const python = providers
      .get('python')!
      .provideCompletionItems(fakeModel('', { startColumn: 1, endColumn: 1 }, ''), cursorAt);
    const go = providers
      .get('go')!
      .provideCompletionItems(fakeModel('', { startColumn: 1, endColumn: 1 }, ''), cursorAt);
    const rust = providers
      .get('rust')!
      .provideCompletionItems(fakeModel('', { startColumn: 1, endColumn: 1 }, ''), cursorAt);

    const labels = (result: { suggestions: Array<{ label: string; kind: number }> }) =>
      result.suggestions.filter((s) => s.kind !== monacoStubs.Variable).map((s) => s.label);
    expect(labels(cpp)).toEqual(labels(c));
    expect(labels(cpp)).toContain('static_cast');
    expect(labels(java)).toContain('sysout');
    expect(labels(python)).toContain('for each');
    expect(labels(go)).toContain('fmt.Scan');
    expect(labels(rust)).toContain('read_line');
    expect(labels(java)).not.toContain('static_cast');
  });
});

describe('provideCompletionItems', () => {
  let cpp: CapturedProvider;

  beforeEach(() => {
    registerCompletionProviders();
    cpp = providersByLanguage().get('cpp')!;
  });

  it('returns snippets first, then unique document words of usable length', () => {
    const model = fakeModel(
      'int memo_table; memo_table repeated for x\nunique_name',
      { startColumn: 1, endColumn: 1 },
      '',
    );

    const { suggestions } = cpp.provideCompletionItems(model, cursorAt);
    const snippetCount = suggestions.filter((s) => s.kind !== monacoStubs.Variable).length;
    const words = suggestions.slice(snippetCount).map((s) => s.label);

    expect(words).toEqual(['memo_table', 'repeated', 'unique_name']);
    for (const suggestion of suggestions.slice(0, snippetCount)) {
      expect([monacoStubs.Snippet, monacoStubs.Keyword]).toContain(suggestion.kind);
    }
  });

  it('renders snippets with the insert-as-snippet rule and the typed range', () => {
    const range = { startColumn: 1, endColumn: 4 };
    const model = fakeModel('int main', range, 'int');
    const { suggestions } = cpp.provideCompletionItems(model, cursorAt);

    const main = suggestions.find((s) => s.label === 'main')!;
    expect(main.kind).toBe(monacoStubs.Snippet);
    expect(main.insertText).toContain('$1');
    expect(suggestions.every((s) => 'range' in s)).toBe(true);
  });

  it('drops the typed word, snippet labels, and single characters from document words', () => {
    const model = fakeModel(
      'int typed_word constexpr a for',
      { startColumn: 15, endColumn: 24 },
      'typed_word',
    );

    const { suggestions } = cpp.provideCompletionItems(model, cursorAt);
    const snippetCount = suggestions.filter((s) => s.kind !== monacoStubs.Variable).length;
    const words = suggestions.slice(snippetCount).map((s) => s.label);

    expect(words).toEqual(['constexpr']);
  });

  it('inserts at the cursor when there is no partial word', () => {
    const model = fakeModel('plain', { startColumn: 6, endColumn: 6 }, '');
    cpp.provideCompletionItems(model, cursorAt);
    const range = (
      cpp.provideCompletionItems(model, cursorAt) as unknown as {
        suggestions: Array<{ range: { startColumn: number; endColumn: number } }>;
      }
    ).suggestions[0].range;
    expect(range.startColumn).toBe(cursorAt.column);
    expect(range.endColumn).toBe(cursorAt.column);
  });

  it('caps document words at three hundred entries', () => {
    const manyWords = Array.from({ length: 350 }, (_, index) => `word_${index}`).join(' ');
    const model = fakeModel(manyWords, { startColumn: 1, endColumn: 1 }, '');

    const { suggestions } = cpp.provideCompletionItems(model, cursorAt);
    const snippetCount = suggestions.filter((s) => s.kind !== monacoStubs.Variable).length;
    expect(suggestions.length - snippetCount).toBe(300);
  });
});
