import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ConversationMarkdown, safeMarkdownUrl } from "./conversationMarkdown";

describe("ConversationMarkdown", () => {
  it("renders rich Markdown without executable HTML or dangerous links", () => {
    const markdown = [
      "# Result",
      "",
      "| key | value |",
      "| --- | --- |",
      "| token | secret-123 |",
      "",
      "```ts",
      "const ready = true;",
      "```",
      "",
      "[unsafe](javascript:alert(1))",
      "[also unsafe](vbscript:msgbox(1))",
      "[data](data:text/html;base64,PHNjcmlwdD4=)",
      "[safe](https://example.com/docs)",
      "",
      '<iframe src="https://unsafe.invalid"></iframe>',
      '<script>alert("unsafe")</script>',
    ].join("\n");

    const html = renderToStaticMarkup(createElement(ConversationMarkdown, { markdown }));

    expect(html).toContain("<h1>Result</h1>");
    expect(html).toContain("<table>");
    expect(html).toContain("<pre><code class=\"language-ts\"");
    expect(html).toContain("secret-123");
    expect(html).toContain('href="https://example.com/docs"');
    expect(html).not.toContain("<iframe");
    expect(html).not.toContain("<script");
    expect(html).not.toContain('href="javascript:');
    expect(html).not.toContain('href="vbscript:');
    expect(html).not.toContain('href="data:');
  });

  it("allows only explicit navigation protocols", () => {
    expect(safeMarkdownUrl("https://example.com")).toBe("https://example.com");
    expect(safeMarkdownUrl("mailto:user@example.com")).toBe("mailto:user@example.com");
    expect(safeMarkdownUrl("#section")).toBe("#section");
    expect(safeMarkdownUrl("javascript:alert(1)")).toBe("");
    expect(safeMarkdownUrl("data:text/html,unsafe")).toBe("");
  });
});
