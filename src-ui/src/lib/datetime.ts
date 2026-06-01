// Time / date helpers carried over from old frontend/src/pages/MeetingsPage.
// Pure functions — no React state, no DOM. Tested by passing fixed Date instances.

export function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

export function todayIso(): string {
  const d = new Date();
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

export function dateKey(d: Date): string {
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

export function daysAgo(d: Date, base: Date): number {
  const a = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const b = new Date(base.getFullYear(), base.getMonth(), base.getDate()).getTime();
  return Math.round((b - a) / 86_400_000);
}

const WEEKDAYS_KO = ["일", "월", "화", "수", "목", "금", "토"];
const WEEKDAYS_EN = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

export function shortDate(d: Date, lang: "ko" | "en" = "ko"): string {
  if (lang === "en") return `${WEEKDAYS_EN[d.getDay()]} ${d.getMonth() + 1}/${d.getDate()}`;
  return `${d.getMonth() + 1}/${d.getDate()} ${WEEKDAYS_KO[d.getDay()]}`;
}

export function formatDateLong(now: Date, lang: "ko" | "en" = "ko"): string {
  if (lang === "en") {
    return now.toLocaleDateString("en-US", {
      year: "numeric",
      month: "long",
      day: "numeric",
      weekday: "long",
    });
  }
  return `${now.getFullYear()}년 ${now.getMonth() + 1}월 ${now.getDate()}일 ${WEEKDAYS_KO[now.getDay()]}요일`;
}

export function formatTime(iso: string | null): string {
  if (!iso) return "--:--";
  const d = new Date(iso);
  return `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
}

export function roundTo5Min(date: Date): Date {
  const d = new Date(date);
  d.setMinutes(Math.round(d.getMinutes() / 5) * 5, 0, 0);
  return d;
}

export function formatDateRangeLabel(from: string, to: string): string {
  const fmt = (s: string) => {
    if (!s) return "";
    const [, m, d] = s.split("-");
    return `${Number(m)}/${Number(d)}`;
  };
  if (from && to) {
    if (from === to) return fmt(from);
    return `${fmt(from)} ~ ${fmt(to)}`;
  }
  if (from) return `${fmt(from)} ~`;
  return `~ ${fmt(to)}`;
}

/**
 * Time-of-day greeting. 5–11 morning, 11–17 afternoon, 17–22 evening, else night.
 */
export function greetingFor(now: Date, name: string | null, lang: "ko" | "en" = "ko"): string {
  const h = now.getHours();
  if (lang === "en") {
    const period =
      h >= 5 && h < 11
        ? "Good morning"
        : h >= 11 && h < 17
          ? "Good afternoon"
          : h >= 17 && h < 22
            ? "Good evening"
            : "Working late";
    return name ? `${period}, ${name}` : period;
  }
  const period =
    h >= 5 && h < 11
      ? "좋은 아침이에요"
      : h >= 11 && h < 17
        ? "좋은 오후예요"
        : h >= 17 && h < 22
          ? "좋은 저녁이에요"
          : "늦은 시간이네요";
  return name ? `${period}, ${name}님` : period;
}
