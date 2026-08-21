import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

const SAFE_PROTOCOL = /^(https?:|mailto:)/i;

export function safeMarkdownUrl(url: string): string {
  const trimmed = url.trim();
  if (trimmed.startsWith("#") || SAFE_PROTOCOL.test(trimmed)) {
    return trimmed;
  }
  return "";
}

export function ConversationMarkdown({ markdown }: { markdown: string }) {
  return (
    <div className="conversation-markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        skipHtml
        urlTransform={safeMarkdownUrl}
        components={{
          a: ({ children, ...props }) => (
            <a {...props} target="_blank" rel="noreferrer noopener">
              {children}
            </a>
          ),
        }}
      >
        {markdown}
      </ReactMarkdown>
    </div>
  );
}
