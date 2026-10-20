// Continuous PDF view. Pages render lazily when scrolled into view, at device
// pixel ratio for crisp text. Clicking a page places the signature stamp.
import type { PDFDocumentProxy, PDFPageProxy } from "pdfjs-dist";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { useApp, type Placement } from "../stores/app";

/** Stamp size in PDF points; must match src-tauri/src/stamp.rs. */
export const STAMP_W = 230;
export const STAMP_H = 95;

function Page({
  doc,
  index,
  zoom,
  fitWidth,
  placement,
  stamp,
  onPlace,
  onVisible,
}: {
  doc: PDFDocumentProxy;
  index: number;
  zoom: number;
  fitWidth: number;
  placement: Placement | null;
  stamp: (scale: number) => ReactNode;
  onPlace: (p: Placement) => void;
  onVisible: (page: number) => void;
}) {
  const holder = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [page, setPage] = useState<PDFPageProxy | null>(null);
  const [visible, setVisible] = useState(index < 2);

  useEffect(() => {
    let alive = true;
    doc.getPage(index + 1).then((p) => alive && setPage(p));
    return () => {
      alive = false;
    };
  }, [doc, index]);

  useEffect(() => {
    const el = holder.current;
    if (!el) return;
    const io = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) setVisible(true);
        if (entry.intersectionRatio > 0.5) onVisible(index + 1);
      },
      { rootMargin: "600px 0px", threshold: [0, 0.5] },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [index, onVisible]);

  // `zoom` is relative to fit-width: 1 means the page fills the available width.
  // No upper clamp: a small page (e.g. A6) must still fit, and the displayed size
  // is already bounded by fitWidth * zoom.
  const base = page?.getViewport({ scale: 1 });
  const scale = base ? Math.max(0.1, (fitWidth / base.width) * zoom) : 1;
  const viewport = page?.getViewport({ scale });

  useEffect(() => {
    if (!page || !visible || !canvas.current || !viewport) return;
    const ratio = window.devicePixelRatio || 1;
    const c = canvas.current;
    c.width = Math.floor(viewport.width * ratio);
    c.height = Math.floor(viewport.height * ratio);
    const task = page.render({
      canvas: c,
      viewport,
      transform: ratio !== 1 ? [ratio, 0, 0, ratio, 0, 0] : undefined,
    });
    task.promise.catch(() => {});
    return () => task.cancel();
  }, [page, visible, viewport?.width, viewport?.height]); // eslint-disable-line react-hooks/exhaustive-deps

  const width = viewport?.width ?? fitWidth * zoom;
  const height = viewport?.height ?? fitWidth * zoom * 1.294;
  const stampW = base ? (STAMP_W / base.width) * width : 0;
  const stampH = base ? (STAMP_H / base.height) * height : 0;
  const here = placement?.page === index + 1 ? placement : null;

  return (
    <div
      ref={holder}
      className="pdf-sheet"
      style={{ width, height }}
      onClick={(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        const w = stampW / r.width;
        const h = stampH / r.height;
        // Centre the stamp on the click, clamped inside the page.
        onPlace({
          page: index + 1,
          x: Math.min(Math.max((e.clientX - r.left) / r.width - w / 2, 0), 1 - w),
          y: Math.min(Math.max((e.clientY - r.top) / r.height - h / 2, 0), 1 - h),
        });
      }}
    >
      <canvas ref={canvas} style={{ width, height, display: "block" }} />
      {here && (
        <div
          className="stamp"
          style={{ left: here.x * width, top: here.y * height, width: stampW, height: stampH }}
          onClick={(e) => e.stopPropagation()}
          onPointerDown={(e) => {
            // Drag the stamp; position stays inside the page.
            e.stopPropagation();
            const el = e.currentTarget;
            el.setPointerCapture(e.pointerId);
            const sheet = el.parentElement!.getBoundingClientRect();
            const dx = e.clientX - el.getBoundingClientRect().left;
            const dy = e.clientY - el.getBoundingClientRect().top;
            const move = (ev: PointerEvent) =>
              onPlace({
                page: index + 1,
                x: Math.min(Math.max((ev.clientX - sheet.left - dx) / sheet.width, 0), 1 - stampW / sheet.width),
                y: Math.min(Math.max((ev.clientY - sheet.top - dy) / sheet.height, 0), 1 - stampH / sheet.height),
              });
            const up = () => {
              el.removeEventListener("pointermove", move);
              el.removeEventListener("pointerup", up);
            };
            el.addEventListener("pointermove", move);
            el.addEventListener("pointerup", up);
          }}
        >
          {stamp(scale)}
        </div>
      )}
      <div className="absolute -bottom-6 left-0 text-xs text-muted">Trang {index + 1}</div>
    </div>
  );
}

export function PdfViewer({
  doc,
  stamp,
  onPageChange,
  scrollTo,
}: {
  doc: PDFDocumentProxy;
  stamp: (scale: number) => ReactNode;
  onPageChange: (page: number) => void;
  scrollTo: number | null;
}) {
  const { zoom, placement, setPlacement } = useApp();
  const scroller = useRef<HTMLDivElement>(null);
  const [fitWidth, setFitWidth] = useState(800);

  // Measure before the first paint, then follow resizes, so a narrow pane never
  // shows pages at the 800px placeholder width.
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const fit = (w: number) => setFitWidth(Math.max(240, Math.min(w - 96, 1000)));
    fit(el.clientWidth);
    const ro = new ResizeObserver(() => fit(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    if (!scrollTo || !scroller.current) return;
    const el = scroller.current.querySelectorAll("[data-page]")[scrollTo - 1] as HTMLElement | undefined;
    el?.scrollIntoView({ behavior: "smooth", block: "start" });
  }, [scrollTo]);

  return (
    <div ref={scroller} className="pdf-canvas min-h-0 flex-1 overflow-auto">
      <div className="flex flex-col items-center gap-10 px-12 pt-8 pb-12">
        {Array.from({ length: doc.numPages }, (_, i) => (
          <div key={i} data-page={i + 1}>
            <Page
              doc={doc}
              index={i}
              zoom={zoom}
              fitWidth={fitWidth}
              placement={placement}
              stamp={stamp}
              onPlace={setPlacement}
              onVisible={onPageChange}
            />
          </div>
        ))}
      </div>
    </div>
  );
}
