import type { MouseEvent } from "react";

interface Props {
  /** Sanitized by the backend: allowlisted tags, absolute http(s) links. */
  html: string;
  onOpen: (url: string) => void;
}

/**
 * Renders an addon's description. Links open in the system browser instead
 * of navigating the app's webview.
 */
export function DescriptionView({ html, onOpen }: Props) {
  function openLink(event: MouseEvent<HTMLDivElement>) {
    const link = (event.target as Element).closest("a");
    if (!link) return;

    event.preventDefault();
    const href = link.getAttribute("href");
    if (href) onOpen(href);
  }

  return (
    <div
      className="prose description"
      onClick={openLink}
      onAuxClick={openLink}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
