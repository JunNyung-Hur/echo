import { useRef, useState } from "react";
import { Copy, Check } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

import { TranscriptBlock, matchTranscriptClassName } from "@/components/TranscriptViewerModal";
import { useT } from "@/i18n/LangContext";

// Assistant chat text rendered as markdown — ported 1:1 from the old
// MeetingChatPanel. Server responses (and spliced transcript / worker
// messages) routinely include **bold**, lists, links, and ``` fenced blocks;
// plain text leaked the asterisks. Limited surface — no headings (chat is not
// a document).

function CodeBlock({ children }: { children: React.ReactNode }) {
  const t = useT();
  const preRef = useRef<HTMLPreElement | null>(null);
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    const text = preRef.current?.textContent ?? "";
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // clipboard may be unavailable in some contexts — fail silent.
    }
  };

  return (
    <div className="relative my-2 group">
      <pre
        ref={preRef}
        className="p-3 rounded-md bg-gray-50 border border-gray-200 overflow-x-auto text-[12px] font-mono leading-relaxed text-gray-800 whitespace-pre-wrap [&>code]:!bg-transparent [&>code]:!p-0 [&>code]:!rounded-none [&>code]:!text-inherit [&>code]:!font-mono"
      >
        {children}
      </pre>
      <button
        type="button"
        onClick={handleCopy}
        title={copied ? t("md.copied") : t("md.copy")}
        aria-label={copied ? t("md.copied") : t("md.copy")}
        className="absolute top-1.5 right-1.5 p-1.5 rounded-md bg-white/90 border border-gray-200 text-gray-500 hover:text-gray-900 hover:bg-white backdrop-blur-sm shadow-sm opacity-0 group-hover:opacity-100 focus:opacity-100 transition-opacity cursor-pointer"
      >
        {copied ? <Check size={14} className="text-sky-600" /> : <Copy size={14} />}
      </button>
    </div>
  );
}

export default function AssistantMarkdown({ text }: { text: string }) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm]}
      components={{
        p: ({ children }) => (
          <p className="text-gray-800 text-sm whitespace-pre-wrap leading-relaxed">{children}</p>
        ),
        strong: ({ children }) => <strong className="font-semibold">{children}</strong>,
        em: ({ children }) => <em className="italic">{children}</em>,
        // ```transcript-<uuid> from read_transcript → TranscriptBlock (1K
        // preview + 전체보기 modal). Otherwise the normal CodeBlock. (1f207ab)
        pre: ({ children, node }) => {
          const codeNode = (
            node as
              | { children?: Array<{ properties?: { className?: string[] }; children?: Array<{ value?: string }> }> }
              | undefined
          )?.children?.[0];
          const className = codeNode?.properties?.className?.[0];
          const transcriptId = matchTranscriptClassName(className);
          if (transcriptId) {
            const previewText = codeNode?.children?.[0]?.value ?? "";
            return <TranscriptBlock transcriptId={transcriptId} previewText={previewText} />;
          }
          return <CodeBlock>{children}</CodeBlock>;
        },
        code: ({ children }) => (
          <code className="px-1 py-0.5 rounded bg-gray-100 text-[12px] font-mono">{children}</code>
        ),
        ul: ({ children }) => (
          <ul className="list-disc pl-5 text-gray-800 text-sm leading-relaxed">{children}</ul>
        ),
        ol: ({ children }) => (
          <ol className="list-decimal pl-5 text-gray-800 text-sm leading-relaxed">{children}</ol>
        ),
        a: ({ children, href }) => (
          <a href={href} className="text-sky-700 underline" target="_blank" rel="noreferrer">
            {children}
          </a>
        ),
      }}
    >
      {text}
    </ReactMarkdown>
  );
}
