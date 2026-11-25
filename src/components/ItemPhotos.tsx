// The item's photos at the top of its page, beside Details: the oldest first, with arrows
// on hover to step through the rest. Selecting the picture opens the file.

import { Button } from "@heroui/react";
import { ChevronLeft, ChevronRight, ImageOff } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Attachment } from "../bindings/Attachment";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { useOpenAttachment } from "./Attachments";

/** Picture types the window can show (the backend checks the same list). */
const SHOWABLE = ["image/jpeg", "image/png", "image/gif", "image/webp", "image/bmp", "image/avif"];
/** The widest a photo box gets, relative to its height. */
const MAX_RATIO = 4 / 3;
/** Height when the photo sits above Details instead of beside it (narrow windows). */
const STACKED_HEIGHT = 240;

/**
 * Photos attached to the item itself (not to a service record), oldest first, that the
 * window can show.
 */
export function useItemPhotos(assetId: number, version: number): Attachment[] {
  const files = useAsync(() => api.listAttachments({ asset_id: assetId }), [assetId, version]);
  return (files.data ?? [])
    .filter(
      (a) =>
        a.asset_id === assetId &&
        a.category === "photo" &&
        a.calibration_id == null &&
        !!a.content_type &&
        SHOWABLE.includes(a.content_type),
    )
    .sort((a, b) => a.created_at.localeCompare(b.created_at) || a.id - b.id);
}

/** The element's current height, kept up to date as it resizes. */
export function useHeight(el: HTMLElement | null): number {
  const [height, setHeight] = useState(0);
  useLayoutEffect(() => {
    if (!el) return;
    const observer = new ResizeObserver(() => setHeight(el.getBoundingClientRect().height));
    observer.observe(el);
    setHeight(el.getBoundingClientRect().height);
    return () => observer.disconnect();
  }, [el]);
  return height;
}

function useWideLayout(): boolean {
  // Tailwind's xl breakpoint, where the photo, Details, and Calibration share a row.
  const query = "(min-width: 80rem)";
  const [wide, setWide] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const media = window.matchMedia(query);
    const onChange = () => setWide(media.matches);
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, []);
  return wide;
}

/** Loads a picture's bytes once and keeps a URL for it while the page is open. */
function usePictureUrls() {
  const urls = useRef(new Map<number, string>());
  const [, rerender] = useState(0);
  const [failed, setFailed] = useState<Record<number, string>>({});
  useEffect(
    () => () => {
      urls.current.forEach((u) => URL.revokeObjectURL(u));
      urls.current.clear();
    },
    [],
  );
  function load(photo: Attachment) {
    if (urls.current.has(photo.id) || failed[photo.id]) return;
    urls.current.set(photo.id, ""); // loading
    api
      .attachmentImage(photo.id)
      .then((bytes) => {
        const url = URL.createObjectURL(new Blob([bytes], { type: photo.content_type ?? "image/*" }));
        urls.current.set(photo.id, url);
        rerender((n) => n + 1);
      })
      .catch((e: unknown) => {
        urls.current.delete(photo.id);
        setFailed((f) => ({ ...f, [photo.id]: e instanceof Error ? e.message : String(e) }));
      });
  }
  return { url: (id: number) => urls.current.get(id) || undefined, failed, load };
}

export function ItemPhotos({ photos, detailsHeight }: { photos: Attachment[]; detailsHeight: number }) {
  const [index, setIndex] = useState(0);
  const [ratios, setRatios] = useState<Record<number, number>>({});
  const { url, failed, load } = usePictureUrls();
  const { open, dialog } = useOpenAttachment();
  const wide = useWideLayout();

  // Back to the oldest when the set of photos changes (one added or archived).
  const ids = photos.map((p) => p.id).join(",");
  useEffect(() => setIndex(0), [ids]);

  const photo = photos[Math.min(index, photos.length - 1)];
  useEffect(() => {
    if (photo) load(photo);
    // Load the next one too, so stepping through doesn't wait.
    const next = photos[(index + 1) % photos.length];
    if (next && next !== photo) load(next);
  });

  const height = wide ? detailsHeight : STACKED_HEIGHT;
  if (!photo || height <= 0) return null;
  const ratio = Math.min(ratios[photo.id] ?? MAX_RATIO, MAX_RATIO);
  const src = url(photo.id);
  const problem = failed[photo.id];
  const many = photos.length > 1;
  const step = (by: number) => setIndex((i) => (i + by + photos.length) % photos.length);

  return (
    <div
      className="group relative shrink-0 overflow-hidden rounded-3xl bg-surface shadow-surface"
      style={{ height, width: Math.round(height * ratio) }}
    >
      {problem ? (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-4 text-center text-xs text-muted">
          <ImageOff className="size-6" />
          <span>Couldn't show {photo.file_name}.</span>
          <span>{problem}</span>
        </div>
      ) : src ? (
        <button
          type="button"
          className="block h-full w-full cursor-zoom-in outline-none focus-visible:ring-2 focus-visible:ring-accent"
          onClick={() => void open(photo)}
          title={`Open ${photo.file_name}`}
        >
          <img
            src={src}
            alt={photo.description ?? photo.file_name}
            className="h-full w-full object-cover"
            onLoad={(e) => {
              const img = e.currentTarget;
              if (img.naturalHeight > 0) {
                setRatios((r) => ({ ...r, [photo.id]: img.naturalWidth / img.naturalHeight }));
              }
            }}
          />
        </button>
      ) : (
        <div className="h-full w-full animate-pulse bg-default" aria-label="Loading picture" />
      )}

      {many && (
        <>
          <Button
            isIconOnly
            size="sm"
            variant="secondary"
            aria-label="Previous picture"
            className="absolute top-1/2 left-2 -translate-y-1/2 opacity-0 shadow transition-opacity group-focus-within:opacity-100 group-hover:opacity-100"
            onPress={() => step(-1)}
          >
            <ChevronLeft className="size-4" />
          </Button>
          <Button
            isIconOnly
            size="sm"
            variant="secondary"
            aria-label="Next picture"
            className="absolute top-1/2 right-2 -translate-y-1/2 opacity-0 shadow transition-opacity group-focus-within:opacity-100 group-hover:opacity-100"
            onPress={() => step(1)}
          >
            <ChevronRight className="size-4" />
          </Button>
          <span className="pointer-events-none absolute right-2 bottom-2 rounded-full bg-black/60 px-2 py-0.5 text-[11px] text-white opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
            {index + 1} / {photos.length}
          </span>
        </>
      )}
      {dialog}
    </div>
  );
}
