import { memo, useMemo } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";

type MarkdownProps = {
  content: string;
  className?: string;
};

// Custom components for markdown rendering
const components: Components = {
  // Code blocks with syntax highlighting placeholder
  code: ({ className, children, ...props }) => {
    const isInline = !className;
    if (isInline) {
      return (
        <code className="oc-md__inline-code" {...props}>
          {children}
        </code>
      );
    }
    const language = className?.replace("language-", "") || "";
    return (
      <div className="oc-md__code-block">
        {language && <div className="oc-md__code-lang">{language}</div>}
        <pre className="oc-md__pre">
          <code className={className} {...props}>
            {children}
          </code>
        </pre>
      </div>
    );
  },
  // Links open externally
  a: ({ href, children, ...props }) => (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      className="oc-md__link"
      {...props}
    >
      {children}
    </a>
  ),
  // Paragraphs
  p: ({ children, ...props }) => (
    <p className="oc-md__p" {...props}>
      {children}
    </p>
  ),
  // Lists
  ul: ({ children, ...props }) => (
    <ul className="oc-md__ul" {...props}>
      {children}
    </ul>
  ),
  ol: ({ children, ...props }) => (
    <ol className="oc-md__ol" {...props}>
      {children}
    </ol>
  ),
  li: ({ children, ...props }) => (
    <li className="oc-md__li" {...props}>
      {children}
    </li>
  ),
  // Block quotes
  blockquote: ({ children, ...props }) => (
    <blockquote className="oc-md__blockquote" {...props}>
      {children}
    </blockquote>
  ),
  // Headers
  h1: ({ children, ...props }) => (
    <h1 className="oc-md__h1" {...props}>{children}</h1>
  ),
  h2: ({ children, ...props }) => (
    <h2 className="oc-md__h2" {...props}>{children}</h2>
  ),
  h3: ({ children, ...props }) => (
    <h3 className="oc-md__h3" {...props}>{children}</h3>
  ),
  h4: ({ children, ...props }) => (
    <h4 className="oc-md__h4" {...props}>{children}</h4>
  ),
};

export const Markdown = memo(function Markdown({ content, className }: MarkdownProps) {
  const plugins = useMemo(() => [remarkGfm], []);

  return (
    <div className={`oc-md ${className || ""}`}>
      <ReactMarkdown remarkPlugins={plugins} components={components}>
        {content}
      </ReactMarkdown>
    </div>
  );
});
