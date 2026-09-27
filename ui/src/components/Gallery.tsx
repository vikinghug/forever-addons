import { useEffect, useRef, useState } from "react";
import type { Screenshot } from "../types";

/**
 * Screenshots lead the overview: one large frame, the author's caption, and a
 * strip to pick another. The frame opens a full-window viewer driven by the
 * arrow keys.
 */
export function Gallery({ shots }: { shots: Screenshot[] }) {
  const [index, setIndex] = useState(0);
  const [viewing, setViewing] = useState(false);
  const lead = useRef<HTMLButtonElement>(null);

  const shot = shots[Math.min(index, shots.length - 1)];
  if (!shot) return null;

  const step = (by: number) => setIndex((current) => (current + by + shots.length) % shots.length);
  const close = () => {
    setViewing(false);
    lead.current?.focus();
  };

  return (
    <div className="gallery">
      <button
        ref={lead}
        type="button"
        className="gallery-lead"
        onClick={() => setViewing(true)}
        aria-label={`View ${shot.title ?? "screenshot"} full size`}
      >
        <img src={shot.url} alt={shot.title ?? ""} />
        {shots.length > 1 && (
          <span className="gallery-count">
            {index + 1} / {shots.length}
          </span>
        )}
      </button>

      <Caption shot={shot} />

      {shots.length > 1 && (
        <div className="gallery-strip">
          {shots.map((entry, at) => (
            <button
              key={entry.url}
              type="button"
              className="gallery-thumb"
              aria-current={at === index}
              aria-label={entry.title ?? `Screenshot ${at + 1}`}
              onClick={() => setIndex(at)}
            >
              <img src={entry.thumbnail_url ?? entry.url} alt="" loading="lazy" />
            </button>
          ))}
        </div>
      )}

      {viewing && (
        <Viewer
          shot={shot}
          position={`${index + 1} / ${shots.length}`}
          canStep={shots.length > 1}
          onStep={step}
          onClose={close}
        />
      )}
    </div>
  );
}

function Caption({ shot }: { shot: Screenshot }) {
  if (!shot.title && !shot.description) return null;

  return (
    <p className="gallery-caption">
      {shot.title && <b>{shot.title}</b>}
      {shot.description && <span>{shot.description}</span>}
    </p>
  );
}

function Viewer({
  shot,
  position,
  canStep,
  onStep,
  onClose,
}: {
  shot: Screenshot;
  position: string;
  canStep: boolean;
  onStep: (by: number) => void;
  onClose: () => void;
}) {
  const closeButton = useRef<HTMLButtonElement>(null);

  useEffect(() => closeButton.current?.focus(), []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
      if (canStep && event.key === "ArrowLeft") onStep(-1);
      if (canStep && event.key === "ArrowRight") onStep(1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [canStep, onStep, onClose]);

  return (
    <div className="viewer" role="dialog" aria-modal="true" aria-label="Screenshot viewer">
      <div className="viewer-top">
        <span className="stat">{position}</span>
        <button ref={closeButton} type="button" className="btn" data-variant="quiet" onClick={onClose}>
          ✕ Close
        </button>
      </div>
      <div className="viewer-stage">
        {canStep && (
          <button type="button" className="btn viewer-nav" onClick={() => onStep(-1)} aria-label="Previous screenshot">
            ←
          </button>
        )}
        <img src={shot.url} alt={shot.title ?? ""} />
        {canStep && (
          <button type="button" className="btn viewer-nav" onClick={() => onStep(1)} aria-label="Next screenshot">
            →
          </button>
        )}
      </div>
      <Caption shot={shot} />
    </div>
  );
}
