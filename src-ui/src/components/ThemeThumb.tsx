// 노트 스타일 썸네일 — 실제 테마 CSS로 렌더한 문서를 축소해 보여준다
// (scale 트릭: iframe을 480px로 렌더하고 width/480 배율로 접어 카드에 클립).
// 설정 → 노트 스타일 탭과 초기 설정(Step 4)이 공유한다.

import { buildThemedDoc } from "@/lib/markdownDoc";

/** 썸네일 미리보기용 샘플 마크다운 — 각 스타일의 제목/섹션/불릿 렌더를 보여준다. */
export const THEME_SAMPLE_MD =
  "# 노트 제목\n\n2026-07-05\n\n## 섹션 하나\n\n- 첫 번째 항목\n- 두 번째 항목\n- **강조** 포인트\n\n## 섹션 둘\n\n- 이어지는 내용\n";

export function ThemeThumb({ themeId, width = 144 }: { themeId: string; width?: number }) {
  const height = Math.round(width * 1.25); // 480:600 = 4:5 비율 유지
  const scale = width / 480;
  return (
    <div
      className="overflow-hidden rounded-lg border border-gray-200 bg-white relative pointer-events-none select-none"
      style={{ width, height }}
    >
      <iframe
        srcDoc={buildThemedDoc(THEME_SAMPLE_MD, themeId)}
        sandbox=""
        scrolling="no"
        tabIndex={-1}
        title={`theme-${themeId}`}
        className="absolute top-0 left-0 origin-top-left border-0"
        style={{ width: 480, height: 600, transform: `scale(${scale})` }}
      />
    </div>
  );
}
