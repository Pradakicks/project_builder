import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Components } from "react-markdown";

const components: Components = {
  p: ({ children }) => (
    <p className="text-xs text-gray-200 leading-relaxed mb-1.5">{children}</p>
  ),
  h1: ({ children }) => (
    <h1 className="text-sm font-bold text-gray-100 mb-1 mt-2">{children}</h1>
  ),
  h2: ({ children }) => (
    <h2 className="text-xs font-bold text-gray-100 mb-1 mt-2">{children}</h2>
  ),
  h3: ({ children }) => (
    <h3 className="text-xs font-semibold text-gray-200 mb-1 mt-1.5">{children}</h3>
  ),
  ul: ({ children }) => (
    <ul className="list-disc list-inside text-xs text-gray-200 mb-1.5 space-y-0.5 pl-1">{children}</ul>
  ),
  ol: ({ children }) => (
    <ol className="list-decimal list-inside text-xs text-gray-200 mb-1.5 space-y-0.5 pl-1">{children}</ol>
  ),
  li: ({ children }) => (
    <li className="text-xs text-gray-200 leading-relaxed">{children}</li>
  ),
  code: ({ children, className }) => {
    const isBlock = className?.includes("language-");
    if (isBlock) {
      return (
        <code className="text-[10px] font-mono text-gray-200">{children}</code>
      );
    }
    return (
      <code className="rounded bg-gray-900 px-1 py-0.5 text-[10px] font-mono text-blue-300">
        {children}
      </code>
    );
  },
  pre: ({ children }) => (
    <pre className="rounded bg-gray-900 p-1.5 text-[10px] font-mono overflow-x-auto mb-1.5">
      {children}
    </pre>
  ),
  a: ({ href, children }) => (
    <a href={href} className="text-blue-400 hover:underline" target="_blank" rel="noopener noreferrer">
      {children}
    </a>
  ),
  strong: ({ children }) => (
    <strong className="font-semibold text-gray-100">{children}</strong>
  ),
  em: ({ children }) => (
    <em className="italic text-gray-300">{children}</em>
  ),
  blockquote: ({ children }) => (
    <blockquote className="border-l-2 border-gray-600 pl-2 text-xs text-gray-400 italic mb-1.5">
      {children}
    </blockquote>
  ),
  table: ({ children }) => (
    <div className="overflow-x-auto mb-1.5">
      <table className="text-[10px] text-gray-200 border-collapse">{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border border-gray-700 bg-gray-800 px-1.5 py-0.5 text-left font-semibold">{children}</th>
  ),
  td: ({ children }) => (
    <td className="border border-gray-700 px-1.5 py-0.5">{children}</td>
  ),
};

export function Markdown({ content, className }: { content: string; className?: string }) {
  return (
    <div className={className ?? ""}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={components}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
