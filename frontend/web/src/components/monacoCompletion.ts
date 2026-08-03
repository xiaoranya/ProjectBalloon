import * as monaco from 'monaco-editor/editor/editor.api';

type Snippet = {
  label: string;
  insertText: string;
  kind: monaco.languages.CompletionItemKind;
  detail?: string;
};

const CPP: Snippet[] = [
  {
    label: '#include',
    insertText: '#include <$1>',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'using namespace std;',
    insertText: 'using namespace std;',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'main',
    insertText: 'int main() {\n\t$1\n\treturn 0;\n}',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'for',
    insertText: 'for (int i = 0; i < $1; i++) {\n\t$2\n}',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'while',
    insertText: 'while ($1) {\n\t$2\n}',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'if',
    insertText: 'if ($1) {\n\t$2\n}',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'cout',
    insertText: 'std::cout << $1 << std::endl;',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'cin',
    insertText: 'std::cin >> $1;',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'vector',
    insertText: 'std::vector<$1> $2;',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'string',
    insertText: 'std::string $1;',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  { label: 'auto', insertText: 'auto', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'bool', insertText: 'bool', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'char', insertText: 'char', kind: monaco.languages.CompletionItemKind.Keyword },
  {
    label: 'class',
    insertText: 'class $1 {\n\t$2\n};',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  { label: 'const', insertText: 'const ', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'double', insertText: 'double', kind: monaco.languages.CompletionItemKind.Keyword },
  {
    label: 'else',
    insertText: 'else {\n\t$1\n}',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'enum',
    insertText: 'enum $1 {\n\t$2\n};',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  { label: 'float', insertText: 'float', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'int', insertText: 'int', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'long', insertText: 'long', kind: monaco.languages.CompletionItemKind.Keyword },
  {
    label: 'namespace',
    insertText: 'namespace $1 {\n\t$2\n}',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  { label: 'return', insertText: 'return $1;', kind: monaco.languages.CompletionItemKind.Snippet },
  { label: 'short', insertText: 'short', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'sizeof', insertText: 'sizeof($1)', kind: monaco.languages.CompletionItemKind.Snippet },
  { label: 'static', insertText: 'static ', kind: monaco.languages.CompletionItemKind.Keyword },
  {
    label: 'struct',
    insertText: 'struct $1 {\n\t$2\n};',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'switch',
    insertText: 'switch ($1) {\n\tcase $2:\n\t\t$3\n\t\tbreak;\n\tdefault:\n\t\t$4\n}',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'template',
    insertText: 'template <typename $1>\n$2',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'try',
    insertText: 'try {\n\t$1\n} catch (const std::exception& e) {\n\t$2\n}',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'typedef',
    insertText: 'typedef $1 $2;',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  { label: 'unsigned', insertText: 'unsigned', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'void', insertText: 'void', kind: monaco.languages.CompletionItemKind.Keyword },
  {
    label: 'static_cast',
    insertText: 'static_cast<$1>($2)',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'dynamic_cast',
    insertText: 'dynamic_cast<$1>($2)',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
];

const JAVA: Snippet[] = [
  {
    label: 'main',
    insertText: 'public static void main(String[] args) {\n\t$1\n}',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'class',
    insertText: 'class $1 {\n\t$2\n}',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'for',
    insertText: 'for (int i = 0; i < $1; i++) {\n\t$2\n}',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'while',
    insertText: 'while ($1) {\n\t$2\n}',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'if',
    insertText: 'if ($1) {\n\t$2\n}',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'else',
    insertText: 'else {\n\t$1\n}',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'sysout',
    insertText: 'System.out.println($1);',
    kind: monaco.languages.CompletionItemKind.Snippet,
    detail: 'System.out.println',
  },
  {
    label: 'Scanner',
    insertText: 'Scanner $1 = new Scanner(System.in);',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  { label: 'import', insertText: 'import $1;', kind: monaco.languages.CompletionItemKind.Snippet },
  { label: 'public', insertText: 'public ', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'private', insertText: 'private ', kind: monaco.languages.CompletionItemKind.Keyword },
  {
    label: 'protected',
    insertText: 'protected ',
    kind: monaco.languages.CompletionItemKind.Keyword,
  },
  { label: 'static', insertText: 'static ', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'final', insertText: 'final ', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'void', insertText: 'void', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'int', insertText: 'int', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'long', insertText: 'long', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'double', insertText: 'double', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'boolean', insertText: 'boolean', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'char', insertText: 'char', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'String', insertText: 'String', kind: monaco.languages.CompletionItemKind.Keyword },
  {
    label: 'ArrayList',
    insertText: 'ArrayList<$1> $2 = new ArrayList<>();',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'HashMap',
    insertText: 'HashMap<$1, $2> $3 = new HashMap<>();',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'HashSet',
    insertText: 'HashSet<$1> $2 = new HashSet<>();',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'try',
    insertText: 'try {\n\t$1\n} catch (Exception e) {\n\t$2\n}',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  { label: 'return', insertText: 'return $1;', kind: monaco.languages.CompletionItemKind.Snippet },
  { label: 'new', insertText: 'new $1()', kind: monaco.languages.CompletionItemKind.Snippet },
  { label: 'throws', insertText: 'throws $1', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'this', insertText: 'this', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'super', insertText: 'super()', kind: monaco.languages.CompletionItemKind.Keyword },
];

const PYTHON: Snippet[] = [
  {
    label: 'def',
    insertText: 'def $1($2):\n\t$3',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'main',
    insertText: 'def main():\n\t$1\n\n\nif __name__ == "__main__":\n\tmain()',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'for',
    insertText: 'for $1 in range($2):\n\t$3',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'for each',
    insertText: 'for $1 in $2:\n\t$3',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'while',
    insertText: 'while $1:\n\t$2',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  { label: 'if', insertText: 'if $1:\n\t$2', kind: monaco.languages.CompletionItemKind.Snippet },
  {
    label: 'elif',
    insertText: 'elif $1:\n\t$2',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  { label: 'else', insertText: 'else:\n\t$1', kind: monaco.languages.CompletionItemKind.Snippet },
  { label: 'print', insertText: 'print($1)', kind: monaco.languages.CompletionItemKind.Snippet },
  { label: 'input', insertText: 'input($1)', kind: monaco.languages.CompletionItemKind.Snippet },
  { label: 'import', insertText: 'import $1', kind: monaco.languages.CompletionItemKind.Snippet },
  {
    label: 'from',
    insertText: 'from $1 import $2',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'class',
    insertText: 'class $1:\n\tdef __init__(self$2):\n\t\t$3',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'try',
    insertText: 'try:\n\t$1\nexcept $2:\n\t$3',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  {
    label: 'with',
    insertText: 'with open($1) as $2:\n\t$3',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  { label: 'return', insertText: 'return $1', kind: monaco.languages.CompletionItemKind.Snippet },
  { label: 'True', insertText: 'True', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'False', insertText: 'False', kind: monaco.languages.CompletionItemKind.Keyword },
  { label: 'None', insertText: 'None', kind: monaco.languages.CompletionItemKind.Keyword },
  {
    label: 'lambda',
    insertText: 'lambda $1: $2',
    kind: monaco.languages.CompletionItemKind.Snippet,
  },
  { label: 'len', insertText: 'len($1)', kind: monaco.languages.CompletionItemKind.Snippet },
  { label: 'range', insertText: 'range($1)', kind: monaco.languages.CompletionItemKind.Snippet },
  { label: 'int', insertText: 'int($1)', kind: monaco.languages.CompletionItemKind.Snippet },
  { label: 'str', insertText: 'str($1)', kind: monaco.languages.CompletionItemKind.Snippet },
  { label: 'list', insertText: '[$1]', kind: monaco.languages.CompletionItemKind.Snippet },
  { label: 'dict', insertText: '{$1: $2}', kind: monaco.languages.CompletionItemKind.Snippet },
];

function wordRange(model: monaco.editor.ITextModel, position: monaco.Position): monaco.IRange {
  const word = model.getWordUntilPosition(position);
  if (word.startColumn !== word.endColumn) {
    return new monaco.Range(
      position.lineNumber,
      word.startColumn,
      position.lineNumber,
      position.column,
    );
  }
  return new monaco.Range(
    position.lineNumber,
    position.column,
    position.lineNumber,
    position.column,
  );
}

const DOC_WORD_RE = /[\p{L}_][\p{L}\d_]*/gu;
const MAX_DOC_WORDS = 300;

function documentWords(model: monaco.editor.ITextModel): string[] {
  const words = new Set<string>();
  const text = model.getValue();
  const matches = text.matchAll(DOC_WORD_RE);
  for (const match of matches) {
    words.add(match[0]);
    if (words.size >= MAX_DOC_WORDS) break;
  }
  return [...words];
}

function makeProvider(snippets: Snippet[]): monaco.languages.CompletionItemProvider {
  const snippetLabels = new Set(snippets.map((s) => s.label));
  return {
    triggerCharacters: ['.', ':', '#', '<'],
    provideCompletionItems(model, position) {
      const range = wordRange(model, position);
      const typed = model.getValueInRange(range);
      const words = documentWords(model).filter(
        (w) => w !== typed && !snippetLabels.has(w) && w.length >= 2,
      );
      return {
        suggestions: [
          ...snippets.map((s) => ({
            label: s.label,
            kind: s.kind,
            detail: s.detail,
            insertText: s.insertText,
            insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            range,
          })),
          ...words.map((w) => ({
            label: w,
            kind: monaco.languages.CompletionItemKind.Variable,
            insertText: w,
            range,
          })),
        ],
      };
    },
  };
}

let registered = false;

export function registerCompletionProviders(): void {
  if (registered) return;
  registered = true;
  monaco.languages.registerCompletionItemProvider('cpp', makeProvider(CPP));
  monaco.languages.registerCompletionItemProvider('java', makeProvider(JAVA));
  monaco.languages.registerCompletionItemProvider('python', makeProvider(PYTHON));
}
