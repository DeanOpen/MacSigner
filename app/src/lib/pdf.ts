// pdf.js loader: the library and its worker load only when a PDF is opened.
import type { PDFDocumentProxy } from "pdfjs-dist";

let lib: Promise<typeof import("pdfjs-dist")> | null = null;

function pdfjs() {
  lib ??= Promise.all([import("pdfjs-dist"), import("pdfjs-dist/build/pdf.worker.min.mjs?url")]).then(
    ([mod, worker]) => {
      mod.GlobalWorkerOptions.workerSrc = worker.default;
      return mod;
    },
  );
  return lib;
}

export async function openPdf(data: ArrayBuffer): Promise<PDFDocumentProxy> {
  const mod = await pdfjs();
  return mod.getDocument({ data: new Uint8Array(data) }).promise;
}
