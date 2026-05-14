import { useCallback, useEffect, useRef } from 'react';
import { EditorState } from '@codemirror/state';
import { EditorView, lineNumbers, highlightActiveLine, highlightSpecialChars, placeholder as cmPlaceholder } from '@codemirror/view';
import { bracketMatching } from '@codemirror/language';
import { json, jsonParseLinter } from '@codemirror/lang-json';
import { linter } from '@codemirror/lint';

export interface CodeEditorProps {
  value: string;
  onChange: (value: string) => void;
  /** 'json' 时启用 JSON 模式（括号匹配、折叠、linter） */
  language?: 'json';
  placeholder?: string;
  readOnly?: boolean;
  className?: string;
}

const editorTheme = EditorView.theme({
  '&': {
    height: 'auto',
    fontSize: 'var(--font-callout, 13px)',
    border: '1px solid var(--line-strong)',
    borderRadius: 'var(--radius-md, 6px)',
    background: 'var(--panel)',
    color: 'var(--text)',
  },
  '.cm-content': {
    minHeight: '72px',
    padding: '0.42rem 0.58rem',
    fontFamily: "'JetBrains Mono', 'Fira Code', 'SF Mono', 'Menlo', 'Consolas', monospace",
    caretColor: 'var(--text)',
  },
  '.cm-cursor': {
    borderLeftColor: 'var(--text)',
  },
  '.cm-activeLine': {
    backgroundColor: 'color-mix(in srgb, var(--text) 4%, transparent)',
  },
  '.cm-gutters': {
    background: 'transparent',
    color: 'var(--muted)',
    borderRight: '1px solid var(--line-strong)',
    minWidth: '2.2em',
  },
  '.cm-gutterElement': {
    padding: '0 0.3em 0 0.4em',
    fontSize: '0.85em',
  },
  '.cm-scroller': {
    overflow: 'auto',
    fontFamily: 'inherit',
  },
  '.cm-focused': {
    outline: 'none',
    borderColor: 'var(--accent, #4f46e5)',
  },
  '&.cm-editor.cm-focused': {
    outline: 'none',
  },
  '.cm-lintRange-error': {
    textDecoration: 'wavy underline var(--danger, #ef4444)',
  },
  '.cm-tooltip-lint': {
    background: 'var(--panel)',
    border: '1px solid var(--line-strong)',
    color: 'var(--danger, #ef4444)',
    borderRadius: 'var(--radius-sm, 4px)',
    padding: '2px 6px',
    fontSize: '0.85em',
  },
}, { dark: false });

export function CodeEditor({
  value,
  onChange,
  language,
  placeholder: placeholderText,
  readOnly = false,
  className,
}: CodeEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  const createExtensions = useCallback(() => {
    const exts = [
      editorTheme,
      EditorView.lineWrapping,
      lineNumbers(),
      highlightActiveLine(),
      highlightSpecialChars(),
      bracketMatching(),
      EditorState.readOnly.of(readOnly),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          onChangeRef.current(update.state.doc.toString());
        }
      }),
      EditorState.tabSize.of(2),
    ];

    if (placeholderText) {
      exts.push(cmPlaceholder(placeholderText));
    }

    if (language === 'json') {
      exts.push(json());
      exts.push(linter(jsonParseLinter()));
    }

    return exts;
  }, [language, placeholderText, readOnly]);

  // 初始化编辑器
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const state = EditorState.create({
      doc: value,
      extensions: createExtensions(),
    });

    const view = new EditorView({
      state,
      parent: container,
    });

    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // 仅在挂载时创建
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // readOnly / language 变更时重建编辑器
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;

    const currentDoc = view.state.doc.toString();
    view.destroy();
    viewRef.current = null;

    const container = containerRef.current;
    if (!container) return;

    const state = EditorState.create({
      doc: currentDoc,
      extensions: createExtensions(),
    });

    const newView = new EditorView({
      state,
      parent: container,
    });

    viewRef.current = newView;
  }, [createExtensions]);

  // 外部 value 同步（仅在内容不同时更新，避免光标跳动）
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;

    const currentDoc = view.state.doc.toString();
    if (currentDoc !== value) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: value },
      });
    }
  }, [value]);

  return (
    <div
      ref={containerRef}
      className={`flowgram-code-editor ${className ?? ''}`}
    />
  );
}
