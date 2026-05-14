import { useCallback, useEffect, useRef, useState } from 'react';
import { marked } from 'marked';

/// 自定义 renderer：代码块增加头部（语言标签 + 复制按钮）。
const renderer = {
  code({ text, lang }: { text: string; lang?: string }): string {
    const language = lang || '';
    const escapedCode = text
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;');
    const langLabel = language ? `<span class="copilot-code-lang">${language}</span>` : '';
    return (
      `<div class="copilot-code-block">` +
      `<div class="copilot-code-header">` +
      langLabel +
      `<button class="copilot-code-copy" type="button" title="复制代码">` +
      `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">` +
      `<rect x="5" y="5" width="9" height="9" rx="1.5"/>` +
      `<path d="M3 11V3a1.5 1.5 0 011.5-1.5H11"/>` +
      `</svg>` +
      `</button>` +
      `</div>` +
      `<pre><code>${escapedCode}</code></pre>` +
      `</div>`
    );
  },
};

marked.use({ breaks: true, gfm: true, renderer });

interface MarkdownContentProps {
  content: string;
  streaming?: boolean;
}

export function MarkdownContent({ content, streaming }: MarkdownContentProps) {
  const [html, setHtml] = useState('');
  const rafRef = useRef<number>(0);
  const contentRef = useRef(content);
  const containerRef = useRef<HTMLDivElement>(null);

  contentRef.current = content;

  const renderMarkdown = useCallback(() => {
    const text = contentRef.current;
    if (!text) {
      setHtml('');
      return;
    }
    try {
      setHtml(marked.parse(text) as string);
    } catch {
      setHtml(text);
    }
  }, []);

  useEffect(() => {
    if (streaming) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = requestAnimationFrame(renderMarkdown);
    } else {
      cancelAnimationFrame(rafRef.current);
      renderMarkdown();
    }
    return () => cancelAnimationFrame(rafRef.current);
  }, [content, streaming, renderMarkdown]);

  /// 事件委托：处理代码块复制按钮点击。
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const handleClick = async (e: MouseEvent) => {
      const target = (e.target as HTMLElement).closest<HTMLButtonElement>('.copilot-code-copy');
      if (!target) return;

      const block = target.closest('.copilot-code-block');
      const codeEl = block?.querySelector('code');
      if (!codeEl) return;

      const text = codeEl.textContent ?? '';
      try {
        await navigator.clipboard.writeText(text);
      } catch {
        const ta = document.createElement('textarea');
        ta.value = text;
        ta.style.position = 'fixed';
        ta.style.opacity = '0';
        document.body.appendChild(ta);
        ta.select();
        document.execCommand('copy');
        document.body.removeChild(ta);
      }

      target.classList.add('copilot-code-copy--done');
      target.title = '已复制';
      setTimeout(() => {
        target.classList.remove('copilot-code-copy--done');
        target.title = '复制代码';
      }, 1500);
    };

    container.addEventListener('click', handleClick);
    return () => container.removeEventListener('click', handleClick);
  }, [html]);

  if (!content) {
    return streaming ? <>{'...'}</> : null;
  }

  return <div ref={containerRef} className="copilot-md" dangerouslySetInnerHTML={{ __html: html }} />;
}
