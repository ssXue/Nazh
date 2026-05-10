import { useCallback, useEffect, useRef, useState } from 'react';
import { marked } from 'marked';

marked.setOptions({ breaks: true, gfm: true });

interface MarkdownContentProps {
  content: string;
  streaming?: boolean;
}

export function MarkdownContent({ content, streaming }: MarkdownContentProps) {
  const [html, setHtml] = useState('');
  const rafRef = useRef<number>(0);
  const contentRef = useRef(content);

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

  if (!content) {
    return streaming ? <>{'...'}</> : null;
  }

  return <div className="copilot-md" dangerouslySetInnerHTML={{ __html: html }} />;
}
