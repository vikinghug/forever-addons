import DOMPurify from "dompurify";
import { marked } from "marked";
import { useMemo, type MouseEvent } from "react";
import type { Description } from "../types";

interface Props {
  description: Description;
  /** The addon's page on its source; site-relative links resolve against it. */
  pageUrl: string;
  /** The pane already shows the name, so a leading heading repeating it is dropped. */
  title: string;
  onOpen: (url: string) => void;
}

/**
 * Renders a source's HTML or Markdown description. The markup is written by
 * third parties, so it is sanitized, stripped of inline styling, and its links
 * open in the system browser instead of navigating the app's webview.
 */
export function DescriptionView({ description, pageUrl, title, onOpen }: Props) {
  const html = useMemo(
    () => (description.format === "plain" ? null : render(description, pageUrl, title)),
    [description, pageUrl, title],
  );

  if (html === null) return <p className="prose">{description.text}</p>;

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

function render(description: Description, pageUrl: string, title: string): string {
  const markup =
    description.format === "markdown"
      ? marked.parse(description.text, { async: false, gfm: true })
      : description.text;

  const body = DOMPurify.sanitize(markup, {
    RETURN_DOM: true,
    FORBID_TAGS: ["style", "form", "input", "button", "select", "textarea"],
    FORBID_ATTR: ["style", "class", "id", "width", "height", "align", "target"],
  }) as HTMLElement;

  resolveLinks(body, pageUrl);
  resolveImages(body, pageUrl);
  dropSpacers(body);
  dropTitle(body, title);
  return body.innerHTML;
}

function resolveLinks(body: HTMLElement, base: string) {
  for (const link of body.querySelectorAll("a")) {
    const href = resolve(link.getAttribute("href"), base);
    if (href) link.setAttribute("href", href);
    else link.removeAttribute("href");
  }
}

function resolveImages(body: HTMLElement, base: string) {
  for (const image of body.querySelectorAll("img")) {
    // The webview only loads images over https; most hosts serve both.
    const src = resolve(image.getAttribute("src"), base)?.replace(/^http:/, "https:");
    if (!src) {
      image.remove();
      continue;
    }
    image.setAttribute("src", src);
    image.setAttribute("loading", "lazy");
  }
}

/** Empty paragraphs and headings (`<p>&nbsp;</p>`) that sources use as spacing. */
function dropSpacers(body: HTMLElement) {
  for (const block of body.querySelectorAll("p, h1, h2, h3, h4, h5, h6")) {
    if (!block.textContent?.trim() && !block.querySelector("img")) block.remove();
  }
}

function dropTitle(body: HTMLElement, title: string) {
  const first = body.firstElementChild;
  if (
    first &&
    /^H[1-3]$/.test(first.tagName) &&
    first.textContent?.trim().toLowerCase() === title.trim().toLowerCase()
  ) {
    first.remove();
  }
}

/**
 * An absolute http(s) URL, or null for anything that should not open
 * (fragments, `javascript:`, `mailto:`). CurseForge wraps outbound links in
 * `/linkout?remoteUrl=…` with the target percent-encoded twice.
 */
function resolve(raw: string | null, base: string): string | null {
  if (!raw || raw.startsWith("#")) return null;

  try {
    const linkout = /\/linkout\?remoteUrl=([^&]+)/.exec(raw);
    const target = linkout ? decodeURIComponent(decodeURIComponent(linkout[1])) : raw;
    const url = new URL(target, base);
    return url.protocol === "https:" || url.protocol === "http:" ? url.href : null;
  } catch {
    return null;
  }
}
