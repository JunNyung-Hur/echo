import { useState, useEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { UploadCloud } from "lucide-react";
import { useT } from "@/i18n/LangContext";

const AUDIO_EXTS = ["mp3", "wav", "m4a", "webm", "ogg", "flac", "aac", "mp4", "mov", "mkv"];

/**
 * F-REC-004 — file import dropzone. Unlike the old web app (HTML `<input
 * type=file>` → multipart upload), this is a desktop app: we get a real
 * filesystem path (via the dialog plugin or Tauri's native drag-drop, which —
 * unlike HTML5 drop — exposes paths) and hand it to the backend, which converts
 * + kicks the transcribe chain. Visual design carried over from the old
 * FileUploader (dashed dropzone + in-flight spinner).
 */
export default function FileUploader({
  onSelect,
  disabled = false,
}: {
  onSelect: (path: string) => Promise<void>;
  disabled?: boolean;
}) {
  const t = useT();
  const [isDragging, setIsDragging] = useState(false);
  const [importing, setImporting] = useState(false);
  const inactive = disabled || importing;

  const handle = async (path: string) => {
    setImporting(true);
    try {
      await onSelect(path);
    } finally {
      setImporting(false);
    }
  };

  // Tauri's native drag-drop gives real filesystem paths (HTML5 drop in a
  // WebView does not). It's a webview-global event; this component only mounts
  // in the `before` stage, so it's scoped to when import is actually offered.
  useEffect(() => {
    if (inactive) return;
    let un: (() => void) | undefined;
    let alive = true;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        if (e.payload.type === "drop") {
          setIsDragging(false);
          const p = e.payload.paths?.[0];
          if (p) handle(p);
        } else if (e.payload.type === "leave") {
          setIsDragging(false);
        } else {
          setIsDragging(true); // enter / over
        }
      })
      .then((f) => {
        if (alive) un = f;
        else f();
      });
    return () => {
      alive = false;
      un?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [inactive]);

  const pick = async () => {
    if (inactive) return;
    const sel = await open({
      multiple: false,
      filters: [{ name: t("uploader.audio"), extensions: AUDIO_EXTS }],
    });
    if (typeof sel === "string") handle(sel);
  };

  if (importing) {
    return (
      <div className="border-2 border-dashed border-sky-200 bg-sky-50/40 rounded-lg p-4 flex flex-col items-center gap-2.5">
        <div className="flex items-center gap-2">
          <div className="w-4 h-4 border-2 border-sky-500 border-t-transparent rounded-full animate-spin" />
          <p className="text-sm text-gray-700">{t("uploader.importing")}</p>
        </div>
        <div className="w-full h-1.5 bg-sky-100 rounded-full overflow-hidden">
          <div
            className="h-full w-2/5 bg-sky-600 rounded-full"
            style={{ animation: "indeterminate 1.1s ease-in-out infinite" }}
          />
        </div>
      </div>
    );
  }

  return (
    <div
      onClick={pick}
      className={`border-2 border-dashed rounded-lg p-4 text-center transition-colors ${
        isDragging
          ? "border-sky-600 bg-sky-50 cursor-pointer"
          : "border-gray-300 hover:border-gray-400 cursor-pointer"
      }`}
    >
      <UploadCloud className="w-6 h-6 mx-auto mb-1.5 text-gray-500" strokeWidth={1.5} />
      <p className="text-sm text-gray-500">{t("uploader.dropHint")}</p>
    </div>
  );
}
