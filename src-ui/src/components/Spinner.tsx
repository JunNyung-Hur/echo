import { useT } from "@/i18n/LangContext";

/**
 * The app's standard ring spinner — sky-300 ring with a sky-600 leading edge.
 * Matches the original frontend's loading indicator (NOT lucide Loader2).
 * Size via `className` (e.g. "w-6 h-6", "w-10 h-10").
 */
export default function Spinner({ className = "w-10 h-10" }: { className?: string }) {
  const t = useT();
  return (
    <div
      role="status"
      aria-label={t("spinner.loading")}
      className={`${className} border-[3px] border-sky-300 border-t-sky-600 rounded-full animate-spin`}
    />
  );
}
